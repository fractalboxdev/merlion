/**
 * Sequence metrics (specs/benchmark.md#metrics, specs/sequence.md).
 *
 * A sequence diagram has no routed graph, so crossings, bends, edge length and
 * stress do not apply: participants are columns in source order and messages are
 * rows. What is comparable between two renderers is the content each drew — the
 * participant labels, the message count and labels, the notes — and the size and
 * legibility of the drawing.
 */
import { boxIntersectionArea } from "../svg/geom.ts";
import { labelBoxes, type SequenceDrawing } from "../svg/sequence.ts";

/** A drawing wider than this scrolls or zooms in `<merlion-view>`. */
export const FIT_WIDTH = 720;

export interface SequenceMetrics {
  readonly participants: number;
  readonly messages: number;
  readonly notes: number;
  readonly fragments: number;
  readonly width: number;
  readonly height: number;
  readonly area: number;
  readonly fits720: boolean;
  readonly labelOverlaps: number;
}

export interface SequenceCompat {
  /** Every check below except `fragments`, which the two renderers define differently. */
  readonly pass: boolean;
  readonly participants: boolean;
  readonly messageCount: boolean;
  readonly messageLabels: boolean;
  readonly notes: boolean;
  readonly fragments: boolean;
  readonly missingParticipants: readonly string[];
  readonly extraParticipants: readonly string[];
  readonly refMessages: number;
  readonly candMessages: number;
}

/**
 * Labels compared as a multiset with whitespace removed: the two renderers wrap
 * at different points, and neither wrap position is a compatibility claim. The
 * zero-width characters mermaid inserts at a wrap point go with the whitespace,
 * for the same reason.
 */
const multiset = (labels: readonly string[]): string[] =>
  labels
    .map((l) => l.replace(/[\s\u200B-\u200D\uFEFF]+/g, ""))
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

export const sequenceMetrics = (d: SequenceDrawing): SequenceMetrics => {
  const w = d.viewBox?.w ?? 0;
  const h = d.viewBox?.h ?? 0;
  const boxes = labelBoxes(d);
  let overlaps = 0;
  for (let i = 0; i < boxes.length; i++) {
    for (let j = i + 1; j < boxes.length; j++) if (boxIntersectionArea(boxes[i]!, boxes[j]!) > 1) overlaps++;
  }
  return {
    participants: d.participants.length,
    messages: d.messages.length,
    notes: d.notes.length,
    fragments: d.fragments.length,
    width: w,
    height: h,
    area: w * h,
    fits720: w <= FIT_WIDTH,
    labelOverlaps: overlaps,
  };
};

/**
 * Compatibility against the reference drawing of the same source: the same
 * participant labels, the same message count, the same non-empty message labels
 * and the same note texts. `rect` is left out of the fragment comparison —
 * mermaid tints the rows and draws no kind tab, so it has no fragment to read.
 */
export const sequenceCompat = (ref: SequenceDrawing, cand: SequenceDrawing): SequenceCompat => {
  const refParts = multiset(ref.participants.map((p) => p.label));
  const candParts = multiset(cand.participants.map((p) => p.label));
  const participants = sameMultiset(refParts, candParts);
  const messageCount = ref.messages.length === cand.messages.length;
  const messageLabels = sameMultiset(multiset(ref.messages.map((m) => m.label)), multiset(cand.messages.map((m) => m.label)));
  const notes = sameMultiset(multiset(ref.notes.map((n) => n.text)), multiset(cand.notes.map((n) => n.text)));
  const kinds = (d: SequenceDrawing) => d.fragments.map((f) => f.kind).filter((k) => k !== "rect").sort();
  const fragments = sameMultiset(kinds(ref), kinds(cand));
  return {
    pass: participants && messageCount && messageLabels && notes,
    participants,
    messageCount,
    messageLabels,
    notes,
    fragments,
    missingParticipants: missing(refParts, candParts),
    extraParticipants: missing(candParts, refParts),
    refMessages: ref.messages.length,
    candMessages: cand.messages.length,
  };
};
