import { describe, expect, it } from "vitest";
import {
  applyMatrix,
  boxIntersectionArea,
  boxOfPoints,
  distPointBox,
  IDENTITY,
  multiply,
  parseTransform,
  polylineLength,
  segmentIntersection,
} from "../src/svg/geom.ts";
import { parsePath } from "../src/svg/path.ts";

describe("transforms", () => {
  it("parses translate, scale, matrix and composes left to right", () => {
    const m = parseTransform("translate(10, 20) scale(2)");
    expect(applyMatrix(m, { x: 1, y: 1 })).toEqual({ x: 12, y: 22 });
    expect(applyMatrix(parseTransform("translate(5)"), { x: 0, y: 0 })).toEqual({ x: 5, y: 0 });
    expect(applyMatrix(parseTransform("matrix(1 0 0 1 3 4)"), { x: 1, y: 1 })).toEqual({ x: 4, y: 5 });
    const r = applyMatrix(parseTransform("rotate(90)"), { x: 1, y: 0 });
    expect(r.x).toBeCloseTo(0);
    expect(r.y).toBeCloseTo(1);
    expect(parseTransform("garbage(1)")).toEqual(IDENTITY);
    expect(parseTransform(undefined)).toEqual(IDENTITY);
    const parent = parseTransform("translate(100,0)");
    expect(applyMatrix(multiply(parent, m), { x: 1, y: 1 })).toEqual({ x: 112, y: 22 });
  });
});

describe("boxes and segments", () => {
  it("computes bounding boxes, overlaps and distances", () => {
    const b = boxOfPoints([
      { x: 1, y: 2 },
      { x: 5, y: -1 },
    ])!;
    expect(b).toEqual({ x: 1, y: -1, w: 4, h: 3 });
    expect(boxOfPoints([])).toBeNull();
    expect(boxIntersectionArea({ x: 0, y: 0, w: 10, h: 10 }, { x: 5, y: 5, w: 10, h: 10 })).toBe(25);
    expect(boxIntersectionArea({ x: 0, y: 0, w: 1, h: 1 }, { x: 2, y: 2, w: 1, h: 1 })).toBe(0);
    expect(distPointBox({ x: 13, y: 14 }, { x: 0, y: 0, w: 10, h: 10 })).toBe(5);
    expect(distPointBox({ x: 5, y: 5 }, { x: 0, y: 0, w: 10, h: 10 })).toBe(0);
  });

  it("finds proper intersections only", () => {
    const p = segmentIntersection({ x: 0, y: 0 }, { x: 10, y: 10 }, { x: 0, y: 10 }, { x: 10, y: 0 });
    expect(p).toEqual({ x: 5, y: 5 });
    // Parallel, collinear-overlapping and touching-at-endpoint segments do not cross.
    expect(segmentIntersection({ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 0, y: 1 }, { x: 10, y: 1 })).toBeNull();
    expect(segmentIntersection({ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 5, y: 0 }, { x: 15, y: 0 })).toBeNull();
    expect(segmentIntersection({ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 })).toBeNull();
    expect(polylineLength([{ x: 0, y: 0 }, { x: 3, y: 4 }, { x: 3, y: 10 }])).toBe(11);
  });
});

describe("parsePath", () => {
  it("parses absolute and relative lines", () => {
    const subs = parsePath("M0,0 L10,0 l0,10 H0 v-5 Z");
    expect(subs).toHaveLength(1);
    expect(subs[0]!.points).toEqual([
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 10, y: 10 },
      { x: 0, y: 10 },
      { x: 0, y: 5 },
      { x: 0, y: 0 },
    ]);
    expect(subs[0]!.closed).toBe(true);
    expect(subs[0]!.vertices).toHaveLength(6);
  });

  it("samples cubic and quadratic curves through their endpoints", () => {
    const [c] = parsePath("M0,0C0,10,10,10,10,0");
    expect(c!.points[0]).toEqual({ x: 0, y: 0 });
    expect(c!.points[c!.points.length - 1]).toEqual({ x: 10, y: 0 });
    expect(c!.points.length).toBeGreaterThan(5);
    // The apex of this symmetric cubic is at y = 7.5.
    expect(Math.max(...c!.points.map((p) => p.y))).toBeCloseTo(7.5, 1);
    // Curves contribute only their endpoints as vertices.
    expect(c!.vertices).toEqual([{ x: 0, y: 0 }, { x: 10, y: 0 }]);
    const [q] = parsePath("M0 0 Q5 10 10 0 T20 0");
    expect(q!.points[q!.points.length - 1]).toEqual({ x: 20, y: 0 });
  });

  it("handles implicit repeats, compact numbers and multiple subpaths", () => {
    const subs = parsePath("M0 0 10 0 20 0M-1.5.5l1e1-2");
    expect(subs).toHaveLength(2);
    expect(subs[0]!.points).toHaveLength(3);
    expect(subs[1]!.points).toEqual([
      { x: -1.5, y: 0.5 },
      { x: 8.5, y: -1.5 },
    ]);
  });

  it("samples arcs and survives garbage", () => {
    const [a] = parsePath("M0,0 A5,5 0 0 1 10,0");
    const last = a!.points[a!.points.length - 1]!;
    expect(last.x).toBeCloseTo(10);
    expect(last.y).toBeCloseTo(0);
    expect(Math.min(...a!.points.map((p) => p.y))).toBeCloseTo(-5, 0);
    expect(parsePath("hello")).toEqual([]);
    expect(parsePath("M1,1 L")).toEqual([{ points: [{ x: 1, y: 1 }], vertices: [{ x: 1, y: 1 }], closed: false }]);
  });
});
