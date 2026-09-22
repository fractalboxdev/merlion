/**
 * State metrics (specs/benchmark.md#metrics, specs/state.md).
 *
 * A state machine lowers to a routed graph, so the flowchart measures apply
 * unchanged — crossings, bends, label overlaps, size and fit come from
 * [`layoutMetrics`] over the extracted graph. What is state-specific is the
 * content each renderer drew: the states, the transitions and their labels, and
 * the notes.
 */
import { boxIntersectionArea } from "../svg/geom.ts";
import type { StateDrawing } from "../svg/state.ts";
import { type LayoutMetrics, layoutMetrics, OVERLAP_MIN_AREA } from "./metrics.ts";

export interface StateMetrics extends LayoutMetrics {
  readonly states: number;
  readonly transitions: number;
  readonly composites: number;
  readonly notes: number;
  /** Label overlaps with the note boxes counted in, which no flowchart has. */
  readonly overlapsWithNotes: number;
}

export interface StateCompat {
  /** Every check below: the same states, transition count, transition labels and notes. */
  readonly pass: boolean;
  readonly states: boolean;
  readonly pseudoStates: boolean;
  readonly transitionCount: boolean;
  readonly transitionLabels: boolean;
  readonly notes: boolean;
  readonly missingStates: readonly string[];
  readonly extraStates: readonly string[];
  readonly refPseudo: number;
  readonly candPseudo: number;
  readonly refTransitions: number;
  readonly candTransitions: number;
  readonly refNotes: number;
  readonly candNotes: number;
}

/**
 * Labels compared as a multiset with whitespace removed: the two renderers wrap
 * at different points, and neither wrap position is a compatibility claim. The
 * zero-width characters mermaid inserts at a wrap point go with the whitespace,
 * for the same reason.
 */
const multiset = (labels: readonly string[]): string[] =>
  labels
    .map((l) => l.replace(/[\s​-‍﻿]+/g, ""))
    .filter((l) => l.length > 0)
    .sort();

const sameMultiset = (a: readonly string[], b: readonly string[]): boolean =>
  a.length === b.length && a.every((x, k) => x === b[k]);

/** Items of `a` that `b` does not hold, counting duplicates. */
const missing = (a: readonly string[], b: readonly string[]): string[] => {
  const rest = [...b];
  const out: string[] = [];
  for (const x of a) {
    const k = rest.indexOf(x);
    if (k < 0) out.push(x);
    else rest.splice(k, 1);
  }
  return out;
};

/**
 * The labels the drawing shows, over states and composite states alike. A state
 * is named by what a reader sees, not by its id: mermaid's `state "…" as id`
 * draws the description, and the two renderers generate different ids for the
 * same scope (`Active_r0_start` against `divider-id-1_start`), so an id
 * comparison would report a difference no reader can see.
 */
export const stateLabels = (d: StateDrawing): string[] =>
  [...d.graph.nodes.map((n) => n.label), ...d.graph.clusters.map((c) => c.label)].filter((l) => l.trim() !== "");

/** States drawn as a bare symbol: `[*]`, a choice diamond, a fork or join bar. */
export const pseudoStateCount = (d: StateDrawing): number => d.graph.nodes.filter((n) => n.label.trim() === "").length;

export const stateMetrics = (d: StateDrawing): StateMetrics => {
  const base = layoutMetrics(d.graph);
  const boxes = [
    ...d.graph.nodes.map((n) => n.box),
    ...d.graph.edges.flatMap((e) => (e.labelBox === null ? [] : [e.labelBox])),
    ...d.notes.flatMap((n) => (n.box === null ? [] : [n.box])),
  ];
  let withNotes = 0;
  for (let i = 0; i < boxes.length; i++) {
    for (let j = i + 1; j < boxes.length; j++) {
      if (boxIntersectionArea(boxes[i]!, boxes[j]!) > OVERLAP_MIN_AREA) withNotes++;
    }
  }
  return {
    ...base,
    states: d.graph.nodes.length,
    transitions: d.graph.edges.length,
    composites: d.graph.clusters.length,
    notes: d.notes.length,
    overlapsWithNotes: withNotes,
  };
};

/**
 * Compatibility against the reference drawing of the same source: the same state
 * labels, the same number of pseudo-states, the same transition count, the same
 * non-empty transition labels and the same note texts. Transition endpoints are
 * left out — mermaid encodes none in its state output, so they would be compared
 * against a geometric guess.
 */
export const stateCompat = (ref: StateDrawing, cand: StateDrawing): StateCompat => {
  const refStates = multiset(stateLabels(ref));
  const candStates = multiset(stateLabels(cand));
  const states = sameMultiset(refStates, candStates);
  const [refPseudo, candPseudo] = [pseudoStateCount(ref), pseudoStateCount(cand)];
  const transitionCount = ref.graph.edges.length === cand.graph.edges.length;
  const transitionLabels = sameMultiset(
    multiset(ref.graph.edges.map((e) => e.label)),
    multiset(cand.graph.edges.map((e) => e.label)),
  );
  const notes = sameMultiset(multiset(ref.notes.map((n) => n.text)), multiset(cand.notes.map((n) => n.text)));
  return {
    pass: states && refPseudo === candPseudo && transitionCount && transitionLabels && notes,
    states,
    pseudoStates: refPseudo === candPseudo,
    transitionCount,
    transitionLabels,
    notes,
    missingStates: missing(refStates, candStates),
    extraStates: missing(candStates, refStates),
    refPseudo,
    candPseudo,
    refTransitions: ref.graph.edges.length,
    candTransitions: cand.graph.edges.length,
    refNotes: ref.notes.length,
    candNotes: cand.notes.length,
  };
};
