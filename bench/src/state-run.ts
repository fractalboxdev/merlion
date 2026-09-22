/**
 * `bench state`: renders the `compat-state` corpus with Merlion and with mermaid
 * 12.0.0, compares the drawn content against the mermaid drawing of the same
 * source, and writes the Markdown summary (specs/benchmark.md, specs/state.md).
 *
 * A state machine is a routed graph on both sides, so this run records the
 * flowchart measures — crossings, label overlaps, size and fit — beside the
 * content comparison and the render time.
 */
import { FileSystem, Path } from "@effect/platform";
import { Console, Effect, Option, Schema } from "effect";
import { stateCompat, type StateCompat, stateMetrics, type StateMetrics } from "./metrics/state.ts";
import { COMPAT_STATE_DIR, RESULTS_DIR } from "./paths.ts";
import { MerlionLive } from "./renderers/merlion.ts";
import { MermaidDagreLive } from "./renderers/mermaid.ts";
import { Renderer } from "./renderers/Renderer.ts";
import { Manifest } from "./schema.ts";
import { extractState, type StateDrawing } from "./svg/state.ts";

export interface StateRunOptions {
  readonly limit: Option.Option<number>;
  readonly outSvgs: boolean;
}

interface Diagram {
  readonly name: string;
  readonly source: string;
}

interface Drawn {
  readonly name: string;
  readonly error: string | null;
  readonly ms: number;
  readonly fuelUsed: number | null;
  readonly drawing: StateDrawing;
  readonly metrics: StateMetrics | null;
}

const loadCorpus = (limit: Option.Option<number>) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const text = yield* fs.readFileString(path.join(COMPAT_STATE_DIR, "manifest.json"));
    const manifest = yield* Schema.decodeUnknown(Schema.parseJson(Manifest))(text);
    const entries = Option.match(limit, { onNone: () => manifest.diagrams, onSome: (n) => manifest.diagrams.slice(0, n) });
    const diagrams = yield* Effect.forEach(entries, (e) =>
      fs
        .readFileString(path.join(COMPAT_STATE_DIR, `${e.name}.mmd`))
        .pipe(Effect.map((source): Diagram => ({ name: e.name, source }))),
    );
    return { manifest, diagrams };
  });

const drawAll = (diagrams: readonly Diagram[], outSvgs: boolean) =>
  Effect.gen(function* () {
    const r = yield* Renderer;
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const svgDir = path.join(RESULTS_DIR, "svgs-state", r.name);
    if (outSvgs) yield* fs.makeDirectory(svgDir, { recursive: true });

    const batch = r.renderBatch === undefined ? undefined : yield* r.renderBatch(diagrams);
    const out: Drawn[] = [];
    let done = 0;
    for (const d of diagrams) {
      const o = batch?.get(d.name) ?? (yield* r.render(d.source));
      const drawing = extractState(o.svg ?? "");
      if (outSvgs && o.svg !== null) yield* fs.writeFileString(path.join(svgDir, `${d.name}.svg`), o.svg);
      out.push({
        name: d.name,
        error: o.error,
        ms: o.ms,
        fuelUsed: o.fuelUsed,
        drawing,
        metrics: o.svg === null ? null : stateMetrics(drawing),
      });
      done++;
      if (done % 25 === 0 || done === diagrams.length) yield* Console.log(`  ${r.name}: ${done}/${diagrams.length}`);
    }
    return { name: r.name, version: r.version, drawn: out };
  });

const percentile = (xs: readonly number[], p: number): number | null => {
  if (xs.length === 0) return null;
  const s = [...xs].sort((a, b) => a - b);
  const k = Math.min(s.length - 1, Math.max(0, Math.ceil((p / 100) * s.length) - 1));
  return s[k]!;
};

const mean = (xs: readonly number[]): number | null => (xs.length === 0 ? null : xs.reduce((a, b) => a + b, 0) / xs.length);
const median = (xs: readonly number[]): number | null => percentile(xs, 50);

const fmt = (v: number | null, digits = 2): string => (v === null ? "–" : v.toFixed(digits));
const int = (v: number | null): string => (v === null ? "–" : Math.round(v).toLocaleString("en-US"));
/** Today where the run happens; a results file is named after the operator's day, not UTC's. */
const localDate = (): string => {
  const d = new Date();
  const two = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
};

const pct = (n: number, d: number): string => (d === 0 ? "–" : `${((100 * n) / d).toFixed(1)}% (${n}/${d})`);

export const stateRun = (opts: StateRunOptions) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const { manifest, diagrams } = yield* loadCorpus(opts.limit);
    yield* Console.log(`compat-state: ${diagrams.length} diagrams at ${manifest.tag}`);

    const reference = yield* drawAll(diagrams, opts.outSvgs).pipe(Effect.provide(MermaidDagreLive));
    const candidate = yield* drawAll(diagrams, opts.outSvgs).pipe(Effect.provide(MerlionLive));

    const refBy = new Map(reference.drawn.map((d) => [d.name, d]));
    const compat = new Map<string, StateCompat>();
    for (const c of candidate.drawn) {
      const ref = refBy.get(c.name);
      if (ref === undefined || ref.error !== null || c.error !== null) continue;
      compat.set(c.name, stateCompat(ref.drawing, c.drawing));
    }

    const md = report(manifest, reference, candidate, compat);
    const out = path.join(RESULTS_DIR, `${localDate()}-state-baseline.md`);
    yield* fs.makeDirectory(RESULTS_DIR, { recursive: true });
    yield* fs.writeFileString(out, md);
    yield* Console.log(`report: ${path.relative(process.cwd(), out)}`);
    return { diagrams: diagrams.length, passed: [...compat.values()].filter((c) => c.pass).length };
  });

interface Side {
  readonly name: string;
  readonly version: string;
  readonly drawn: readonly Drawn[];
}

const column = (side: Side) => {
  const ok = side.drawn.filter((d) => d.error === null && d.metrics !== null);
  const m = ok.map((d) => d.metrics!);
  return {
    rendered: ok.length,
    total: side.drawn.length,
    fits: m.filter((x) => x.fits720).length,
    states: mean(m.map((x) => x.states)),
    transitions: mean(m.map((x) => x.transitions)),
    crossings: [mean(m.map((x) => x.crossings)), median(m.map((x) => x.crossings))] as const,
    crossFree: m.filter((x) => x.crossings === 0).length,
    overlaps: [mean(m.map((x) => x.labelOverlaps)), median(m.map((x) => x.labelOverlaps))] as const,
    overlapFree: m.filter((x) => x.overlapsWithNotes === 0).length,
    width: [mean(m.map((x) => x.width)), median(m.map((x) => x.width))] as const,
    height: [mean(m.map((x) => x.height)), median(m.map((x) => x.height))] as const,
    area: [mean(m.map((x) => x.area)), median(m.map((x) => x.area))] as const,
    p50: percentile(ok.map((d) => d.ms), 50),
    p95: percentile(ok.map((d) => d.ms), 95),
    fuel: mean(ok.flatMap((d) => (d.fuelUsed === null ? [] : [d.fuelUsed]))),
  };
};

const report = (
  manifest: Manifest,
  reference: Side,
  candidate: Side,
  compat: ReadonlyMap<string, StateCompat>,
): string => {
  const ref = column(reference);
  const cand = column(candidate);
  const cs = [...compat.values()];
  const failures = [...compat.entries()].filter(([, c]) => !c.pass);
  const lines: string[] = [];

  lines.push(`# State baseline — ${localDate()}`);
  lines.push("");
  lines.push(
    `\`compat-state\`: ${manifest.diagrams.length} \`stateDiagram\` and \`stateDiagram-v2\` sources extracted from ` +
      `mermaid at \`${manifest.tag}\` (see \`bench/NOTICES.md\`). Compatibility is measured against ` +
      `\`${reference.name}\` ${reference.version}: the same state labels, the same number of pseudo-states, the same ` +
      "transition count, the same non-empty transition labels and the same note texts, each compared as a multiset " +
      "with whitespace removed. A state is named by what a reader sees, because the two renderers generate different " +
      "ids for the same scope, so `[*]`, a choice diamond and a fork bar are counted rather than named.",
  );
  lines.push("");
  lines.push("| Measure | merlion | mermaid-dagre |");
  lines.push("|---|---|---|");
  lines.push(`| Rendered | ${pct(cand.rendered, cand.total)} | ${pct(ref.rendered, ref.total)} |`);
  lines.push(`| Same content as mermaid-dagre | ${pct(cs.filter((c) => c.pass).length, cs.length)} | – |`);
  lines.push(`| Fits 720 px | ${pct(cand.fits, cand.rendered)} | ${pct(ref.fits, ref.rendered)} |`);
  lines.push(`| States / transitions, mean | ${fmt(cand.states, 1)} / ${fmt(cand.transitions, 1)} | ${fmt(ref.states, 1)} / ${fmt(ref.transitions, 1)} |`);
  lines.push(`| Crossings, mean / median | ${fmt(cand.crossings[0])} / ${fmt(cand.crossings[1])} | ${fmt(ref.crossings[0])} / ${fmt(ref.crossings[1])} |`);
  lines.push(`| Crossing-free | ${pct(cand.crossFree, cand.rendered)} | ${pct(ref.crossFree, ref.rendered)} |`);
  lines.push(
    `| Label overlaps, mean / median | ${fmt(cand.overlaps[0])} / ${fmt(cand.overlaps[1])} | ${fmt(ref.overlaps[0])} / ${fmt(ref.overlaps[1])} |`,
  );
  lines.push(`| Overlap-free, notes included | ${pct(cand.overlapFree, cand.rendered)} | ${pct(ref.overlapFree, ref.rendered)} |`);
  lines.push(`| Width, mean / median | ${int(cand.width[0])} / ${int(cand.width[1])} | ${int(ref.width[0])} / ${int(ref.width[1])} |`);
  lines.push(`| Height, mean / median | ${int(cand.height[0])} / ${int(cand.height[1])} | ${int(ref.height[0])} / ${int(ref.height[1])} |`);
  lines.push(`| Area (px²), mean / median | ${int(cand.area[0])} / ${int(cand.area[1])} | ${int(ref.area[0])} / ${int(ref.area[1])} |`);
  lines.push(`| Speed p50 / p95 (ms) | ${fmt(cand.p50, 1)} / ${fmt(cand.p95, 1)} | ${fmt(ref.p50, 1)} / ${fmt(ref.p95, 1)} |`);
  lines.push(`| Mean fuel | ${int(cand.fuel)} | – |`);
  lines.push("");
  lines.push(
    "Crossings count pairs of routed transition segments meeting away from a shared endpoint; label overlaps count " +
      "pairs of drawn boxes intersecting by more than 1 px² over the state boxes and the transition label chips, " +
      "and the overlap-free row adds the note boxes to that set.",
  );
  lines.push("");
  lines.push("## Where the content differs");
  lines.push("");
  lines.push("| Check | Diagrams failing |");
  lines.push("|---|---|");
  for (const [key, label] of [
    ["states", "State labels"],
    ["pseudoStates", "Pseudo-state count"],
    ["transitionCount", "Transition count"],
    ["transitionLabels", "Transition labels"],
    ["notes", "Note texts"],
  ] as const) {
    lines.push(`| ${label} | ${cs.filter((c) => !c[key]).length} |`);
  }
  lines.push("");
  if (failures.length > 0) {
    lines.push(`${failures.length} diagrams differ. The first 20, with the first check each fails:`);
    lines.push("");
    lines.push("| Diagram | Check | Detail |");
    lines.push("|---|---|---|");
    for (const [name, c] of failures.slice(0, 20)) {
      const [check, detail] = !c.states
        ? ["state labels", `missing ${JSON.stringify(c.missingStates.slice(0, 3))}, extra ${JSON.stringify(c.extraStates.slice(0, 3))}`]
        : !c.pseudoStates
          ? ["pseudo-states", `${c.candPseudo} drawn against ${c.refPseudo}`]
          : !c.transitionCount
            ? ["transitions", `${c.candTransitions} drawn against ${c.refTransitions}`]
            : !c.transitionLabels
              ? ["transition labels", "the label multisets differ"]
              : ["notes", `${c.candNotes} drawn against ${c.refNotes}`];
      lines.push(`| \`${name}\` | ${check} | ${detail} |`);
    }
    lines.push("");
  }

  const refFailed = reference.drawn.filter((d) => d.error !== null);
  const candFailed = candidate.drawn.filter((d) => d.error !== null);
  lines.push("## Failed renders");
  lines.push("");
  lines.push(`mermaid rejects ${refFailed.length}; Merlion rejects ${candFailed.length}.`);
  if (candFailed.length > 0) {
    lines.push("");
    for (const d of candFailed.slice(0, 20)) lines.push(`- \`${d.name}\`: ${d.error}`);
  }
  if (refFailed.length > 0) {
    lines.push("");
    lines.push("mermaid rejects:");
    lines.push("");
    for (const d of refFailed.slice(0, 20)) lines.push(`- \`${d.name}\`: ${d.error}`);
  }
  lines.push("");
  return lines.join("\n");
};
