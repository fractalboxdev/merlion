/**
 * `bench sequence`: renders the `compat-sequence` corpus with Merlion and with
 * mermaid 12.0.0, compares the drawn content against the mermaid drawing of the
 * same source, and writes the Markdown summary (specs/benchmark.md,
 * specs/sequence.md).
 *
 * The flowchart run measures a routed graph; a sequence has none, so this
 * command records what the two renderers drew, the size of the drawing, its
 * label overlaps and the render time, and nothing about crossings or stress.
 */
import { FileSystem, Path } from "@effect/platform";
import { Console, Effect, Option, Schema } from "effect";
import { sequenceCompat, type SequenceCompat, sequenceMetrics, type SequenceMetrics } from "./metrics/sequence.ts";
import { COMPAT_SEQUENCE_DIR, RESULTS_DIR } from "./paths.ts";
import { MerlionLive } from "./renderers/merlion.ts";
import { MermaidDagreLive } from "./renderers/mermaid.ts";
import { Renderer } from "./renderers/Renderer.ts";
import { Manifest } from "./schema.ts";
import { extractSequence, type SequenceDrawing } from "./svg/sequence.ts";

export interface SequenceRunOptions {
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
  readonly drawing: SequenceDrawing;
  readonly metrics: SequenceMetrics | null;
}

const loadCorpus = (limit: Option.Option<number>) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const text = yield* fs.readFileString(path.join(COMPAT_SEQUENCE_DIR, "manifest.json"));
    const manifest = yield* Schema.decodeUnknown(Schema.parseJson(Manifest))(text);
    const entries = Option.match(limit, { onNone: () => manifest.diagrams, onSome: (n) => manifest.diagrams.slice(0, n) });
    const diagrams = yield* Effect.forEach(entries, (e) =>
      fs
        .readFileString(path.join(COMPAT_SEQUENCE_DIR, `${e.name}.mmd`))
        .pipe(Effect.map((source): Diagram => ({ name: e.name, source }))),
    );
    return { manifest, diagrams };
  });

const drawAll = (diagrams: readonly Diagram[], outSvgs: boolean) =>
  Effect.gen(function* () {
    const r = yield* Renderer;
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const svgDir = path.join(RESULTS_DIR, "svgs-sequence", r.name);
    if (outSvgs) yield* fs.makeDirectory(svgDir, { recursive: true });

    const batch = r.renderBatch === undefined ? undefined : yield* r.renderBatch(diagrams);
    const out: Drawn[] = [];
    let done = 0;
    for (const d of diagrams) {
      const o = batch?.get(d.name) ?? (yield* r.render(d.source));
      const drawing = o.svg === null ? extractSequence("") : extractSequence(o.svg);
      if (outSvgs && o.svg !== null) yield* fs.writeFileString(path.join(svgDir, `${d.name}.svg`), o.svg);
      out.push({
        name: d.name,
        error: o.error,
        ms: o.ms,
        fuelUsed: o.fuelUsed,
        drawing,
        metrics: o.svg === null ? null : sequenceMetrics(drawing),
      });
      done++;
      if (done % 50 === 0 || done === diagrams.length) yield* Console.log(`  ${r.name}: ${done}/${diagrams.length}`);
    }
    return { name: r.name, version: r.version, drawn: out };
  });

const percentile = (xs: readonly number[], p: number): number | null => {
  if (xs.length === 0) return null;
  const s = [...xs].sort((a, b) => a - b);
  const k = Math.min(s.length - 1, Math.max(0, Math.ceil((p / 100) * s.length) - 1));
  return s[k]!;
};

const mean = (xs: readonly number[]): number | null =>
  xs.length === 0 ? null : xs.reduce((a, b) => a + b, 0) / xs.length;

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

export const sequenceRun = (opts: SequenceRunOptions) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const { manifest, diagrams } = yield* loadCorpus(opts.limit);
    yield* Console.log(`compat-sequence: ${diagrams.length} diagrams at ${manifest.tag}`);

    const reference = yield* drawAll(diagrams, opts.outSvgs).pipe(Effect.provide(MermaidDagreLive));
    const candidate = yield* drawAll(diagrams, opts.outSvgs).pipe(Effect.provide(MerlionLive));

    const refBy = new Map(reference.drawn.map((d) => [d.name, d]));
    const compat = new Map<string, SequenceCompat>();
    for (const c of candidate.drawn) {
      const ref = refBy.get(c.name);
      if (ref === undefined || ref.error !== null || c.error !== null) continue;
      compat.set(c.name, sequenceCompat(ref.drawing, c.drawing));
    }

    const sources = new Map(diagrams.map((d) => [d.name, d.source]));
    const md = report(manifest, reference, candidate, compat, sources);
    const stamp = localDate();
    const out = path.join(RESULTS_DIR, `${stamp}-sequence-baseline.md`);
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
    area: [mean(m.map((x) => x.area)), median(m.map((x) => x.area))] as const,
    width: [mean(m.map((x) => x.width)), median(m.map((x) => x.width))] as const,
    height: [mean(m.map((x) => x.height)), median(m.map((x) => x.height))] as const,
    overlaps: [mean(m.map((x) => x.labelOverlaps)), median(m.map((x) => x.labelOverlaps))] as const,
    p50: percentile(ok.map((d) => d.ms), 50),
    p95: percentile(ok.map((d) => d.ms), 95),
    fuel: mean(ok.flatMap((d) => (d.fuelUsed === null ? [] : [d.fuelUsed]))),
  };
};

const report = (
  manifest: Manifest,
  reference: Side,
  candidate: Side,
  compat: ReadonlyMap<string, SequenceCompat>,
  sources: ReadonlyMap<string, string>,
): string => {
  const ref = column(reference);
  const cand = column(candidate);
  const cs = [...compat.values()];
  const failures = [...compat.entries()].filter(([, c]) => !c.pass);
  const lines: string[] = [];

  lines.push(`# Sequence baseline — ${localDate()}`);
  lines.push("");
  lines.push(
    `\`compat-sequence\`: ${manifest.diagrams.length} \`sequenceDiagram\` sources extracted from mermaid at ` +
      `\`${manifest.tag}\` (see \`bench/NOTICES.md\`). Compatibility is measured against ` +
      `\`${reference.name}\` ${reference.version}: the same participant labels, the same message count, the same ` +
      "non-empty message labels and the same note texts, each compared as a multiset with whitespace removed.",
  );
  lines.push("");
  lines.push("| Measure | merlion | mermaid-dagre |");
  lines.push("|---|---|---|");
  lines.push(`| Rendered | ${pct(cand.rendered, cand.total)} | ${pct(ref.rendered, ref.total)} |`);
  lines.push(`| Same content as mermaid-dagre | ${pct(cs.filter((c) => c.pass).length, cs.length)} | – |`);
  lines.push(`| Fits 720 px | ${pct(cand.fits, cand.rendered)} | ${pct(ref.fits, ref.rendered)} |`);
  lines.push(`| Width, mean / median | ${int(cand.width[0])} / ${int(cand.width[1])} | ${int(ref.width[0])} / ${int(ref.width[1])} |`);
  lines.push(
    `| Height, mean / median | ${int(cand.height[0])} / ${int(cand.height[1])} | ${int(ref.height[0])} / ${int(ref.height[1])} |`,
  );
  lines.push(`| Area (px²), mean / median | ${int(cand.area[0])} / ${int(cand.area[1])} | ${int(ref.area[0])} / ${int(ref.area[1])} |`);
  lines.push(
    `| Label overlaps, mean / median | ${fmt(cand.overlaps[0])} / ${fmt(cand.overlaps[1])} | ${fmt(ref.overlaps[0])} / ${fmt(ref.overlaps[1])} |`,
  );
  lines.push(`| Speed p50 / p95 (ms) | ${fmt(cand.p50, 1)} / ${fmt(cand.p95, 1)} | ${fmt(ref.p50, 1)} / ${fmt(ref.p95, 1)} |`);
  lines.push(`| Mean fuel | ${int(cand.fuel)} | – |`);
  lines.push("");
  lines.push(
    "Label overlaps count pairs of drawn boxes intersecting by more than 1 px²: the participant head boxes and " +
      "note boxes each renderer draws, plus one box per message label estimated at 0.55 em per character in that " +
      "renderer's own font size, because mermaid draws no background box behind a message label.",
  );
  lines.push("");
  lines.push("## Where the content differs");
  lines.push("");
  lines.push("| Check | Diagrams failing |");
  lines.push("|---|---|");
  for (const [key, label] of [
    ["participants", "Participant labels"],
    ["messageCount", "Message count"],
    ["messageLabels", "Message labels"],
    ["notes", "Note texts"],
    ["fragments", "Fragment kinds (not part of the pass)"],
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
      const [check, detail] = !c.participants
        ? ["participants", `missing ${JSON.stringify(c.missingParticipants.slice(0, 3))}, extra ${JSON.stringify(c.extraParticipants.slice(0, 3))}`]
        : !c.messageCount
          ? ["messages", `${c.candMessages} drawn against ${c.refMessages}`]
          : !c.messageLabels
            ? ["message labels", "the label multisets differ"]
            : ["notes", "the note texts differ"];
      lines.push(`| \`${name}\` | ${check} | ${detail} |`);
    }
    lines.push("");
  }

  const math = failures.filter(([name]) => (sources.get(name) ?? "").includes("$$")).length;
  if (math > 0) {
    lines.push("## Known differences");
    lines.push("");
    lines.push(
      `mermaid typesets \`$$…$$\` with KaTeX and Merlion draws it as the text it is, so the two never agree on ` +
        `a diagram that carries one. ${math} of the ${failures.length} differing diagrams do.`,
    );
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
  lines.push("");
  return lines.join("\n");
};
