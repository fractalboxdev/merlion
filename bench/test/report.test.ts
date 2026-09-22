import { describe, expect, it } from "vitest";
import { renderReport, summarise, winLoss } from "../src/report.ts";
import type { DiagramResult, RendererResult, ResultsFile } from "../src/schema.ts";

const metrics = (crossings: number, width = 500) => ({
  nodes: 3,
  edges: 2,
  crossings,
  maxCrossingsPerEdge: crossings,
  bends: 1,
  edgeLength: 100,
  width,
  height: 100,
  area: width * 100,
  aspectRatio: width / 100,
  labelOverlaps: 0,
  stress: 0.1,
  fits720: width <= 720,
});

const diag = (name: string, crossings: number | null, extra: Partial<DiagramResult> = {}): DiagramResult => ({
  name,
  ok: crossings !== null,
  error: crossings === null ? "Parse error on line 2" : null,
  ms: 10,
  fuelUsed: null,
  svgBytes: crossings === null ? 0 : 1000,
  metrics: crossings === null ? null : metrics(crossings),
  compat: null,
  ...extra,
});

const renderer = (name: string, diagrams: DiagramResult[]): RendererResult => ({ renderer: name, version: "1", diagrams, edits: [] });

describe("summarise", () => {
  it("aggregates rendered diagrams and groups errors", () => {
    const s = summarise(renderer("x", [diag("a", 2), diag("b", 4), diag("c", null)]), "ref");
    expect(s.rendered).toBe(2);
    expect(s.metrics.crossings).toEqual({ mean: 3, median: 3 });
    expect(s.fit).toEqual({ pass: 2, of: 2 });
    expect(s.errors).toEqual([["Parse error on line 2", 1]]);
    expect(summarise(renderer("ref", []), "ref").compat).toBeNull();
  });
});

describe("winLoss", () => {
  it("compares diagrams both renderers drew", () => {
    const base = renderer("mermaid-elk", [diag("a", 2), diag("b", 2), diag("c", 2), diag("d", null)]);
    const cand = renderer("merlion", [diag("a", 1), diag("b", 2), diag("c", 3), diag("d", 0)]);
    const row = winLoss(cand, base).find((r) => r.metric === "crossings")!;
    expect([row.win, row.tie, row.loss]).toEqual([1, 1, 1]);
  });
});

describe("renderReport", () => {
  it("renders every section for a mermaid-only run", () => {
    const res: ResultsFile = {
      date: "2026-09-22",
      commit: "abc1234",
      corpus: "compat",
      corpusCommit: "98a0945418c76238f15df2afaddbba4272656c3b",
      diagrams: 2,
      editPairs: 0,
      reference: "mermaid-dagre",
      renderers: [
        renderer("mermaid-dagre", [diag("a", 1), diag("b", null)]),
        renderer("mermaid-elk", [
          diag("a", 0, { compat: { pass: true, nodeLabels: true, edgeCount: true, edgeLabels: true, missingNodeLabels: [], extraNodeLabels: [], refEdges: 2, candEdges: 2 } }),
          diag("b", 3),
        ]),
      ],
      determinism: { status: "skipped", reason: "merlion was not among the renderers", compared: 0, identical: 0, differing: [] },
    };
    const md = renderReport(res);
    expect(md).toContain("# Benchmark: compat corpus");
    expect(md).toContain("| mermaid-dagre | 1 | 50.0% (1/2) | reference |");
    expect(md).toContain("| mermaid-elk | 1 | 100.0% (2/2) | 100.0% (1/1) |");
    expect(md).toContain("## Per diagram against mermaid-elk");
    expect(md).toContain("| Crossings | 0 / 0 / 1 |");
    expect(md).toContain("skipped (merlion was not among the renderers)");
    expect(md).toContain("- 1 × Parse error on line 2");
    expect(md.endsWith("\n")).toBe(true);
  });
});
