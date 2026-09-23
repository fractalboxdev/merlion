/**
 * State-diagram extraction from rendered SVG (specs/benchmark.md#output,
 * specs/state.md#svg-output).
 *
 * A state machine is a routed graph on both sides, so the drawing reduces to an
 * [`ExtractedGraph`] — states are nodes, transitions are edges, composite states
 * are clusters — and the flowchart metrics (crossings, bends, label overlaps,
 * stress) apply to it unchanged. Notes are geometry beside the graph, so they
 * come out separately.
 *
 * Merlion output is read through its data attributes; mermaid output through the
 * class names and ids of its `statediagram` renderer, with `htmlLabels: false`
 * so every label is a `<text>`. mermaid draws a note as a node whose id ends in
 * `----note-<n>`, wraps its state in a `note-cluster`, and links the two with a
 * `note-edge`; none of the three is part of the graph.
 */
import {
  buildCtx,
  collect,
  type Ctx,
  estimateTextBox,
  type ExtractedEdge,
  type ExtractedGraph,
  type ExtractedNode,
  type Flavor,
  isShape,
  labelText,
  nearestNode,
  parseViewBox,
  shapeBox,
} from "./extract.ts";
import { applyMatrix, type Box, IDENTITY, polylineLength, type Point, unionBoxes } from "./geom.ts";
import { parsePath } from "./path.ts";
import { classList, hasClass, parseXml, type XmlElement } from "./xml.ts";

export interface StateNote {
  readonly text: string;
  readonly box: Box | null;
}

export interface StateDrawing {
  readonly flavor: Flavor;
  /** States as nodes, transitions as edges, composite states as clusters. */
  readonly graph: ExtractedGraph;
  readonly notes: readonly StateNote[];
}

const EMPTY_GRAPH: ExtractedGraph = { flavor: "unknown", viewBox: null, nodes: [], edges: [], clusters: [] };
const EMPTY: StateDrawing = { flavor: "unknown", graph: EMPTY_GRAPH, notes: [] };

/** Box of every shape in `el`, not descending into the groups `prune` names. */
const groupBox = (ctx: Ctx, el: XmlElement, prune: (e: XmlElement) => boolean): Box | null =>
  unionBoxes(
    collect(el, isShape, prune)
      .map((s) => shapeBox(ctx, s))
      .filter((b): b is Box => b !== null),
  );

/** The longest path in a group is its line; the rest are arrowheads and decorations. */
const longestPath = (ctx: Ctx, el: XmlElement, prune: (e: XmlElement) => boolean = () => false) => {
  let best: { points: Point[]; vertices: Point[]; len: number } | null = null;
  for (const p of collect(el, (e) => e.name === "path", prune)) {
    const mat = ctx.ctm.get(p) ?? IDENTITY;
    for (const sp of parsePath(p.attrs["d"] ?? "")) {
      const points = sp.points.map((q) => applyMatrix(mat, q));
      const len = polylineLength(points);
      if (best === null || len > best.len) best = { points, vertices: sp.vertices.map((q) => applyMatrix(mat, q)), len };
    }
  }
  return best;
};

// ---------------------------------------------------------------------------
// Merlion

const merlionDrawing = (root: XmlElement, ctx: Ctx): StateDrawing => {
  const isGroup = (cls: string) => (e: XmlElement) => e.name === "g" && hasClass(e, cls);
  const isItem = (e: XmlElement) =>
    isGroup("merlion-node")(e) || isGroup("merlion-edge")(e) || isGroup("merlion-cluster")(e) || isGroup("merlion-note")(e);
  const text = (g: XmlElement) => labelText(collect(g, (e) => e.name === "text", isItem));

  const nodes: ExtractedNode[] = [];
  for (const g of collect(root, isGroup("merlion-node"))) {
    const box = groupBox(ctx, g, (e) => e.name === "text" || isItem(e));
    if (box === null) continue;
    nodes.push({ id: g.attrs["data-merlion-id"] ?? "", label: text(g), box });
  }

  // A concurrency region is `merlion-region`, not `merlion-cluster`, so only the
  // composite states land here (specs/state.md#groups-and-data-attributes).
  const clusters = collect(root, isGroup("merlion-cluster")).flatMap((g) => {
    const box = shapeBox(ctx, collect(g, isShape, (e) => e.name === "text" || isItem(e))[0] ?? g);
    return box === null ? [] : [{ id: g.attrs["data-merlion-id"] ?? "", label: text(g), box }];
  });

  const edges: ExtractedEdge[] = [];
  for (const g of collect(root, isGroup("merlion-edge"))) {
    const best = longestPath(ctx, g, isItem);
    if (best === null) continue;
    const texts = collect(g, (e) => e.name === "text", isItem);
    const label = labelText(texts);
    const chips = collect(g, (e) => e.name === "rect", isItem)
      .map((r) => shapeBox(ctx, r))
      .filter((b): b is Box => b !== null);
    edges.push({
      from: g.attrs["data-merlion-from"] ?? nearestNode(best.points[0], nodes),
      to: g.attrs["data-merlion-to"] ?? nearestNode(best.points[best.points.length - 1], nodes),
      label,
      points: best.points,
      vertices: best.vertices,
      labelBox:
        label.length === 0
          ? null
          : (unionBoxes(chips) ?? unionBoxes(texts.map((t) => estimateTextBox(ctx, t)).filter((b): b is Box => b !== null))),
    });
  }

  const notes = collect(root, isGroup("merlion-note")).map((g) => ({
    text: text(g),
    box: unionBoxes(
      collect(g, (e) => hasClass(e, "merlion-note-box"), isItem)
        .map((s) => shapeBox(ctx, s))
        .filter((b): b is Box => b !== null),
    ),
  }));

  return { flavor: "merlion", graph: { flavor: "merlion", viewBox: parseViewBox(root), nodes, edges, clusters }, notes };
};

// ---------------------------------------------------------------------------
// mermaid

/** `<svg id>-state-<state id>-<n>`, the id mermaid gives a state's group. */
const MERMAID_STATE_ID = /(?:^|-)state-(.+)-\d+$/;
/** mermaid's generated ids for the two elements it adds around a note. */
const NOTE_SUFFIX = /----(note|parent)$/;
/**
 * mermaid draws a concurrency divider as a node of its own; Merlion draws the
 * dashed line the UML notation asks for and no node. Neither is a state.
 */
const DIVIDER_ID = /^divider-id-\d+$/;

const mermaidDrawing = (root: XmlElement, ctx: Ctx): StateDrawing => {
  const svgId = root.attrs["id"] ?? "";
  const strip = (id: string): string => (svgId !== "" && id.startsWith(`${svgId}-`) ? id.slice(svgId.length + 1) : id);
  const idOf = (g: XmlElement): string => {
    const raw = strip(g.attrs["id"] ?? "");
    return MERMAID_STATE_ID.exec(raw)?.[1] ?? raw;
  };
  const isLabelGroup = (e: XmlElement) => e.name === "g" && (hasClass(e, "label") || hasClass(e, "cluster-label"));
  const prune = (e: XmlElement) => e.name === "defs" || e.name === "marker" || e.name === "text" || isLabelGroup(e);
  // A composite state's group is `statediagram-cluster`, a flowchart subgraph's is `cluster`.
  const isCluster = (e: XmlElement) => hasClass(e, "cluster") || hasClass(e, "statediagram-cluster");

  const noteGroups = collect(root, (e) => e.name === "g" && hasClass(e, "statediagram-note"));
  const notes: StateNote[] = noteGroups.map((g) => ({
    text: labelText(collect(g, (e) => e.name === "text")),
    box: groupBox(ctx, g, prune),
  }));

  const nodes: ExtractedNode[] = [];
  for (const g of collect(
    root,
    (e) => e.name === "g" && classList(e).includes("node") && !hasClass(e, "statediagram-note") && !isCluster(e),
    (e) => e.name === "defs" || e.name === "marker",
  )) {
    const id = idOf(g);
    if (NOTE_SUFFIX.test(id) || DIVIDER_ID.test(id)) continue;
    const box = groupBox(ctx, g, prune);
    if (box === null) continue;
    nodes.push({ id, label: labelText(collect(g, (e) => e.name === "text")), box });
  }

  // `note-cluster` wraps a noted state to keep its note beside it; it draws nothing.
  const clusters = collect(root, (e) => e.name === "g" && isCluster(e) && !hasClass(e, "note-cluster")).flatMap(
    (g) => {
      const box = groupBox(ctx, g, prune);
      return box === null ? [] : [{ id: g.attrs["data-id"] ?? idOf(g), label: labelText(collect(g, (e) => e.name === "text")), box }];
    },
  );

  // One `g.edgeLabel > g.label[data-id]` per edge, keyed by the edge's `data-id`.
  const labels = new Map<string, { label: string; box: Box | null }>();
  for (const lg of collect(root, (e) => e.name === "g" && hasClass(e, "edgeLabel"))) {
    const key = collect(lg, (e) => e.name === "g" && "data-id" in e.attrs)[0]?.attrs["data-id"];
    if (key === undefined) continue;
    const texts = collect(lg, (e) => e.name === "text");
    const label = labelText(texts);
    const bg = collect(lg, (e) => e.name === "rect")
      .map((r) => shapeBox(ctx, r))
      .filter((b): b is Box => b !== null);
    labels.set(key, {
      label,
      box:
        label.length === 0
          ? null
          : (unionBoxes(bg) ?? unionBoxes(texts.map((t) => estimateTextBox(ctx, t)).filter((b): b is Box => b !== null))),
    });
  }

  const edges: ExtractedEdge[] = [];
  for (const p of collect(
    root,
    (e) => e.name === "path" && hasClass(e, "transition") && !hasClass(e, "note-edge"),
    (e) => e.name === "defs" || e.name === "marker",
  )) {
    const mat = ctx.ctm.get(p) ?? IDENTITY;
    const subs = parsePath(p.attrs["d"] ?? "");
    const points = subs.flatMap((s) => s.points).map((q) => applyMatrix(mat, q));
    if (points.length < 2) continue;
    const lab = labels.get(p.attrs["data-id"] ?? strip(p.attrs["id"] ?? ""));
    edges.push({
      from: nearestNode(points[0], nodes),
      to: nearestNode(points[points.length - 1], nodes),
      label: lab?.label ?? "",
      points,
      vertices: subs.flatMap((s) => s.vertices).map((q) => applyMatrix(mat, q)),
      labelBox: lab?.box ?? null,
    });
  }

  return { flavor: "mermaid", graph: { flavor: "mermaid", viewBox: parseViewBox(root), nodes, edges, clusters }, notes };
};

// ---------------------------------------------------------------------------

/** True when the root is a mermaid state drawing. */
export const isMermaidState = (root: XmlElement): boolean =>
  root.attrs["aria-roledescription"] === "stateDiagram" || hasClass(root, "statediagram");

/** True when the root is a Merlion state drawing. */
export const isMerlionState = (root: XmlElement): boolean => hasClass(root, "merlion-state");

/** Extracts the drawn state machine from an SVG string. Never throws; unknown input is empty. */
export const extractState = (svg: string): StateDrawing => {
  let root: XmlElement;
  try {
    root = parseXml(svg);
  } catch {
    return EMPTY;
  }
  if (root.name !== "svg") return EMPTY;
  const ctx = buildCtx(root);
  if (isMerlionState(root)) return merlionDrawing(root, ctx);
  if (isMermaidState(root)) return mermaidDrawing(root, ctx);
  return EMPTY;
};
