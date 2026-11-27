/** Quadrant chart compatibility; every retained source remains in the denominator. */
import { FileSystem, Path } from "@effect/platform";
import { Console, Effect, Option, Schema } from "effect";
import { quadrantCompat, quadrantGate, quadrantOutcome, type QuadrantCompat } from "./metrics/quadrant.ts";
import { COMPAT_QUADRANT_DIR, RESULTS_DIR } from "./paths.ts";
import { MerlionLive } from "./renderers/merlion.ts";
import { MermaidDagreLive } from "./renderers/mermaid.ts";
import { Renderer, failed, type RenderOutcome } from "./renderers/Renderer.ts";
import { Manifest } from "./schema.ts";
import { extractQuadrant } from "./svg/quadrant.ts";

export interface QuadrantRunOptions { readonly limit: Option.Option<number>; readonly outSvgs: boolean; readonly minCompat: number }
const cell = (s: string): string => s.replace(/\|/g, "\\|").replace(/\s+/g, " ").slice(0, 600);
export class QuadrantGateFailed extends Schema.TaggedError<QuadrantGateFailed>()("QuadrantGateFailed", { message: Schema.String }) {}

const renderAll = (diagrams: ReadonlyArray<{ name: string; source: string }>) =>
  Effect.gen(function* () {
    const renderer = yield* Renderer;
    if (renderer.renderBatch !== undefined) {
      const batch = yield* renderer.renderBatch(diagrams);
      return diagrams.map((d) => batch.get(d.name) ?? failed("missing batch outcome"));
    }
    return yield* Effect.forEach(diagrams, (d) => renderer.render(d.source));
  });

export const quadrantRun = (opts: QuadrantRunOptions) => Effect.gen(function* () {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const manifest = yield* fs.readFileString(path.join(COMPAT_QUADRANT_DIR, "manifest.json")).pipe(
    Effect.flatMap(Schema.decodeUnknown(Schema.parseJson(Manifest))),
  );
  const entries = Option.match(opts.limit, { onNone: () => manifest.diagrams, onSome: (n) => manifest.diagrams.slice(0, n) });
  if (opts.minCompat > 0 && Option.isSome(opts.limit)) return yield* new QuadrantGateFailed({ message: "min-compat requires the complete corpus; omit --limit" });
  const optionsError = quadrantGate(manifest.diagrams.length, entries.length, entries.length, opts.minCompat);
  if (optionsError !== null) return yield* new QuadrantGateFailed({ message: optionsError });
  const diagrams = yield* Effect.forEach(entries, (d) => fs.readFileString(path.join(COMPAT_QUADRANT_DIR, `${d.name}.mmd`)).pipe(
    Effect.map((source) => ({ name: d.name, source })),
  ));
  yield* Console.log(`compat-quadrant: ${diagrams.length}/${manifest.diagrams.length} sources at ${manifest.tag}`);
  const references = yield* renderAll(diagrams).pipe(Effect.provide(MermaidDagreLive));
  const candidates = yield* renderAll(diagrams).pipe(Effect.provide(MerlionLive));
  yield* fs.makeDirectory(RESULTS_DIR, { recursive: true });
  const rows: Array<{ name: string; outcome: string; compat: QuadrantCompat | null; reference: Omit<RenderOutcome, "svg">; candidate: Omit<RenderOutcome, "svg">; fits720: boolean }> = [];
  for (let i = 0; i < diagrams.length; i++) {
    const d = diagrams[i]!;
    const ref = references[i]!;
    const cand = candidates[i]!;
    const drawing = extractQuadrant(cand.svg ?? "");
    const compat = ref.error === null && cand.error === null ? quadrantCompat(extractQuadrant(ref.svg ?? ""), drawing, d.source) : null;
    const { svg: _refSvg, ...reference } = ref;
    const { svg: _candSvg, ...candidate } = cand;
    rows.push({ name: d.name, outcome: quadrantOutcome(ref.error, cand.error, compat), compat, reference, candidate, fits720: drawing.fits720 });
    if (opts.outSvgs) {
      for (const [name, svg] of [["mermaid", ref.svg], ["merlion", cand.svg]] as const) {
        const dir = path.join(RESULTS_DIR, "svgs-quadrant", name);
        yield* fs.makeDirectory(dir, { recursive: true });
        if (svg !== null) yield* fs.writeFileString(path.join(dir, `${d.name}.svg`), svg);
      }
    }
  }
  const date = new Date().toISOString().slice(0, 10);
  const passed = rows.filter((r) => r.outcome === "matched").length;
  const percent = rows.length === 0 ? "0.0" : ((100 * passed) / rows.length).toFixed(1);
  const lines = [
    `# Quadrant chart baseline — ${date}`, "",
    `Pinned mermaid ${manifest.tag} (${manifest.commit}) in headless Chromium; ${rows.length} of ${manifest.diagrams.length} retained unique sources. See \`bench/NOTICES.md\` for provenance.`, "",
    "Compatibility compares the title, the four quadrant labels and where each sits in its quadrant, each axis label with its side of the plot and its alignment, and every point: its label, its 0–1 coordinates recovered from the circle's position in the plot, its radius, its stroke width, its explicit fill and stroke colours, and its label below the point. Quadrant fills are compared where the source sets them. Default colours are each renderer's theme and are not compared. This is content and placement compatibility, not pixel equivalence.", "",
    "| Measure | Result |", "|---|---|",
    `| Matched / all retained sources | ${percent}% (${passed}/${rows.length}) |`,
    `| Merlion rendered | ${rows.filter((r) => r.candidate.error === null).length}/${rows.length} |`,
    `| mermaid rendered | ${rows.filter((r) => r.reference.error === null).length}/${rows.length} |`,
    `| Merlion fits 720 px | ${rows.filter((r) => r.candidate.error === null && r.fits720).length}/${rows.length} |`, "",
    "Invalid and unsupported sources stay in the denominator; two rejections earn no credit. A limited run is exploratory and cannot establish corpus coverage.", "",
    "| Source | Outcome | Comparison / error |", "|---|---|---|",
    ...rows.map((r) => `| \`${r.name}\` | ${r.outcome} | ${cell(r.compat?.pass ? "" : [r.reference.error && `mermaid: ${r.reference.error}`, r.candidate.error && `Merlion: ${r.candidate.error}`, ...(r.compat?.differences ?? [])].filter(Boolean).join("; "))} |`), "",
  ];
  const stem = path.join(RESULTS_DIR, `${date}-quadrant-baseline`);
  yield* fs.writeFileString(`${stem}.md`, lines.join("\n"));
  yield* fs.writeFileString(`${stem}.json`, `${JSON.stringify({ tag: manifest.tag, commit: manifest.commit, totalCorpus: manifest.diagrams.length, measured: rows.length, passed, rows }, null, 2)}\n`);
  yield* Console.log(`quadrant compatibility: ${percent}% (${passed}/${rows.length}); report: ${path.relative(process.cwd(), `${stem}.md`)}`);
  const gateError = quadrantGate(manifest.diagrams.length, rows.length, passed, opts.minCompat);
  if (gateError !== null) return yield* new QuadrantGateFailed({ message: gateError });
  return { diagrams: rows.length, passed };
});
