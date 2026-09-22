import { describe, expect, it } from "vitest";
import type { ExtractedEdge, ExtractedGraph, ExtractedNode } from "../src/svg/extract.ts";
import type { Point } from "../src/svg/geom.ts";
import {
  compareGraphs,
  countBends,
  crossings,
  displacement,
  labelOverlaps,
  layoutMetrics,
  quantile,
  simplify,
  stress,
} from "../src/metrics/metrics.ts";

const node = (id: string, x: number, y: number, label = id, w = 20, h = 20): ExtractedNode => ({
  id,
  label,
  box: { x: x - w / 2, y: y - h / 2, w, h },
});

const edge = (from: string | null, to: string | null, pts: Point[], label = "", labelBox: ExtractedEdge["labelBox"] = null): ExtractedEdge => ({
  from,
  to,
  label,
  points: pts,
  vertices: pts,
  labelBox,
});

const graph = (nodes: ExtractedNode[], edges: ExtractedEdge[], w = 100, h = 100): ExtractedGraph => ({
  flavor: "merlion",
  viewBox: { x: 0, y: 0, w, h },
  nodes,
  edges,
  clusters: [],
});

describe("crossings", () => {
  it("counts proper crossings between distinct edges", () => {
    // Two diagonals of a square cross once; a third edge crosses both.
    const g = graph(
      [node("a", 0, 0), node("b", 100, 100), node("c", 100, 0), node("d", 0, 100), node("e", 50, -50), node("f", 50, 150)],
      [
        edge("a", "b", [{ x: 0, y: 0 }, { x: 100, y: 100 }]),
        edge("c", "d", [{ x: 100, y: 0 }, { x: 0, y: 100 }]),
        edge("e", "f", [{ x: 49, y: -50 }, { x: 49, y: 150 }]),
      ],
    );
    expect(crossings(g)).toEqual({ total: 3, maxPerEdge: 2 });
  });

  it("ignores crossings inside a shared endpoint's box", () => {
    // Edges from "a" whose sampled curves cross right at the node border.
    const g = graph(
      [node("a", 50, 0, "a", 40, 20), node("b", 0, 100), node("c", 100, 100)],
      [
        edge("a", "b", [{ x: 45, y: 10 }, { x: 55, y: 14 }, { x: 60, y: 100 }]),
        edge("a", "c", [{ x: 55, y: 10 }, { x: 45, y: 14 }, { x: 40, y: 100 }]),
      ],
    );
    expect(crossings(g).total).toBe(0);
  });

  it("does not count a self-overlap or segments that only touch", () => {
    const g = graph(
      [node("a", 0, 0), node("b", 100, 0)],
      [edge("a", "b", [{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 50, y: 50 }, { x: 50, y: -50 }])],
    );
    expect(crossings(g).total).toBe(0);
  });
});

describe("bends", () => {
  it("counts corners of an orthogonal route", () => {
    expect(countBends([{ x: 0, y: 0 }, { x: 0, y: 50 }, { x: 100, y: 50 }, { x: 100, y: 100 }])).toBe(2);
    expect(countBends([{ x: 0, y: 0 }, { x: 0, y: 50 }, { x: 0, y: 100 }])).toBe(0);
  });
  it("collapses a rounded corner and ignores sampling noise on a straight line", () => {
    const arc: Point[] = [{ x: 0, y: 0 }, { x: 0, y: 43 }];
    for (let k = 1; k <= 8; k++) {
      const th = (Math.PI / 2) * (k / 8);
      arc.push({ x: 7 - 7 * Math.cos(th), y: 43 + 7 * Math.sin(th) });
    }
    arc.push({ x: 100, y: 50 });
    expect(countBends(arc)).toBe(1);
    const wobble = Array.from({ length: 20 }, (_, k) => ({ x: k * 10, y: k % 2 === 0 ? 0 : 0.5 }));
    expect(countBends(wobble)).toBe(0);
  });
  it("simplify keeps endpoints", () => {
    expect(simplify([{ x: 0, y: 0 }], 3)).toEqual([{ x: 0, y: 0 }]);
    expect(simplify([{ x: 0, y: 0 }, { x: 5, y: 1 }, { x: 10, y: 0 }], 3)).toEqual([{ x: 0, y: 0 }, { x: 10, y: 0 }]);
  });
});

describe("labelOverlaps", () => {
  it("counts intersecting node and edge-label boxes", () => {
    const g = graph(
      [node("a", 0, 0), node("b", 10, 10), node("c", 100, 100)],
      [
        edge("a", "c", [{ x: 0, y: 0 }, { x: 100, y: 100 }], "x", { x: 95, y: 95, w: 10, h: 10 }),
        edge("b", "c", [{ x: 0, y: 0 }, { x: 100, y: 100 }], "y", { x: 50, y: 50, w: 10, h: 10 }),
      ],
    );
    // a–b overlap, label x overlaps c.
    expect(labelOverlaps(g)).toBe(2);
  });
});

describe("stress", () => {
  it("is zero for a path laid out at uniform spacing", () => {
    const g = graph(
      [node("a", 0, 0), node("b", 50, 0), node("c", 100, 0)],
      [edge("a", "b", []), edge("b", "c", [])],
    );
    expect(stress(g)).toBeCloseTo(0, 10);
  });
  it("is scale invariant and positive for a distorted layout", () => {
    const g1 = graph([node("a", 0, 0), node("b", 10, 0), node("c", 11, 0)], [edge("a", "b", []), edge("b", "c", [])]);
    const g2 = graph([node("a", 0, 0), node("b", 100, 0), node("c", 110, 0)], [edge("a", "b", []), edge("b", "c", [])]);
    expect(stress(g1)!).toBeGreaterThan(0.05);
    expect(stress(g1)!).toBeCloseTo(stress(g2)!, 10);
  });
  it("is null without connected pairs", () => {
    expect(stress(graph([node("a", 0, 0), node("b", 1, 1)], []))).toBeNull();
  });
});

describe("layoutMetrics", () => {
  it("reports area, aspect ratio, fit and edge length", () => {
    const g = graph([node("a", 0, 0), node("b", 0, 100)], [edge("a", "b", [{ x: 0, y: 10 }, { x: 0, y: 90 }])], 800, 400);
    const m = layoutMetrics(g);
    expect(m.area).toBe(320000);
    expect(m.aspectRatio).toBe(2);
    expect(m.fits720).toBe(false);
    expect(m.edgeLength).toBe(80);
    expect(m.nodes).toBe(2);
    expect(m.edges).toBe(1);
    expect(m.bends).toBe(0);
  });
});

describe("compareGraphs", () => {
  const ref = graph([node("a", 0, 0, "Start"), node("b", 0, 0, "End")], [edge("a", "b", [], "go")]);
  it("passes on equal label multisets and edge counts", () => {
    const cand = graph([node("x", 5, 5, "End"), node("y", 5, 5, "Start")], [edge("y", "x", [], "go")]);
    expect(compareGraphs(ref, cand).pass).toBe(true);
  });
  it("fails on a dropped edge or changed label", () => {
    expect(compareGraphs(ref, graph(ref.nodes as ExtractedNode[], [])).pass).toBe(false);
    const r = compareGraphs(ref, graph([node("a", 0, 0, "Start"), node("b", 0, 0, "Fin")], ref.edges as ExtractedEdge[]));
    expect(r.nodeLabels).toBe(false);
    expect(r.edgeCount).toBe(true);
    expect(r.missingNodeLabels).toEqual(["End"]);
    expect(r.extraNodeLabels).toEqual(["Fin"]);
  });
});

describe("displacement", () => {
  it("matches surviving nodes by id and normalises by the diagonal", () => {
    const before = graph([node("a", 10, 10), node("b", 50, 50)], [], 30, 40);
    const after = graph([node("a", 10, 10), node("b", 50, 60), node("c", 0, 0)], [], 30, 40);
    const d = displacement(before, after);
    expect(d.matched).toBe(2);
    expect(d.values).toEqual([0, 10 / 50]);
  });
  it("uses viewBox-relative coordinates", () => {
    const before: ExtractedGraph = { ...graph([node("a", 10, 10)], []), viewBox: { x: -10, y: 0, w: 30, h: 40 } };
    const after = graph([node("a", 20, 10)], [], 30, 40);
    expect(displacement(before, after).values).toEqual([0]);
  });
});

describe("quantile", () => {
  it("interpolates linearly", () => {
    expect(quantile([1, 2, 3, 4], 0.5)).toBe(2.5);
    expect(quantile([5], 0.95)).toBe(5);
    expect(quantile([], 0.5)).toBeNull();
    expect(quantile([10, 0, 20], 1)).toBe(20);
  });
});
