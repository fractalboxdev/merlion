/**
 * mermaid 12.0.0 baselines (specs/benchmark.md#baselines), run in headless
 * Chromium through Playwright. The full mermaid bundle includes both dagre and
 * ELK, so one bundle serves both baselines; the adapter differs only in the
 * site-wide `layout`. Labels are drawn as `<text>` (`htmlLabels: false`) so
 * the SVG can be read without a DOM.
 */
import { Duration, Effect, Layer, Ref } from "effect";
import { createRequire } from "node:module";
import { type Browser, chromium, type Page } from "playwright";
import { MERMAID_DIST } from "../paths.ts";
import { failed, RENDER_TIMEOUT, type RenderOutcome, Renderer } from "./Renderer.ts";

const require = createRequire(import.meta.url);
const MERMAID_VERSION: string = (require("mermaid/package.json") as { version: string }).version;

type Layout = "dagre" | "elk";

const openPage = (browser: Browser, layout: Layout): Effect.Effect<Page, Error> =>
  Effect.tryPromise({
    try: async () => {
      const page = await browser.newPage();
      await page.setContent("<!doctype html><html><head><meta charset='utf-8'></head><body></body></html>");
      await page.addScriptTag({ path: MERMAID_DIST });
      await page.evaluate((layout) => {
        const m = (globalThis as unknown as { mermaid: { initialize: (c: unknown) => void } }).mermaid;
        m.initialize({
          startOnLoad: false,
          layout,
          htmlLabels: false,
          flowchart: { htmlLabels: false },
          securityLevel: "strict",
          suppressErrorRendering: true,
          deterministicIds: true,
        });
      }, layout);
      return page;
    },
    catch: (e) => (e instanceof Error ? e : new Error(String(e))),
  });

interface PageResult {
  readonly svg: string | null;
  readonly error: string | null;
  readonly ms: number;
}

const renderInPage = (page: Page, source: string, seq: number): Effect.Effect<PageResult, Error> =>
  Effect.tryPromise({
    try: () =>
      page.evaluate(
        async ({ source, seq }) => {
          const m = (globalThis as unknown as { mermaid: { render: (id: string, s: string) => Promise<{ svg: string }> } })
            .mermaid;
          const id = `d${seq}`;
          const t0 = performance.now();
          try {
            const r = await m.render(id, source);
            return { svg: r.svg, error: null, ms: performance.now() - t0 };
          } catch (e) {
            // mermaid leaves its temporary container behind on failure.
            document.getElementById(`d${id}`)?.remove();
            document.getElementById(id)?.remove();
            return { svg: null, error: e instanceof Error ? e.message : String(e), ms: performance.now() - t0 };
          }
        },
        { source, seq },
      ),
    catch: (e) => (e instanceof Error ? e : new Error(String(e))),
  });

const make = (layout: Layout) =>
  Effect.gen(function* () {
    const browser = yield* Effect.acquireRelease(
      Effect.tryPromise({ try: () => chromium.launch(), catch: (e) => new Error(`chromium launch failed: ${String(e)}`) }),
      (b) => Effect.promise(() => b.close()),
    );
    const pageRef = yield* Ref.make(yield* openPage(browser, layout));
    const seq = yield* Ref.make(0);

    const render = (source: string): Effect.Effect<RenderOutcome> =>
      Effect.gen(function* () {
        const n = yield* Ref.getAndUpdate(seq, (k) => k + 1);
        const page = yield* Ref.get(pageRef);
        return yield* renderInPage(page, source, n).pipe(
          Effect.timeout(RENDER_TIMEOUT),
          Effect.map((r): RenderOutcome => ({ svg: r.svg, error: r.error, ms: r.ms, fuelUsed: null })),
          Effect.catchTag("TimeoutException", () =>
            // The page may be stuck in layout: replace it before the next diagram.
            Effect.promise(() => page.close()).pipe(
              Effect.andThen(openPage(browser, layout)),
              Effect.flatMap((p) => Ref.set(pageRef, p)),
              Effect.as(failed(`timeout after ${Duration.format(RENDER_TIMEOUT)}`, Duration.toMillis(RENDER_TIMEOUT))),
              Effect.catchAll((e) => Effect.succeed(failed(`page reset failed: ${e.message}`))),
            ),
          ),
          Effect.catchAll((e) => Effect.succeed(failed(e.message))),
        );
      });

    return Renderer.of({ name: `mermaid-${layout}`, version: `mermaid ${MERMAID_VERSION} (${layout})`, render });
  });

export const MermaidDagreLive = Layer.scoped(Renderer, make("dagre"));
export const MermaidElkLive = Layer.scoped(Renderer, make("elk"));
