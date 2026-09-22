/**
 * `bench report`: summarises a results file as Markdown
 * (specs/benchmark.md#output). `summarise` and `renderReport` are pure.
 */
import { FileSystem, Path } from "@effect/platform";
import { Console, Effect, Option, Schema } from "effect";
import { mean, quantile } from "./metrics/metrics.ts";
import { RESULTS_DIR } from "./paths.ts";
import { type DiagramResult, type RendererResult, ResultsFile } from "./schema.ts";

/** Metrics compared per diagram; lower is better for each. */
export const COMPARED_METRICS = ["crossings", "maxCrossingsPerEdge", "bends", "edgeLength", "area", "labelOverlaps", "stress"] as const;
export type ComparedMetric = (typeof COMPARED_METRICS)[number];

const SUMMARY_METRICS = [...COMPARED_METRICS, "aspectRatio"] as const;
type SummaryMetric = (typeof SUMMARY_METRICS)[number];

/** Reference layout for per-diagram win/loss counts (specs/roadmap.md M1 exit criteria). */
export const WIN_LOSS_BASELINE = "mermaid-elk";

export interface RendererSummary {
  readonly renderer: string;
  readonly version: string;
  readonly total: number;
  readonly rendered: number;
  /** Diagrams drawn as the same graph as the reference, over diagrams the reference rendered; null for the reference itself. */
  readonly compat: { readonly pass: number; readonly of: number } | null;
  readonly fit: { readonly pass: number; readonly of: number };
  readonly msP50: number | null;
  readonly msP95: number | null;
  readonly fuelMean: number | null;
  readonly metrics: Readonly<Record<SummaryMetric, { readonly mean: number | null; readonly median: number | null }>>;
  readonly stability: { readonly pairs: number; readonly ok: number; readonly mean: number | null; readonly p95: number | null };
  readonly errors: ReadonlyArray<readonly [string, number]>;
}

export interface WinLoss {
  readonly renderer: string;
  readonly metric: ComparedMetric;
  readonly win: number;
  readonly tie: number;
  readonly loss: number;
}

const metricValue = (d: DiagramResult, m: SummaryMetric): number | null => (d.metrics === null ? null : d.metrics[m]);

const nonNull = (xs: ReadonlyArray<number | null>): number[] => xs.filter((x): x is number => x !== null);

/** Groups error messages by their first line (truncated), most frequent first. */
const errorCounts = (ds: readonly DiagramResult[]): Array<[string, number]> => {
  const counts = new Map<string, number>();
  for (const d of ds) {
    if (d.ok || d.error === null) continue;
    const key = (d.error.split("\n")[0] ?? "").slice(0, 80);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return [...counts.entries()].sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1));
};

export const summarise = (r: RendererResult, reference: string): RendererSummary => {
  const ok = r.diagrams.filter((d) => d.ok);
  const metrics = Object.fromEntries(
    SUMMARY_METRICS.map((m) => {
      const xs = nonNull(ok.map((d) => metricValue(d, m)));
      return [m, { mean: mean(xs), median: quantile(xs, 0.5) }];
    }),
  ) as RendererSummary["metrics"];
  const ms = ok.map((d) => d.ms);
  const disp = r.edits.filter((e) => e.ok).flatMap((e) => e.displacements);
  return {
    renderer: r.renderer,
    version: r.version,
    total: r.diagrams.length,
    rendered: ok.length,
    compat: r.renderer === reference ? null : { pass: r.diagrams.filter((d) => d.compat?.pass === true).length, of: 0 },
    fit: { pass: ok.filter((d) => d.metrics?.fits720 === true).length, of: ok.length },
    msP50: quantile(ms, 0.5),
    msP95: quantile(ms, 0.95),
    fuelMean: mean(nonNull(ok.map((d) => d.fuelUsed))),
    metrics,
    stability: { pairs: r.edits.length, ok: r.edits.filter((e) => e.ok).length, mean: mean(disp), p95: quantile(disp, 0.95) },
    errors: errorCounts(r.diagrams),
  };
};

/**
 * Per-diagram comparison against the baseline on diagrams both rendered: a
 * win is a strictly lower value, a tie is equal within 1e-6 relative.
 */
export const winLoss = (cand: RendererResult, base: RendererResult): WinLoss[] => {
  const baseBy = new Map(base.diagrams.filter((d) => d.ok).map((d) => [d.name, d]));
  return COMPARED_METRICS.map((metric) => {
    let win = 0;
    let tie = 0;
    let loss = 0;
    for (const d of cand.diagrams) {
      const b = baseBy.get(d.name);
      if (!d.ok || b === undefined) continue;
      const x = metricValue(d, metric);
      const y = metricValue(b, metric);
      if (x === null || y === null) continue;
      if (Math.abs(x - y) <= 1e-6 * Math.max(1, Math.abs(x), Math.abs(y))) tie++;
      else if (x < y) win++;
      else loss++;
    }
    return { renderer: cand.renderer, metric, win, tie, loss };
  });
};

// ---------------------------------------------------------------------------
// Markdown.

const fmt = (x: number | null, digits = 1): string => (x === null ? "–" : x.toFixed(digits));
const pct = (n: number, of: number): string => (of === 0 ? "–" : `${((100 * n) / of).toFixed(1)}% (${n}/${of})`);

const METRIC_LABELS: Readonly<Record<SummaryMetric, [string, number]>> = {
  crossings: ["Crossings", 2],
  maxCrossingsPerEdge: ["Max crossings on one edge", 2],
  bends: ["Bends", 1],
  edgeLength: ["Total edge length (px)", 0],
  area: ["Area (px²)", 0],
  labelOverlaps: ["Label overlaps", 2],
  stress: ["Stress", 3],
  aspectRatio: ["Aspect ratio (w/h)", 2],
};

export const renderReport = (res: ResultsFile): string => {
  const refResult = res.renderers.find((r) => r.renderer === res.reference);
  const refOk = new Set(refResult?.diagrams.filter((d) => d.ok).map((d) => d.name) ?? []);
  const summaries = res.renderers.map((r) => {
    const s = summarise(r, res.reference);
    return s.compat === null ? s : { ...s, compat: { pass: s.compat.pass, of: refOk.size } };
  });
  const lines: string[] = [];
  const push = (...ls: string[]): void => {
    lines.push(...ls);
  };

  push(
    `# Benchmark: ${res.corpus} corpus`,
    "",
    `Run ${res.date} at commit \`${res.commit}\`. Corpus: ${res.diagrams} flowcharts from mermaid at \`${res.corpusCommit.slice(0, 12)}\` (tag \`mermaid@12.0.0\`, MIT; see \`bench/NOTICES.md\`), ${res.editPairs} edit pairs. Metric definitions: \`bench/README.md\`; method: \`specs/benchmark.md\`.`,
    "",
    "## Renderers",
    "",
    "| Renderer | Version | Rendered | Same graph as " + res.reference + " | Fits 720 px | p50 ms | p95 ms | Mean fuel |",
    "|---|---|---|---|---|---|---|---|",
  );
  for (const s of summaries) {
    push(
      `| ${s.renderer} | ${s.version} | ${pct(s.rendered, s.total)} | ${s.compat === null ? "reference" : pct(s.compat.pass, s.compat.of)} | ${pct(s.fit.pass, s.fit.of)} | ${fmt(s.msP50)} | ${fmt(s.msP95)} | ${fmt(s.fuelMean, 0)} |`,
    );
  }

  push("", "## Layout quality", "", "Mean / median over the diagrams each renderer drew. Lower is better except aspect ratio.", "");
  push(`| Metric | ${summaries.map((s) => s.renderer).join(" | ")} |`, `|---|${summaries.map(() => "---").join("|")}|`);
  for (const m of SUMMARY_METRICS) {
    const [label, digits] = METRIC_LABELS[m];
    push(`| ${label} | ${summaries.map((s) => `${fmt(s.metrics[m].mean, digits)} / ${fmt(s.metrics[m].median, digits)}`).join(" | ")} |`);
  }

  const base = res.renderers.find((r) => r.renderer === WIN_LOSS_BASELINE);
  const others = res.renderers.filter((r) => r.renderer !== WIN_LOSS_BASELINE);
  if (base !== undefined && others.length > 0) {
    push(
      "",
      `## Per diagram against ${WIN_LOSS_BASELINE}`,
      "",
      "Wins / ties / losses on diagrams both renderers drew; a win is a strictly lower value.",
      "",
      `| Metric | ${others.map((o) => o.renderer).join(" | ")} |`,
      `|---|${others.map(() => "---").join("|")}|`,
    );
    const wl = others.map((o) => winLoss(o, base));
    COMPARED_METRICS.forEach((m, k) => {
      push(`| ${METRIC_LABELS[m][0]} | ${wl.map((rows) => `${rows[k]!.win} / ${rows[k]!.tie} / ${rows[k]!.loss}`).join(" | ")} |`);
    });
  }

  if (summaries.some((s) => s.stability.pairs > 0)) {
    push(
      "",
      "## Stability",
      "",
      "Displacement of nodes surviving a one-line edit, as a fraction of the diagram's diagonal before the edit. Merlion receives the previous SVG as its layout hint.",
      "",
      "| Renderer | Pairs rendered | Mean | p95 |",
      "|---|---|---|---|",
    );
    for (const s of summaries) {
      if (s.stability.pairs === 0) continue;
      push(`| ${s.renderer} | ${s.stability.ok}/${s.stability.pairs} | ${fmt(s.stability.mean, 3)} | ${fmt(s.stability.p95, 3)} |`);
    }
  }

  push("", "## Determinism", "");
  const det = res.determinism;
  push(
    det.status === "skipped"
      ? `Native vs WASM: skipped (${det.reason ?? "no reason recorded"}).`
      : `Native vs WASM: ${pct(det.identical, det.compared)} byte-identical.${det.differing.length > 0 ? ` Differing: ${det.differing.slice(0, 10).join(", ")}${det.differing.length > 10 ? ", …" : ""}.` : ""}`,
  );

  const failing = summaries.filter((s) => s.errors.length > 0);
  if (failing.length > 0) {
    push("", "## Failures", "");
    for (const s of failing) {
      push(`**${s.renderer}**`, "");
      for (const [msg, n] of s.errors.slice(0, 5)) push(`- ${n} × ${msg.replace(/\|/g, "\\|")}`);
      push("");
    }
  }
  return `${lines.join("\n").trimEnd()}\n`;
};

// ---------------------------------------------------------------------------

/** Newest results file by name (`<date>-<commit>.json`), ties broken by modification time. */
const latestResults = Effect.gen(function* () {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const files = (yield* fs.readDirectory(RESULTS_DIR)).filter((f) => /^\d{4}-\d{2}-\d{2}-.+\.json$/.test(f));
  const withTime = yield* Effect.forEach(files, (f) =>
    fs.stat(path.join(RESULTS_DIR, f)).pipe(Effect.map((st) => [f, Option.getOrElse(st.mtime, () => new Date(0)).getTime()] as const)),
  );
  withTime.sort((a, b) => (a[0].slice(0, 10) === b[0].slice(0, 10) ? a[1] - b[1] : a[0] < b[0] ? -1 : 1));
  return Option.fromNullable(withTime[withTime.length - 1]?.[0]).pipe(Option.map((f) => path.join(RESULTS_DIR, f)));
});

export class NoResults extends Schema.TaggedError<NoResults>()("NoResults", { message: Schema.String }) {}

export const report = (input: Option.Option<string>, output: Option.Option<string>) =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const file = Option.isSome(input) ? input : yield* latestResults;
    if (Option.isNone(file)) return yield* new NoResults({ message: "no results file; run `pnpm bench run` first" });
    const text = yield* fs.readFileString(file.value);
    const res = yield* Schema.decodeUnknown(Schema.parseJson(ResultsFile))(text);
    const md = renderReport(res);
    const out = Option.getOrElse(output, () => file.value.replace(/\.json$/, ".md"));
    yield* fs.writeFileString(out, md);
    yield* Console.log(`report: ${path.relative(process.cwd(), out)}`);
    return out;
  });
