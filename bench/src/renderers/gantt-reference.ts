import { Context, Duration, Effect, Layer, Ref } from "effect";
import { createRequire } from "node:module";
import { chromium, type Browser, type Page } from "playwright";
import { MERMAID_DIST } from "../paths.ts";
import type { GanttTask } from "../svg/gantt.ts";
import { RENDER_TIMEOUT, type RenderOutcome, failed } from "./Renderer.ts";

const mermaidVersion = (createRequire(import.meta.url)("mermaid/package.json") as { version: string }).version;

export interface GanttReferenceOutcome extends RenderOutcome { readonly tasks: readonly GanttTask[] }
export class GanttReference extends Context.Tag("bench/GanttReference")<GanttReference, {
  readonly browserVersion: string;
  readonly render: (source: string) => Effect.Effect<GanttReferenceOutcome>;
}>() {}

const openPage = (browser: Browser) => Effect.tryPromise({
  try: async () => {
    const page = await browser.newPage({ timezoneId: "UTC" });
    await page.setContent("<!doctype html><html><head><meta charset='utf-8'></head><body></body></html>");
    await page.addScriptTag({ path: MERMAID_DIST });
    await page.evaluate(() => {
      (globalThis as unknown as { mermaid: { initialize: (c: unknown) => void } }).mermaid.initialize({
        startOnLoad: false, securityLevel: "strict", suppressErrorRendering: true, deterministicIds: true,
      });
    });
    return page;
  },
  catch: (e) => new Error(String(e)),
});

const draw = (page: Page, source: string, sequence: number) => Effect.tryPromise({
  try: () => page.evaluate(async ({ source, sequence }): Promise<GanttReferenceOutcome> => {
    interface RawTask { id: string; task: string; type: string; startTime: Date; endTime: Date; active?: boolean; done?: boolean; crit?: boolean; milestone?: boolean; vert?: boolean }
    const m = (globalThis as unknown as { mermaid: {
      render: (id: string, source: string) => Promise<{ svg: string }>;
      mermaidAPI: { getDiagramFromText: (s: string) => Promise<{ db: { getTasks: () => RawTask[] } }> };
    } }).mermaid;
    const id = `gantt${sequence}`;
    const t0 = performance.now();
    try {
      const { svg } = await m.render(id, source);
      const diagram = await m.mermaidAPI.getDiagramFromText(source);
      const doc = new DOMParser().parseFromString(svg, "image/svg+xml");
      const tasks = diagram.db.getTasks().map((t): GanttTask => {
        const mark = doc.getElementById(`${id}-${t.id}`);
        const text = doc.getElementById(`${id}-${t.id}-text`);
        return { id: t.id, label: text?.textContent ?? "", section: t.type ?? "", start: Number(t.startTime), end: Number(t.endTime),
          flags: (["active", "done", "crit", "milestone", "vert"] as const).filter((f) => t[f] === true).sort(),
          drawn: mark !== null && text !== null && !/NaN|Infinity/.test(["x", "y", "width", "height"].map((a) => mark.getAttribute(a)).join(" ")) };
      });
      return { svg, error: null, ms: performance.now() - t0, fuelUsed: null, tasks };
    } catch (e) {
      document.getElementById(id)?.remove();
      document.getElementById(`d${id}`)?.remove();
      return { svg: null, error: e instanceof Error ? e.message : String(e), ms: performance.now() - t0, fuelUsed: null, tasks: [] };
    }
  }, { source, sequence }),
  catch: (e) => new Error(String(e)),
});

export const GanttReferenceLive = Layer.scoped(GanttReference, Effect.gen(function* () {
  if (mermaidVersion !== "12.0.0") return yield* Effect.fail(new Error(`Gantt reference requires pinned Mermaid 12.0.0; found ${mermaidVersion}`));
  const browser = yield* Effect.acquireRelease(
    Effect.tryPromise({
      try: () => chromium.launch(process.env.MERLION_BENCH_CHROMIUM ? { executablePath: process.env.MERLION_BENCH_CHROMIUM } : {}),
      catch: (e) => new Error(String(e)),
    }),
    (b) => Effect.promise(() => b.close()),
  );
  const page = yield* Ref.make(yield* openPage(browser));
  const sequence = yield* Ref.make(0);
  return { browserVersion: browser.version(), render: (source: string) => Effect.gen(function* () {
    const n = yield* Ref.getAndUpdate(sequence, (n) => n + 1);
    const p = yield* Ref.get(page);
    return yield* draw(p, source, n).pipe(
      Effect.timeout(RENDER_TIMEOUT),
      Effect.catchTag("TimeoutException", () => Effect.promise(() => p.close()).pipe(
        Effect.andThen(openPage(browser)), Effect.flatMap((p) => Ref.set(page, p)),
        Effect.as({ ...failed(`timeout after ${Duration.format(RENDER_TIMEOUT)}`), tasks: [] }),
      )),
      Effect.catchAll((e) => Effect.succeed({ ...failed(e.message), tasks: [] })),
    );
  }) };
}));
