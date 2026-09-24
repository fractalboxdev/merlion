/**
 * The Markdown summary of an `authoring` run (specs/benchmark.md#authoring).
 *
 * Every table holds the two formats side by side for one model, because the
 * question the corpus answers is which output format serves a model better,
 * not which model is better.
 */
import type { AuthoringResults, Row } from "./score.ts";

const pct = (x: number, of: number): string => (of === 0 ? "—" : `${((100 * x) / of).toFixed(1)}%`);
const num = (x: number | null, digits = 2): string => (x === null ? "—" : x.toFixed(digits));

const mean = (xs: readonly number[]): number | null =>
  xs.length === 0 ? null : xs.reduce((a, b) => a + b, 0) / xs.length;

/** Rows that judge a model answer: a call the provider never returned judges nothing. */
const answeredRows = (rows: readonly Row[]): readonly Row[] => rows.filter((r) => !r.callFailed);

interface Agg {
  readonly n: number;
  readonly drawn: number;
  /** Rows where the provider never returned, which judge no model output. */
  readonly callFailed: number;
  readonly truncated: number;
  readonly mermaidParsed: number | null;
  readonly nodeF1: number | null;
  readonly edgeF1: number | null;
  readonly edgeLabels: number | null;
  readonly outputTokens: number | null;
  /** Output tokens less the ones spent thinking: the diagram's own cost. */
  readonly answerTokens: number | null;
  readonly reasoning: boolean;
  readonly sourceBytes: number | null;
  readonly overflowRate: number | null;
  readonly unplaced: number;
  /** Drawings the probe could not measure; excluded from every legibility figure. */
  readonly unmeasured: number;
  readonly clipped: number;
  readonly overlaps: number;
  readonly dangling: number;
}

const aggregate = (rows: readonly Row[]): Agg => {
  const drawn = rows.filter((r) => r.drawn);
  const fid = drawn.map((r) => r.fidelity).filter((f): f is NonNullable<typeof f> => f !== null);
  /**
   * Fidelity is averaged over every answer the model gave, with one that drew
   * nothing scoring zero. Averaging over the drawn ones alone would let a model
   * raise its score by failing outright, and would compare a 17-answer mean
   * against an 18-answer one.
   */
  const scored = (pick: (f: NonNullable<Row["fidelity"]>) => number) =>
    mean(answeredRows(rows).map((r) => (r.fidelity === null ? 0 : pick(r.fidelity))));
  const leg = drawn.map((r) => r.legibility).filter((l): l is NonNullable<typeof l> => l !== null);
  const parseable = rows.filter((r) => r.mermaidParsed !== null);
  const labelled = fid.reduce((a, f) => a + f.labelledEdges, 0);
  // Labels that could overflow: one touching no shape has nothing to overflow,
  // so counting it in the denominator measures a different thing in each column.
  const labels = leg.reduce((a, l) => a + l.labels - l.unplaced, 0);
  const answered = answeredRows(rows);
  return {
    n: answered.length,
    drawn: drawn.length,
    callFailed: rows.length - answered.length,
    truncated: rows.filter((r) => r.truncated).length,
    mermaidParsed: parseable.length === 0 ? null : parseable.filter((r) => r.mermaidParsed === true).length / parseable.length,
    nodeF1: scored((f) => f.nodeF1),
    edgeF1: scored((f) => f.edgeF1),
    edgeLabels: labelled === 0 ? null : fid.reduce((a, f) => a + f.edgeLabelsMatched, 0) / labelled,
    outputTokens: mean(answered.map((r) => r.outputTokens).filter((t): t is number => t !== null)),
    answerTokens: mean(
      answered
        .filter((r) => r.outputTokens !== null)
        .map((r) => (r.outputTokens ?? 0) - (r.reasoningTokens ?? 0)),
    ),
    reasoning: answered.some((r) => (r.reasoningTokens ?? 0) > 0),
    sourceBytes: mean(answered.map((r) => r.sourceBytes)),
    overflowRate: labels === 0 ? null : leg.reduce((a, l) => a + l.overflowing, 0) / labels,
    unplaced: leg.reduce((a, l) => a + l.unplaced, 0),
    unmeasured: drawn.length - leg.length,
    clipped: leg.reduce((a, l) => a + l.clipped, 0),
    overlaps: leg.reduce((a, l) => a + l.shapeOverlaps, 0),
    dangling: fid.reduce((a, f) => a + f.danglingEdges, 0),
  };
};

const ratio = (a: number | null, b: number | null): string =>
  a === null || b === null || b === 0 ? "—" : `${(a / b).toFixed(1)}×`;

export const renderReport = (r: AuthoringResults): string => {
  const out: string[] = [];
  out.push(`# Authoring: Mermaid or SVG`);
  out.push("");
  out.push(
    `${r.tasks} tasks (corpus version ${r.tasksVersion}), each asked of every model twice — once as a Mermaid flowchart, once as a standalone SVG — with the same description and the same readability requirement. Scored ${r.date}. Method: [specs/benchmark.md](../../specs/benchmark.md#authoring).`,
  );
  out.push("");

  const ceilNode = mean(r.ceiling.map((c) => c.fidelity.nodeF1));
  const ceilEdge = mean(r.ceiling.map((c) => c.fidelity.edgeF1));
  const ceilLabels = r.ceiling.reduce((a, c) => a + c.legibility.labels, 0);
  const ceilOverflow = r.ceiling.reduce((a, c) => a + c.legibility.overflowing, 0);
  out.push("## Reader ceiling");
  out.push("");
  out.push(
    "Merlion's own drawing of each task's declared graph, read back through the same geometry recovery every hand-written SVG is read through. The graph is known to be right, so what this loses is the reader's error, not the drawing's.",
  );
  out.push("");
  out.push("| Node F1 | Edge F1 | Labels overflowing their shape | Shape overlaps | Clipped |");
  out.push("|---|---|---|---|---|");
  out.push(
    `| ${num(ceilNode, 3)} | ${num(ceilEdge, 3)} | ${pct(ceilOverflow, ceilLabels)} | ${
      r.ceiling.reduce((a, c) => a + c.legibility.shapeOverlaps, 0)
    } | ${r.ceiling.reduce((a, c) => a + c.legibility.clipped, 0)} |`,
  );
  out.push("");

  if (r.models.length === 0) {
    out.push("No recorded answers. `pnpm bench authoring generate --provider <p> --model <m>` writes one file per model.");
    out.push("");
    return `${out.join("\n")}`;
  }

  for (const m of r.models) {
    const mermaid = aggregate(m.rows.filter((x) => x.format === "mermaid"));
    const svg = aggregate(m.rows.filter((x) => x.format === "svg"));
    out.push(`## ${m.model}`);
    out.push("");
    out.push(`Answers recorded ${m.generated} through \`${m.provider}\`.`);
    out.push("");
    if (m.provider === "claude-cli") {
      out.push(
        "`claude-cli` counts the whole agentic run rather than the answer: the output figure sums every assistant turn, while only the last one is the diagram. Its token row is what the run cost, not what the diagram cost. Source bytes is the comparable measure here, and the answer-token row of a single-completion provider is the comparable token figure.",
      );
      out.push("");
    }
    out.push("| | Mermaid | SVG | SVG ÷ Mermaid |");
    out.push("|---|---|---|---|");
    out.push(`| Answers holding a diagram that draws | ${pct(mermaid.drawn, mermaid.n)} | ${pct(svg.drawn, svg.n)} | |`);
    if (mermaid.callFailed > 0 || svg.callFailed > 0) {
      out.push(`| Calls the provider never returned (excluded above) | ${mermaid.callFailed} | ${svg.callFailed} | |`);
    }
    if (mermaid.truncated > 0 || svg.truncated > 0) {
      out.push(`| Answers cut off at the token cap | ${mermaid.truncated} | ${svg.truncated} | |`);
    }
    const tokenLabel = m.provider === "claude-cli" ? "Mean output tokens over the run" : "Mean output tokens";
    out.push(`| ${tokenLabel} | ${num(mermaid.outputTokens, 0)} | ${num(svg.outputTokens, 0)} | ${ratio(svg.outputTokens, mermaid.outputTokens)} |`);
    if (mermaid.reasoning || svg.reasoning) {
      out.push(
        `| — of those, the diagram itself | ${num(mermaid.answerTokens, 0)} | ${num(svg.answerTokens, 0)} | ${
          ratio(svg.answerTokens, mermaid.answerTokens)
        } |`,
      );
    }
    out.push(`| Mean source bytes | ${num(mermaid.sourceBytes, 0)} | ${num(svg.sourceBytes, 0)} | ${ratio(svg.sourceBytes, mermaid.sourceBytes)} |`);
    out.push(`| Node F1 against the declared graph (an answer that drew nothing scores 0) | ${num(mermaid.nodeF1, 3)} | ${num(svg.nodeF1, 3)} | |`);
    out.push(`| Edge F1 against the declared graph (an answer that drew nothing scores 0) | ${num(mermaid.edgeF1, 3)} | ${num(svg.edgeF1, 3)} | |`);
    out.push(`| Labelled edges drawn with the right label | ${mermaid.edgeLabels === null ? "—" : pct(mermaid.edgeLabels, 1)} | ${svg.edgeLabels === null ? "—" : pct(svg.edgeLabels, 1)} | |`);
    out.push(`| Labels overflowing the shape they sit in | ${mermaid.overflowRate === null ? "—" : pct(mermaid.overflowRate, 1)} | ${svg.overflowRate === null ? "—" : pct(svg.overflowRate, 1)} | |`);
    out.push(`| Labels touching no shape (a title, or a chipless edge label) | ${mermaid.unplaced} | ${svg.unplaced} | |`);
    out.push(`| Shape overlaps | ${mermaid.overlaps} | ${svg.overlaps} | |`);
    if (mermaid.unmeasured > 0 || svg.unmeasured > 0) {
      out.push(`| Drawings the probe could not measure (excluded above) | ${mermaid.unmeasured} | ${svg.unmeasured} | |`);
    }
    out.push(`| Ink outside the viewBox | ${mermaid.clipped} | ${svg.clipped} | |`);
    out.push(`| Edges with an endpoint at no shape | ${mermaid.dangling} | ${svg.dangling} | |`);
    if (mermaid.mermaidParsed !== null) {
      out.push(`| Mermaid answers mermaid 12.0.0 itself parses | ${pct(mermaid.mermaidParsed, 1)} | — | |`);
    }
    out.push("");

    out.push("### Per task");
    out.push("");
    out.push("| Task | Size | Format | Drawn | Node F1 | Edge F1 | Overflowing | Overlaps | Output tokens |");
    out.push("|---|---|---|---|---|---|---|---|---|");
    for (const row of m.rows) {
      out.push(
        `| ${row.task} | ${row.size} | ${row.format} | ${row.drawn ? "yes" : `no (${row.error ?? "?"})`} | ${
          num(row.fidelity?.nodeF1 ?? null, 2)
        } | ${num(row.fidelity?.edgeF1 ?? null, 2)} | ${
          row.legibility === null ? "—" : `${row.legibility.overflowing}/${row.legibility.labels}`
        } | ${row.legibility?.shapeOverlaps ?? "—"} | ${row.outputTokens ?? "—"} |`,
      );
    }
    out.push("");
  }
  return out.join("\n");
};
