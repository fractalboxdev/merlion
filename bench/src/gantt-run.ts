/** Gantt schedule compatibility; every retained source remains in the denominator. */
import { FileSystem, Path } from "@effect/platform";
import { Console, Effect, Option, Schema } from "effect";
import { ganttCompat, ganttGate, ganttOutcome, type GanttCompat } from "./metrics/gantt.ts";
import { COMPAT_GANTT_DIR, RESULTS_DIR } from "./paths.ts";
import { GanttReference, GanttReferenceLive } from "./renderers/gantt-reference.ts";
import { MerlionLive } from "./renderers/merlion.ts";
import { Renderer, failed, type RenderOutcome } from "./renderers/Renderer.ts";
import { Manifest } from "./schema.ts";
import { extractGantt, ganttGeometry } from "./svg/gantt.ts";

export interface GanttRunOptions { readonly limit: Option.Option<number>; readonly outSvgs: boolean; readonly minCompat: number }
const cell = (s: string): string => s.replace(/\|/g, "\\|").replace(/\s+/g, " ").slice(0, 600);
export class GanttGateFailed extends Schema.TaggedError<GanttGateFailed>()("GanttGateFailed", { message: Schema.String }) {}

export const ganttRun = (opts: GanttRunOptions) => Effect.gen(function* () {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const manifest = yield* fs.readFileString(path.join(COMPAT_GANTT_DIR, "manifest.json")).pipe(
    Effect.flatMap(Schema.decodeUnknown(Schema.parseJson(Manifest))),
  );
  const entries = Option.match(opts.limit, { onNone: () => manifest.diagrams, onSome: (n) => manifest.diagrams.slice(0, n) });
  if (opts.minCompat > 0 && Option.isSome(opts.limit)) return yield* new GanttGateFailed({ message: "min-compat requires the complete corpus; omit --limit" });
  const optionsError = ganttGate(manifest.diagrams.length, entries.length, entries.length, opts.minCompat);
  if (optionsError !== null) return yield* new GanttGateFailed({ message: optionsError });
  const diagrams = yield* Effect.forEach(entries, (d) => fs.readFileString(path.join(COMPAT_GANTT_DIR, `${d.name}.mmd`)).pipe(
    Effect.map((source) => ({ name: d.name, source })),
  ));
  yield* Console.log(`compat-gantt: ${diagrams.length}/${manifest.diagrams.length} sources at ${manifest.tag}`);
  const references = yield* Effect.gen(function* () {
    const renderer = yield* GanttReference;
    const drawn = yield* Effect.forEach(diagrams, (d, i) => renderer.render(d.source).pipe(
      Effect.tap(() => (i + 1) % 20 === 0 || i + 1 === diagrams.length ? Console.log(`  mermaid: ${i + 1}/${diagrams.length}`) : Effect.void),
    ));
    return { browserVersion: renderer.browserVersion, drawn };
  }).pipe(Effect.provide(GanttReferenceLive));
  const candidates = yield* Effect.gen(function* () {
    const renderer = yield* Renderer;
    if (renderer.renderBatch !== undefined) {
      const batch = yield* renderer.renderBatch(diagrams);
      return diagrams.map((d) => batch.get(d.name) ?? failed("missing batch outcome"));
    }
    return yield* Effect.forEach(diagrams, (d) => renderer.render(d.source));
  }).pipe(Effect.provide(MerlionLive));
  yield* fs.makeDirectory(RESULTS_DIR, { recursive: true });
  const rows: Array<{ name: string; classifications: readonly string[]; outcome: string; compat: GanttCompat | null; reference: Omit<RenderOutcome, "svg">; candidate: Omit<RenderOutcome, "svg">; fits720: boolean }> = [];
  for (let i = 0; i < diagrams.length; i++) {
    const d = diagrams[i]!;
    const ref = references.drawn[i]!;
    const cand = candidates[i]!;
    const drawing = extractGantt(cand.svg ?? "");
    const schedule = ref.error === null && cand.error === null ? ganttCompat(ref.tasks, drawing.tasks, d.source) : null;
    const geometry = [...ganttGeometry(drawing.tasks, drawing.ticks), ...(drawing.validSvg ? [] : ["candidate has no valid SVG viewport"])];
    const compat = schedule === null ? null : { ...schedule, pass: schedule.pass && geometry.length === 0, differences: [...schedule.differences, ...geometry] };
    const { svg: _refSvg, tasks: _tasks, ...reference } = ref;
    const { svg: _candSvg, ...candidate } = cand;
    const classifications = [
      ...(/^\s*todayMarker\s+off\s*$/m.test(d.source) ? [] : ["clock-dependent reference today marker"]),
      ...(d.name.includes("should-fail-rendering") ? ["upstream failure fixture"] : []),
      ...(compat !== null && compat.mode !== "absolute UTC" ? [compat.mode] : []),
    ];
    rows.push({ name: d.name, classifications, outcome: ganttOutcome(ref.error, cand.error, compat), compat, reference, candidate, fits720: drawing.fits720 });
    if (opts.outSvgs) {
      for (const [name, svg] of [["mermaid", ref.svg], ["merlion", cand.svg]] as const) {
        const dir = path.join(RESULTS_DIR, "svgs-gantt", name);
        yield* fs.makeDirectory(dir, { recursive: true });
        if (svg !== null) yield* fs.writeFileString(path.join(dir, `${d.name}.svg`), svg);
      }
    }
  }
  const date = new Date().toISOString().slice(0, 10);
  const passed = rows.filter((r) => r.outcome === "matched").length;
  const normalized = rows.filter((r) => r.outcome === "matched" && r.compat?.mode.startsWith("time-of-day")).length;
  const partial = rows.filter((r) => r.outcome === "matched" && r.compat?.mode.startsWith("day-of-month")).length;
  const percent = rows.length === 0 ? "0.0" : (100 * passed / rows.length).toFixed(1);
  const lines = [
    `# Gantt baseline — ${date}`, "",
    `Pinned Mermaid ${manifest.tag} (${manifest.commit}); Chromium ${references.browserVersion}; ${rows.length} of ${manifest.diagrams.length} retained unique sources. See \`bench/NOTICES.md\` for provenance.`, "",
    "Compatibility compares task IDs, visible labels and marks, sections, status flags, and resolved start/end timestamps. Merlion marks and timestamped axis grid lines must follow one increasing affine time scale, including milestone midpoint and vertical-marker start positions. Ordinary task width is the projected duration with a 2 px visibility minimum. Mermaid runs in UTC. This is schedule/content compatibility, not pixel equivalence or proof of axis-label/layout equivalence.", "",
    "Time-only date formats normalize each renderer's common midnight origin. Day-of-month formats without month/year normalize the common first-month origin, preserving day-of-month and elapsed durations; crossing unequal calendar months can still differ. These matches establish relative calendar/time equivalence, not absolute-calendar equivalence. Mermaid's current-day defaults and today marker remain clock-dependent; Merlion's deterministic omissions are documented in specs/gantt.md.", "",
    "| Measure | Result |", "|---|---|",
    `| Matched / all retained sources | ${percent}% (${passed}/${rows.length}) |`,
    `| Absolute UTC matches | ${passed - normalized - partial} |`,
    `| Time-only normalized matches | ${normalized} |`,
    `| Day-of-month normalized matches | ${partial} |`,
    `| Merlion rendered | ${rows.filter((r) => r.candidate.error === null).length}/${rows.length} |`,
    `| Mermaid rendered | ${rows.filter((r) => r.reference.error === null).length}/${rows.length} |`,
    `| Merlion fits 720 px | ${rows.filter((r) => r.candidate.error === null && r.fits720).length}/${rows.length} |`, "",
    "Invalid, unsupported and clock-dependent sources stay in the denominator. Both-rejected cases are rejection agreement and earn no successful-render credit. The fixture named `should-fail-rendering` remains classified as an upstream failure fixture even if the pinned renderer accepts it through browser date fallbacks. Reference today-marker classifications describe a live decoration outside the schedule metric; omission is not claimed as marker parity. A limited run is exploratory and cannot establish corpus coverage.", "",
    "| Source | Outcome | Classification | Comparison / error |", "|---|---|---|---|",
    ...rows.map((r) => `| \`${r.name}\` | ${r.outcome} | ${cell(r.classifications.join("; "))} | ${cell(r.compat?.pass ? r.compat.mode : [r.reference.error && `Mermaid: ${r.reference.error}`, r.candidate.error && `Merlion: ${r.candidate.error}`, ...(r.compat?.differences ?? [])].filter(Boolean).join("; "))} |`), "",
  ];
  const stem = path.join(RESULTS_DIR, `${date}-gantt-baseline`);
  yield* fs.writeFileString(`${stem}.md`, lines.join("\n"));
  yield* fs.writeFileString(`${stem}.json`, `${JSON.stringify({ tag: manifest.tag, commit: manifest.commit, browserVersion: references.browserVersion, totalCorpus: manifest.diagrams.length, measured: rows.length, passed, rows }, null, 2)}\n`);
  yield* Console.log(`schedule compatibility: ${percent}% (${passed}/${rows.length}); report: ${path.relative(process.cwd(), `${stem}.md`)}`);
  const gateError = ganttGate(manifest.diagrams.length, rows.length, passed, opts.minCompat);
  if (gateError !== null) return yield* new GanttGateFailed({ message: gateError });
  return { diagrams: rows.length, passed };
});
