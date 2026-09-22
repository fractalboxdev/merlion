import { describe, expect, it } from "vitest";
import { pseudoStateCount, stateCompat, stateLabels, stateMetrics } from "../src/metrics/state.ts";
import type { ExtractedEdge, ExtractedGraph } from "../src/svg/extract.ts";
import type { StateDrawing } from "../src/svg/state.ts";

const box = (x: number, y: number, w = 60, h = 30) => ({ x, y, w, h });

const edge = (label: string, from: string, to: string, points: Array<[number, number]>): ExtractedEdge => ({
  from,
  to,
  label,
  points: points.map(([x, y]) => ({ x, y })),
  vertices: points.map(([x, y]) => ({ x, y })),
  labelBox: label === "" ? null : box(0, 0, 40, 16),
});

const drawing = (o: {
  states?: Array<[string, string]>;
  composites?: Array<[string, string]>;
  edges?: ExtractedEdge[];
  notes?: string[];
  size?: [number, number];
}): StateDrawing => {
  const graph: ExtractedGraph = {
    flavor: "merlion",
    viewBox: { x: 0, y: 0, w: o.size?.[0] ?? 400, h: o.size?.[1] ?? 300 },
    nodes: (o.states ?? []).map(([id, label], k) => ({ id, label, box: box(0, 60 * k) })),
    edges: o.edges ?? [],
    clusters: (o.composites ?? []).map(([id, label], k) => ({ id, label, box: box(200, 60 * k, 180, 120) })),
  };
  return { flavor: "merlion", graph, notes: (o.notes ?? []).map((text) => ({ text, box: box(300, 0) })) };
};

describe("stateLabels", () => {
  it("names states and composite states by the label a reader sees", () => {
    const d = drawing({ states: [["Draft", "Draft"], ["root_start", ""]], composites: [["Review", "Review"]] });
    expect(stateLabels(d)).toEqual(["Draft", "Review"]);
  });

  it("counts the states that draw a bare symbol instead of naming them", () => {
    const d = drawing({ states: [["root_start", ""], ["pick", ""], ["Draft", "Draft"]] });
    expect(pseudoStateCount(d)).toBe(2);
  });
});

describe("stateCompat", () => {
  const ref = drawing({
    states: [["root_start", ""], ["Draft", "Draft"]],
    composites: [["Review", "Review"]],
    edges: [edge("submit", "Draft", "Review", [[0, 0], [0, 60]])],
    notes: ["watch out"],
  });

  it("passes when both renderers drew the same content", () => {
    const cand = drawing({
      // Different ids for the same scope, which the comparison ignores.
      states: [["divider-id-1_start", ""], ["Draft", "Draft"]],
      composites: [["Review", "Review"]],
      edges: [edge("submit", "Draft", "Review", [[0, 0], [0, 80]])],
      notes: ["watch  out"],
    });
    const c = stateCompat(ref, cand);
    expect(c.pass).toBe(true);
    expect(c.missingStates).toEqual([]);
    expect(c.extraStates).toEqual([]);
  });

  it("reports the state labels one side drew and the other did not", () => {
    const cand = drawing({
      states: [["root_start", ""], ["Draft", "Draft"]],
      composites: [["Review", "Reviewing"]],
      edges: [edge("submit", "Draft", "Review", [[0, 0], [0, 60]])],
      notes: ["watch out"],
    });
    const c = stateCompat(ref, cand);
    expect(c.pass).toBe(false);
    expect(c.states).toBe(false);
    expect(c.missingStates).toEqual(["Review"]);
    expect(c.extraStates).toEqual(["Reviewing"]);
  });

  it("fails on a differing pseudo-state count, which no label would show", () => {
    const cand = drawing({
      states: [["root_start", ""], ["root_end", ""], ["Draft", "Draft"]],
      composites: [["Review", "Review"]],
      edges: [edge("submit", "Draft", "Review", [[0, 0], [0, 60]])],
      notes: ["watch out"],
    });
    const c = stateCompat(ref, cand);
    expect(c.pass).toBe(false);
    expect(c.pseudoStates).toBe(false);
    expect([c.refPseudo, c.candPseudo]).toEqual([1, 2]);
  });

  it("fails on a differing transition count and on differing transition labels", () => {
    const more = drawing({
      states: [["root_start", ""], ["Draft", "Draft"]],
      composites: [["Review", "Review"]],
      edges: [edge("submit", "Draft", "Review", [[0, 0], [0, 60]]), edge("", "Review", "Draft", [[0, 60], [0, 0]])],
      notes: ["watch out"],
    });
    expect(stateCompat(ref, more).transitionCount).toBe(false);

    const renamed = drawing({
      states: [["root_start", ""], ["Draft", "Draft"]],
      composites: [["Review", "Review"]],
      edges: [edge("send", "Draft", "Review", [[0, 0], [0, 60]])],
      notes: ["watch out"],
    });
    expect(stateCompat(ref, renamed).transitionLabels).toBe(false);
  });

  it("fails on a note text one side drew and the other did not", () => {
    const cand = drawing({
      states: [["root_start", ""], ["Draft", "Draft"]],
      composites: [["Review", "Review"]],
      edges: [edge("submit", "Draft", "Review", [[0, 0], [0, 60]])],
      notes: [],
    });
    const c = stateCompat(ref, cand);
    expect(c.notes).toBe(false);
    expect([c.refNotes, c.candNotes]).toEqual([1, 0]);
  });
});

describe("stateMetrics", () => {
  it("counts the drawn content and measures the drawing", () => {
    const d = drawing({
      states: [["root_start", ""], ["Draft", "Draft"]],
      composites: [["Review", "Review"]],
      edges: [edge("submit", "Draft", "Review", [[0, 0], [0, 60]])],
      notes: ["watch out"],
      size: [640, 480],
    });
    const m = stateMetrics(d);
    expect(m).toMatchObject({ states: 2, transitions: 1, composites: 1, notes: 1, width: 640, height: 480, fits720: true });
    expect(m.area).toBe(640 * 480);
  });

  it("counts a note box overlapping a state, which the flowchart overlap count never sees", () => {
    const clear = drawing({ states: [["A", "A"]], notes: ["n"] });
    expect(stateMetrics(clear).overlapsWithNotes).toBe(0);
    const clash: StateDrawing = {
      ...clear,
      notes: [{ text: "n", box: box(10, 10) }],
    };
    expect(stateMetrics(clash).labelOverlaps).toBe(0);
    expect(stateMetrics(clash).overlapsWithNotes).toBe(1);
  });

  it("marks a drawing wider than 720 px as not fitting", () => {
    expect(stateMetrics(drawing({ states: [["A", "A"]], size: [721, 200] })).fits720).toBe(false);
  });
});
