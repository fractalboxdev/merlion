/**
 * `bench parity`: the stylesheet parity gate of specs/benchmark.md.
 *
 * For every compat diagram plus the role fixtures in `fixtures/roles/`, and every theme of
 * `fixtures/parity.css` (compiled with `merlion css --strict`), the reference is the plain
 * SVG inlined in Chromium with the compiled CSS linked and `data-theme` on the host. Three
 * targets must match it:
 *
 *   1. the baked SVG (`render --css --theme`) inlined with no host CSS;
 *   2. the baked SVG with its `<style>` removed (presentation attributes only);
 *   3. the baked SVG rasterised by rsvg-convert, compared by pixels sampled inside each
 *      shape, stroke and marker against a screenshot of the reference (skipped when
 *      rsvg-convert is not on PATH).
 *
 * Chromium's computed fill and stroke are normalised to 8-bit sRGB through a canvas
 * `fillStyle` round trip and compared to ±1 per channel, with dash arrays and the
 * opacity product. Also checks that every named theme of `merlion-themes.css` bakes with
 * no warning.
 */
import { Command, FileSystem, Path } from "@effect/platform";
import { Console, Effect, Option, Schema } from "effect";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { chromium, type Page } from "playwright";
import { COMPAT_DIR, MERLION_BIN, REPO_DIR } from "../paths.ts";
import {
  type Comparison,
  compareElements,
  comparePixels,
  type ElementRecord,
  type Mismatch,
  normaliseDash,
  type PixelSample,
  type Rgba,
  sameColour,
  themesOf,
  uniformAt,
} from "./compare.ts";
import { decodePng, type Raster } from "./png.ts";

export class ParityFailed extends Schema.TaggedError<ParityFailed>()("ParityFailed", {
  message: Schema.String,
}) {}

/** Device pixels per CSS pixel for the screenshot and the rsvg-convert raster. */
const SCALE = 2;
const TOLERANCE = 1;
const CHUNK = 20;
const HOST = "http://parity.test";

const PARITY_CSS = "bench/fixtures/parity.css";
const THEMES_CSS = "packages/merlion-themes/merlion-themes.css";
const ROLES_DIR = "bench/fixtures/roles";

interface ProbeElement {
  readonly index: number;
  readonly tag: string;
  readonly kind: string;
  readonly label: string;
  readonly fill: Rgba | null;
  readonly stroke: Rgba | null;
  readonly strokeWidth: number;
  readonly dash: string;
  readonly opacity: number;
}

interface ProbeSample {
  readonly index: number;
  readonly paint: "fill" | "stroke";
  /** CSS pixels relative to the diagram's container. */
  readonly x: number;
  readonly y: number;
}

interface ProbeDiagram {
  readonly name: string;
  readonly svgId: string;
  readonly elements: readonly ProbeElement[];
  readonly samples: readonly ProbeSample[];
}

const toRecord = (e: ProbeElement): ElementRecord => ({
  index: e.index,
  tag: e.tag,
  kind: e.kind,
  label: e.label,
  props: { fill: e.fill, stroke: e.stroke, dash: normaliseDash(e.dash), opacity: Math.round(e.opacity * 1000) / 1000 },
});

export interface TargetTally {
  compared: number;
  mismatches: (Mismatch & { diagram: string; theme: string })[];
}

const tally = (): TargetTally => ({ compared: 0, mismatches: [] });

const add = (t: TargetTally, c: Comparison, diagram: string, theme: string) => {
  t.compared += c.compared;
  for (const m of c.mismatches) t.mismatches.push({ ...m, diagram, theme });
};

const viewBox = (svg: string): { w: number; h: number } => {
  const m = /viewBox="[-\d.]+ [-\d.]+ ([\d.]+) ([\d.]+)"/.exec(svg);
  return { w: Number(m?.[1] ?? 100), h: Number(m?.[2] ?? 100) };
};

const withoutStyle = (svg: string): string => svg.replace(/<style>[\s\S]*?<\/style>/, "");

const pageHtml = (theme: string | null, linkCss: boolean, diagrams: readonly { name: string; svg: string }[]): string => {
  let top = 0;
  const blocks = diagrams.map(({ name, svg }) => {
    const { w, h } = viewBox(svg);
    const block = `<div class="parity-d" data-name="${name}" style="top:${top}px;width:${Math.ceil(w)}px;height:${Math.ceil(h)}px">${svg}</div>`;
    top += Math.ceil(h) + 16;
    return block;
  });
  return [
    "<!doctype html>",
    theme === null ? "<html>" : `<html data-theme="${theme}">`,
    "<head><meta charset='utf-8'>",
    linkCss ? `<link rel="stylesheet" href="${HOST}/page.css">` : "",
    "<style>html,body{margin:0;background:#fff}.parity-d{position:absolute;left:0}.parity-d>svg{display:block}</style>",
    "</head><body>",
    ...blocks,
    "</body></html>",
  ].join("\n");
};

const PROBE = readFileSync(new URL("./probe.js", import.meta.url), "utf8");

const load = async (page: Page, html: string, css: string, size: { width: number; height: number }): Promise<void> => {
  await page.setViewportSize({ width: Math.max(1600, size.width), height: Math.max(1200, size.height) });
  await page.unroute("**/*");
  await page.route(`${HOST}/**`, (route) => {
    const url = route.request().url();
    if (url.endsWith("/page.css")) return route.fulfill({ contentType: "text/css", body: css });
    return route.fulfill({ contentType: "text/html", body: html });
  });
  await page.goto(`${HOST}/index.html`, { waitUntil: "load" });
  await page.addScriptTag({ content: PROBE });
};

const probe = (page: Page, withSamples: boolean): Promise<ProbeDiagram[]> =>
  page.evaluate((w) => (window as unknown as { parityProbe: (w: boolean) => ProbeDiagram[] }).parityProbe(w), withSamples);

export interface ParityResult {
  readonly diagrams: number;
  readonly themes: readonly string[];
  readonly elements: number;
  readonly notRendered: readonly string[];
  readonly baked: TargetTally;
  readonly attributes: TargetTally;
  readonly rsvg:
    | (TargetTally & { sampledElements: number; unsampledElements: number; rejectedSamples: number; unsampledByKind: Record<string, number> })
    | null;
  readonly themesCssBaked: readonly string[];
}

export const summarise = (r: ParityResult): string => {
  const line = (name: string, t: TargetTally, unit: string) => `  ${name}: ${t.compared} ${unit}, ${t.mismatches.length} mismatches`;
  return [
    `parity: ${r.diagrams} diagrams x ${r.themes.length} themes (${r.themes.join(", ")}), ${r.elements} reference elements, ${r.notRendered.length} not rendered`,
    line("baked, no host CSS", r.baked, "properties"),
    line("baked, <style> removed", r.attributes, "properties"),
    r.rsvg === null
      ? "  rsvg-convert: skipped (not on PATH)"
      : `${line("rsvg-convert", r.rsvg, "pixel samples")} (${r.rsvg.sampledElements} elements sampled, ${r.rsvg.unsampledElements} shapes/strokes/markers without a clear sample, ${r.rsvg.rejectedSamples} samples rejected as occluded or antialiased)`,
    `  merlion-themes.css: ${r.themesCssBaked.length} named themes bake with no warning (${r.themesCssBaked.join(", ")})`,
  ].join("\n");
};

export const parity = (opts: { readonly limit: Option.Option<number>; readonly requireRsvg: boolean }) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const fail = (message: string) => new ParityFailed({ message });
    if (!(yield* fs.exists(MERLION_BIN))) return yield* fail("target/release/merlion not built (cargo build --release -p merlion-cli)");

    const run = (...args: string[]) =>
      Command.make(MERLION_BIN, ...args).pipe(Command.workingDirectory(REPO_DIR), Command.stderr("inherit"), Command.exitCode);
    const runCapture = (...args: string[]) =>
      Effect.sync(() => {
        const r = spawnSync(MERLION_BIN, args, { cwd: REPO_DIR, encoding: "utf8" });
        return { stderr: r.stderr ?? "", code: r.status ?? 1 };
      });

    const work = path.join(REPO_DIR, "target", "parity");
    const rel = (p: string) => path.relative(REPO_DIR, p);
    yield* fs.remove(work, { recursive: true }).pipe(Effect.ignore);
    yield* fs.makeDirectory(path.join(work, "src"), { recursive: true });

    // Inputs: the compat corpus plus the role fixtures, in one directory for --batch.
    const compat = (yield* fs.readDirectory(COMPAT_DIR)).filter((f) => f.endsWith(".mmd")).sort();
    const roles = (yield* fs.readDirectory(path.join(REPO_DIR, ROLES_DIR))).filter((f) => f.endsWith(".mmd")).sort();
    let names: string[] = [];
    for (const f of roles) {
      yield* fs.copyFile(path.join(REPO_DIR, ROLES_DIR, f), path.join(work, "src", `roles-${f}`));
      names.push(`roles-${f.slice(0, -4)}`);
    }
    for (const f of compat) {
      yield* fs.copyFile(path.join(COMPAT_DIR, f), path.join(work, "src", f));
      names.push(f.slice(0, -4));
    }
    names = Option.match(opts.limit, { onNone: () => names, onSome: (n) => names.slice(0, n) });

    // The gate's stylesheet compiles with no dropped declaration.
    const pageCssPath = path.join(work, "page.css");
    if ((yield* run("css", "--strict", PARITY_CSS, "-o", rel(pageCssPath))) !== 0) {
      return yield* fail(`${PARITY_CSS} does not compile under --strict`);
    }
    const pageCss = yield* fs.readFileString(pageCssPath);
    const themes = themesOf(pageCss);

    // Every named theme of merlion-themes.css bakes with no warning.
    if ((yield* run("css", "--strict", THEMES_CSS, "-o", rel(path.join(work, "themes.css")))) !== 0) {
      return yield* fail(`${THEMES_CSS} does not compile under --strict`);
    }
    const themesCssBaked: string[] = [];
    for (const t of themesOf(yield* fs.readFileString(path.join(work, "themes.css")))) {
      if (t === null) continue;
      const probeSrc = path.join(REPO_DIR, ROLES_DIR, roles[0] ?? "");
      const { stderr, code } = yield* runCapture("render", rel(probeSrc), "-o", rel(path.join(work, `themes-${t}.svg`)), "--no-hint", "--css", THEMES_CSS, "--theme", t);
      const stylesheetWarnings = stderr.split("\n").filter((l) => l.startsWith(THEMES_CSS) && / (warning|error) /.test(l));
      if (code !== 0 || stylesheetWarnings.length > 0) {
        return yield* fail(`merlion-themes.css theme ${t} does not bake cleanly: ${stylesheetWarnings.join("; ") || `exit ${code}`}`);
      }
      themesCssBaked.push(t);
    }

    // Renders: plain once, baked once per theme. Exit 1 only means some diagrams failed.
    const batch = (out: string, ...extra: string[]) =>
      Effect.sync(() => spawnSync(MERLION_BIN, ["render", "--batch", rel(path.join(work, "src")), "-o", rel(out), "--no-hint", ...extra], { cwd: REPO_DIR, stdio: "ignore" }).status);
    const plainDir = path.join(work, "plain");
    yield* batch(plainDir);
    const themeKey = (t: string | null) => t ?? "root";
    for (const t of themes) {
      yield* batch(path.join(work, `baked-${themeKey(t)}`), "--css", PARITY_CSS, ...(t === null ? [] : ["--theme", t]));
    }

    const readSvg = (dir: string, name: string) =>
      Effect.gen(function* () {
        const f = path.join(dir, `${name}.svg`);
        return (yield* fs.exists(f)) ? Option.some(yield* fs.readFileString(f)) : Option.none<string>();
      });

    const rsvgAvailable = (yield* Command.make("which", "rsvg-convert").pipe(Command.stdout("pipe"), Command.exitCode, Effect.orElseSucceed(() => 1))) === 0;

    if (!rsvgAvailable && opts.requireRsvg) return yield* fail("rsvg-convert is not on PATH (--require-rsvg)");

    const result: ParityResult = {
      diagrams: 0,
      themes: themes.map(themeKey),
      elements: 0,
      notRendered: [],
      baked: tally(),
      attributes: tally(),
      rsvg: rsvgAvailable ? { ...tally(), sampledElements: 0, unsampledElements: 0, rejectedSamples: 0, unsampledByKind: {} } : null,
      themesCssBaked,
    };
    const mutable = result as { -readonly [K in keyof ParityResult]: ParityResult[K] };
    const notRendered = new Set<string>();

    const browser = yield* Effect.acquireRelease(
      Effect.tryPromise({ try: () => chromium.launch(), catch: (e) => fail(`chromium launch failed: ${String(e)}`) }),
      (b) => Effect.promise(() => b.close()),
    );
    const newPage = (scale: number) =>
      Effect.tryPromise({
        try: async () => {
          const ctx = await browser.newContext({ deviceScaleFactor: scale, colorScheme: "light", viewport: { width: 1600, height: 1200 } });
          return ctx.newPage();
        },
        catch: (e) => fail(`page: ${String(e)}`),
      });
    const refPage = yield* newPage(SCALE);
    const plainPage = yield* newPage(1);
    const promise = <A>(what: string, f: () => Promise<A>) => Effect.tryPromise({ try: f, catch: (e) => fail(`${what}: ${String(e)}`) });

    for (const theme of themes) {
      const key = themeKey(theme);
      const bakedDir = path.join(work, `baked-${key}`);
      const pngDir = path.join(work, `png-${key}`);
      yield* fs.makeDirectory(pngDir, { recursive: true });
      for (let c = 0; c < names.length; c += CHUNK) {
        const chunk: { name: string; plain: string; baked: string }[] = [];
        for (const name of names.slice(c, c + CHUNK)) {
          const plain = yield* readSvg(plainDir, name);
          const baked = yield* readSvg(bakedDir, name);
          if (Option.isNone(plain) && Option.isNone(baked)) {
            notRendered.add(name);
            continue;
          }
          if (Option.isNone(plain) || Option.isNone(baked)) {
            result.baked.mismatches.push({ diagram: name, theme: key, label: "diagram", property: "rendered", reference: String(Option.isSome(plain)), target: String(Option.isSome(baked)) });
            continue;
          }
          chunk.push({ name, plain: plain.value, baked: baked.value });
        }
        if (chunk.length === 0) continue;
        const size = chunk.reduce(
          (acc, d) => {
            const { w, h } = viewBox(d.plain);
            return { width: Math.max(acc.width, Math.ceil(w)), height: Math.max(acc.height, Math.ceil(h)) };
          },
          { width: 0, height: 0 },
        );

        yield* promise("reference page", () => load(refPage, pageHtml(theme, true, chunk.map((d) => ({ name: d.name, svg: d.plain }))), pageCss, size));
        const reference = yield* promise("reference probe", () => probe(refPage, rsvgAvailable));
        const shots: Raster[] = [];
        if (rsvgAvailable) {
          for (let i = 0; i < chunk.length; i++) {
            const png = yield* promise("screenshot", () => refPage.locator(".parity-d").nth(i).screenshot({ animations: "disabled" }));
            shots.push(decodePng(new Uint8Array(png)));
          }
        }
        yield* promise("baked page", () => load(plainPage, pageHtml(theme, false, chunk.map((d) => ({ name: d.name, svg: d.baked }))), pageCss, size));
        const baked = yield* promise("baked probe", () => probe(plainPage, false));
        yield* promise("attribute page", () => load(plainPage, pageHtml(theme, false, chunk.map((d) => ({ name: d.name, svg: withoutStyle(d.baked) }))), pageCss, size));
        const attributes = yield* promise("attribute probe", () => probe(plainPage, false));

        for (const [i, d] of chunk.entries()) {
          const ref = reference[i];
          if (ref === undefined) continue;
          const refRecords = ref.elements.map(toRecord);
          if (theme === themes[0]) {
            mutable.diagrams += 1;
          }
          mutable.elements += refRecords.length;
          add(result.baked, compareElements(refRecords, (baked[i]?.elements ?? []).map(toRecord), TOLERANCE), d.name, key);
          add(result.attributes, compareElements(refRecords, (attributes[i]?.elements ?? []).map(toRecord), TOLERANCE), d.name, key);

          if (result.rsvg !== null) {
            const shot = shots[i];
            const pngPath = path.join(pngDir, `${d.name}.png`);
            const code = yield* Command.make("rsvg-convert", "-z", String(SCALE), "-f", "png", "-o", pngPath, path.join(bakedDir, `${d.name}.svg`)).pipe(
              Command.exitCode,
              Effect.orElseSucceed(() => 1),
            );
            if (code !== 0 || shot === undefined) {
              result.rsvg.mismatches.push({ diagram: d.name, theme: key, label: "diagram", property: "rsvg-convert", reference: "rendered", target: `exit ${code}` });
              continue;
            }
            const raster = decodePng(new Uint8Array(yield* fs.readFile(pngPath)));
            const samples: PixelSample[] = [];
            const sampled = new Set<number>();
            for (const s of ref.samples) {
              const x = Math.floor(s.x * SCALE);
              const y = Math.floor(s.y * SCALE);
              const e = ref.elements[s.index];
              if (e === undefined) continue;
              const paint = s.paint === "fill" ? e.fill : e.stroke;
              const solid = paint !== null && paint[3] === 255 && e.opacity === 1;
              const o = (y * shot.width + x) * 4;
              const px: Rgba = [shot.rgba[o] ?? 0, shot.rgba[o + 1] ?? 0, shot.rgba[o + 2] ?? 0, shot.rgba[o + 3] ?? 0];
              // A sample counts only where the reference shows the element's own paint
              // (opaque) or a flat area (translucent), so occlusion and antialiasing are excluded.
              const clear = solid ? sameColour(px, paint, TOLERANCE) : s.paint === "fill" && uniformAt(shot, x, y, 1);
              if (!clear) {
                result.rsvg.rejectedSamples += 1;
                continue;
              }
              sampled.add(s.index);
              samples.push({ label: `${e.label} ${s.paint}`, x, y });
            }
            add(result.rsvg, comparePixels(shot, raster, samples, TOLERANCE), d.name, key);
            result.rsvg.sampledElements += sampled.size;
            const candidates = ref.elements.filter((e) => e.tag !== "text" && e.tag !== "tspan" && (e.fill !== null || e.stroke !== null));
            const missed = candidates.filter((e) => !sampled.has(e.index));
            result.rsvg.unsampledElements += missed.length;
            for (const e of missed) {
              const k = `${e.kind}:${e.tag}`;
              result.rsvg.unsampledByKind[k] = (result.rsvg.unsampledByKind[k] ?? 0) + 1;
            }
          }
        }
      }
    }
    mutable.notRendered = [...notRendered];

    yield* fs.writeFileString(path.join(work, "report.json"), JSON.stringify(result, null, 2));
    yield* Console.log(summarise(result));
    const all = [...result.baked.mismatches, ...result.attributes.mismatches, ...(result.rsvg?.mismatches ?? [])];
    if (all.length > 0) {
      const head = all.slice(0, 20).map((m) => `${m.diagram} [${m.theme}] ${m.label} ${m.property}: reference ${m.reference}, target ${m.target}`);
      return yield* fail(`${all.length} mismatches (full list in ${rel(path.join(work, "report.json"))}):\n${head.join("\n")}`);
    }
    return result;
  }).pipe(Effect.scoped);
