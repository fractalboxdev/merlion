/**
 * Scoring a drawing against the graph its task declared
 * (specs/benchmark.md#authoring).
 *
 * The comparison is over labels, never ids: a model picks its own ids in
 * Mermaid and has none at all in SVG, so a node is what a reader sees written
 * in it and an edge is the pair of labels it runs between. That makes one
 * function score both output formats, which is the only way the two are
 * comparable at all.
 */
import type { ExtractedGraph } from "../svg/extract.ts";
import type { RefGraph } from "./tasks.ts";

/** Labels compare case-insensitively, without punctuation the renderer or the model may add. */
export const canon = (s: string): string =>
  s
    .toLowerCase()
    .replace(/[\s ]+/g, " ")
    .trim()
    .replace(/[.,;:!?]+$/g, "");

/** Size of the multiset intersection of two label lists. */
export const multisetMatch = (a: readonly string[], b: readonly string[]): number => {
  const counts = new Map<string, number>();
  for (const x of a) counts.set(x, (counts.get(x) ?? 0) + 1);
  let matched = 0;
  for (const y of b) {
    const c = counts.get(y) ?? 0;
    if (c > 0) {
      counts.set(y, c - 1);
      matched++;
    }
  }
  return matched;
};

const f1 = (matched: number, got: number, ref: number): number => {
  if (got === 0 || ref === 0) return matched === 0 && got === 0 && ref === 0 ? 1 : 0;
  const p = matched / got;
  const r = matched / ref;
  return p + r === 0 ? 0 : (2 * p * r) / (p + r);
};

export interface Fidelity {
  readonly refNodes: number;
  readonly gotNodes: number;
  readonly nodesMatched: number;
  readonly nodeF1: number;
  readonly refEdges: number;
  readonly gotEdges: number;
  readonly edgesMatched: number;
  readonly edgeF1: number;
  /** Reference edges carrying a label, and how many of them are drawn with that label on a matching edge. */
  readonly labelledEdges: number;
  readonly edgeLabelsMatched: number;
  /** Drawn edges with an endpoint that resolved to no node; only geometry recovery produces these. */
  readonly danglingEdges: number;
}

const pairKey = (from: string, to: string): string => `${from}\u0000${to}`;

export const fidelity = (ref: RefGraph, got: ExtractedGraph): Fidelity => {
  const refLabelOf = new Map(ref.nodes.map((n) => [n.id, canon(n.label)]));
  const refNodeLabels = ref.nodes.map((n) => canon(n.label));
  const gotNodeLabels = got.nodes.map((n) => canon(n.label)).filter((l) => l !== "");
  const nodesMatched = multisetMatch(refNodeLabels, gotNodeLabels);

  const gotLabelOf = new Map(got.nodes.map((n) => [n.id, canon(n.label)]));
  const refPairs = ref.edges.map((e) => pairKey(refLabelOf.get(e.from) ?? "", refLabelOf.get(e.to) ?? ""));
  const gotPairs: string[] = [];
  let danglingEdges = 0;
  for (const e of got.edges) {
    const from = e.from === null ? undefined : gotLabelOf.get(e.from);
    const to = e.to === null ? undefined : gotLabelOf.get(e.to);
    if (from === undefined || to === undefined || from === "" || to === "") {
      danglingEdges++;
      continue;
    }
    gotPairs.push(pairKey(from, to));
  }
  const edgesMatched = multisetMatch(refPairs, gotPairs);

  // Edge labels: a reference edge counts when a drawn edge runs between the
  // same two labels and carries the same text.
  const drawnLabels = new Map<string, string[]>();
  for (const e of got.edges) {
    const from = e.from === null ? undefined : gotLabelOf.get(e.from);
    const to = e.to === null ? undefined : gotLabelOf.get(e.to);
    if (from === undefined || to === undefined) continue;
    const key = pairKey(from, to);
    drawnLabels.set(key, [...(drawnLabels.get(key) ?? []), canon(e.label)]);
  }
  let labelledEdges = 0;
  let edgeLabelsMatched = 0;
  for (const e of ref.edges) {
    if (e.label === "") continue;
    labelledEdges++;
    const key = pairKey(refLabelOf.get(e.from) ?? "", refLabelOf.get(e.to) ?? "");
    const candidates = drawnLabels.get(key);
    const at = candidates?.indexOf(canon(e.label)) ?? -1;
    if (candidates !== undefined && at >= 0) {
      candidates.splice(at, 1);
      edgeLabelsMatched++;
    }
  }

  return {
    refNodes: ref.nodes.length,
    gotNodes: gotNodeLabels.length,
    nodesMatched,
    nodeF1: f1(nodesMatched, gotNodeLabels.length, refNodeLabels.length),
    refEdges: ref.edges.length,
    gotEdges: gotPairs.length,
    edgesMatched,
    edgeF1: f1(edgesMatched, gotPairs.length, refPairs.length),
    labelledEdges,
    edgeLabelsMatched,
    danglingEdges,
  };
};
