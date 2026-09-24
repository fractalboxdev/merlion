/**
 * Graph recovery from arbitrary SVG (specs/benchmark.md#authoring).
 *
 * The `authoring` corpus scores drawings nobody's renderer produced: an SVG a
 * model wrote by hand, with no class names, no data attributes and no
 * convention beyond the SVG element set. The graph has to be recovered from
 * geometry the way a reader recovers it — a filled closed shape holding a
 * label is a node, an open stroked path between two shapes is an edge — which
 * is the method DiagramEval uses to score generated diagrams (Liang and You,
 * EMNLP 2025).
 *
 * Recovery is lossy by construction, and the loss is measured rather than
 * assumed: `bench authoring score` runs this extractor over Merlion's own
 * drawing of each task's reference graph, where the true graph is known, and
 * reports the result as the ceiling every hand-written SVG is read against.
 */
import {
  applyMatrix,
  type Box,
  boxCenter,
  boxOfPoints,
  dist,
  distPointBox,
  IDENTITY,
  polylineLength,
  type Point,
} from "./geom.ts";
import {
  buildCtx,
  collect,
  type Ctx,
  type ExtractedEdge,
  type ExtractedGraph,
  type ExtractedNode,
  estimateTextBox,
  isShape,
  labelText,
  parseViewBox,
  shapeBox,
} from "./extract.ts";
import { parsePath, type SubPath } from "./path.ts";
import { parseXml, type XmlElement } from "./xml.ts";

/**
 * An endpoint resolves to the nearest node within this multiple of the
 * diagram's diagonal. Hand-written arrows commonly stop well short of the box
 * they point at, or run under it; a relative tolerance reads a 200 px drawing
 * and a 2000 px one the same way.
 */
const ENDPOINT_TOLERANCE_RATIO = 0.06;
/** Floor for that tolerance, in viewBox units, so a small diagram still resolves. */
const ENDPOINT_TOLERANCE_MIN = 24;
/** A shape covering at least this share of the viewBox is a background, not a node. */
const BACKGROUND_AREA_RATIO = 0.9;
/** A stroke shorter than this many viewBox units is a tick or an arrowhead, not an edge. */
const MIN_EDGE_LENGTH = 8;
/**
 * A text within this many viewBox units of a stroke's middle is that stroke's
 * label. The tolerance is absolute, not a share of the diagram: a label sits on
 * its line at any diagram size, while a share of the diagonal grows until it
 * reaches the labels of nearby nodes and takes those instead.
 */
const ON_LINE_TOLERANCE = 12;
/**
 * How far from each end of a stroke its label can sit, as a fraction of the
 * stroke's length. The ends are excluded because that is where the nodes' own
 * labels are; the distance test does most of the work, so the band stays wide.
 */
const ON_LINE_MIDDLE = 0.05;

const CLOSED_SHAPES = new Set(["rect", "circle", "ellipse", "polygon"]);
const OPEN_SHAPES = new Set(["line", "polyline", "path"]);

/** Inherited paint: an element's own attribute, else the nearest ancestor that sets one. */
const inherited = (ctx: Ctx, e: XmlElement, attr: string): string | undefined => {
  let cur: XmlElement | undefined = e;
  while (cur !== undefined) {
    const own = cur.attrs[attr];
    if (own !== undefined) return own;
    const style = cur.attrs["style"];
    if (style !== undefined) {
      const m = new RegExp(`(?:^|;)\\s*${attr}\\s*:\\s*([^;]+)`, "i").exec(style);
      if (m?.[1] !== undefined) return m[1].trim();
    }
    cur = ctx.parent.get(cur);
  }
  return undefined;
};

const isNone = (v: string | undefined): boolean => v === undefined || v.trim().toLowerCase() === "none";

/**
 * Whether a path's subpaths are strokes rather than an outline.
 *
 * A `Z` anywhere used to disqualify the whole element, which lost the very
 * common arrow that draws its shaft and a closed filled head in one `d`
 * (`M10 10 L50 10 M45 5 L50 10 L45 15 Z`). Each subpath is judged on its own
 * instead: an unclosed one is a stroke whatever its siblings do.
 */
const openSubPaths = (e: XmlElement): SubPath[] =>
  parsePath(e.attrs["d"] ?? "").filter((sub) => !sub.closed);

const pathHasOutline = (e: XmlElement): boolean =>
  parsePath(e.attrs["d"] ?? "").some((sub) => sub.closed);

/**
 * A drawn outline: a closed shape, or a closed path, that is painted. An
 * element with neither a fill nor a stroke draws nothing and is skipped.
 */
const isOutline = (ctx: Ctx, e: XmlElement): boolean => {
  if (CLOSED_SHAPES.has(e.name)) return !isNone(inherited(ctx, e, "fill")) || !isNone(inherited(ctx, e, "stroke"));
  if (e.name === "path") return pathHasOutline(e) && !isNone(inherited(ctx, e, "fill"));
  return false;
};

/** A drawn stroke: an open shape, or a path with at least one unclosed subpath, with a stroke. */
const isStroke = (ctx: Ctx, e: XmlElement): boolean => {
  if (!OPEN_SHAPES.has(e.name)) return false;
  if (e.name === "path" && openSubPaths(e).length === 0) return false;
  return !isNone(inherited(ctx, e, "stroke"));
};

/**
 * Sampled polylines of an open shape, in absolute coordinates — one per
 * subpath, because a `d` holding two `M` commands draws two separate strokes
 * and joining them would invent an edge running between them.
 */
const strokePolylines = (ctx: Ctx, e: XmlElement): Array<{ points: Point[]; vertices: Point[] }> => {
  const m = ctx.ctm.get(e) ?? IDENTITY;
  const abs = (ps: readonly Point[]): Point[] => ps.map((p) => applyMatrix(m, p));
  if (e.name === "path") {
    return openSubPaths(e).map((sub) => ({ points: abs(sub.points), vertices: abs(sub.vertices) }));
  }
  // `line` and `polyline` outline points are already the polyline itself.
  const local = e.name === "line"
    ? [
      { x: Number.parseFloat(e.attrs["x1"] ?? "0") || 0, y: Number.parseFloat(e.attrs["y1"] ?? "0") || 0 },
      { x: Number.parseFloat(e.attrs["x2"] ?? "0") || 0, y: Number.parseFloat(e.attrs["y2"] ?? "0") || 0 },
    ]
    : pointsAttr(e.attrs["points"]);
  return [{ points: abs(local), vertices: abs(local) }];
};

const pointsAttr = (s: string | undefined): Point[] => {
  const ns = (s ?? "").split(/[\s,]+/).map(Number.parseFloat).filter(Number.isFinite);
  const out: Point[] = [];
  for (let i = 0; i + 1 < ns.length; i += 2) out.push({ x: ns[i]!, y: ns[i + 1]! });
  return out;
};

/**
 * Distance from a point to the nearest point of a polyline, and where along
 * the polyline that point falls, as a fraction of its length.
 */
const nearestOnPolyline = (p: Point, pts: readonly Point[]): { distance: number; at: number } => {
  const total = polylineLength(pts);
  let best = { distance: Number.POSITIVE_INFINITY, at: 0 };
  let travelled = 0;
  for (let i = 0; i + 1 < pts.length; i++) {
    const a = pts[i]!;
    const b = pts[i + 1]!;
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const len = Math.hypot(dx, dy);
    const len2 = dx * dx + dy * dy;
    const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2));
    const distance = Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
    if (distance < best.distance) best = { distance, at: total === 0 ? 0 : (travelled + t * len) / total };
    travelled += len;
  }
  return best;
};

/**
 * Recovers the drawn graph from any SVG.
 *
 * Edge labels are read before nodes, because a label chip is an outline
 * holding text just as a node is. What separates them is the line: an arrow
 * stops at a node's boundary and runs straight through its own label's chip,
 * so a text sitting on a stroke is that stroke's label and the outline around
 * it is a chip, not a node. Everything else holding text is a node, smallest
 * enclosing outline winning so a box inside a box reads as the inner one, and
 * a text inside no outline is a node with the text's own box, because a model
 * that draws labels without boxes still means them as nodes. Markers, `<defs>`
 * contents and the background plate are excluded.
 */
export const extractGeneric = (svg: string): ExtractedGraph => {
  const root = parseXml(svg);
  const empty: ExtractedGraph = { flavor: "unknown", viewBox: null, nodes: [], edges: [], clusters: [] };
  if (root.name !== "svg") return empty;

  const ctx = buildCtx(root);
  const viewBox = parseViewBox(root);
  const diagonal = viewBox === null ? 0 : Math.hypot(viewBox.w, viewBox.h);
  const endpointTolerance = Math.max(ENDPOINT_TOLERANCE_MIN, diagonal * ENDPOINT_TOLERANCE_RATIO);
  const prune = (e: XmlElement): boolean => e.name === "defs" || e.name === "marker" || e.name === "clipPath";

  const texts = collect(root, (e) => e.name === "text", prune)
    .map((t) => ({ t, box: estimateTextBox(ctx, t) }))
    .filter((x): x is { t: XmlElement; box: Box } => x.box !== null && labelText([x.t]) !== "");

  const outlines = collect(root, (e) => isShape(e) && isOutline(ctx, e), prune)
    .map((e) => ({ e, box: shapeBox(ctx, e) }))
    .filter((o): o is { e: XmlElement; box: Box } => o.box !== null)
    .filter((o) => viewBox === null || o.box.w * o.box.h < viewBox.w * viewBox.h * BACKGROUND_AREA_RATIO);

  const strokes = collect(root, (e) => isShape(e) && isStroke(ctx, e), prune)
    .flatMap((e) => strokePolylines(ctx, e))
    .filter((s) => s.points.length >= 2 && polylineLength(s.points) >= MIN_EDGE_LENGTH);

  // Each stroke takes the text sitting on it, nearest first.
  const edgeLabelOf = new Map<number, { t: XmlElement; box: Box }>();
  const takenText = new Set<XmlElement>();
  for (const [i, s] of strokes.entries()) {
    let best: { t: XmlElement; box: Box } | null = null;
    let bestD = ON_LINE_TOLERANCE;
    for (const x of texts) {
      if (takenText.has(x.t)) continue;
      const { distance, at } = nearestOnPolyline(boxCenter(x.box), s.points);
      if (at < ON_LINE_MIDDLE || at > 1 - ON_LINE_MIDDLE) continue;
      if (distance <= bestD) {
        best = x;
        bestD = distance;
      }
    }
    if (best !== null) {
      takenText.add(best.t);
      edgeLabelOf.set(i, best);
    }
  }

  // A text belongs to the smallest outline holding its centre.
  const ownerOf = new Map<XmlElement, XmlElement>();
  const held = new Map<XmlElement, { t: XmlElement; box: Box }[]>();
  for (const x of texts) {
    let best: { e: XmlElement; box: Box } | null = null;
    for (const o of outlines) {
      if (distPointBox(boxCenter(x.box), o.box) > 0) continue;
      if (best === null || o.box.w * o.box.h < best.box.w * best.box.h) best = o;
    }
    if (best === null) continue;
    ownerOf.set(x.t, best.e);
    held.set(best.e, [...(held.get(best.e) ?? []), x]);
  }

  const nodes: ExtractedNode[] = [];
  let n = 0;
  for (const o of outlines) {
    const own = (held.get(o.e) ?? []).filter((x) => !takenText.has(x.t));
    // An outline whose only text is an edge label is that label's chip.
    if (own.length === 0) continue;
    nodes.push({ id: `n${n++}`, label: labelText(own.map((x) => x.t)), box: o.box });
  }
  // Text inside no outline, and not sitting on a stroke: a node drawn without a box.
  for (const x of texts) {
    if (takenText.has(x.t) || ownerOf.has(x.t)) continue;
    nodes.push({ id: `n${n++}`, label: labelText([x.t]), box: x.box });
  }

  const nearest = (p: Point | undefined, exclude: string | null): string | null => {
    if (p === undefined) return null;
    let best: string | null = null;
    let bestD = endpointTolerance;
    for (const node of nodes) {
      if (node.id === exclude) continue;
      const d = distPointBox(p, node.box);
      if (d <= bestD) {
        best = node.id;
        bestD = d;
      }
    }
    return best;
  };

  const edges: ExtractedEdge[] = [];
  for (const [i, s] of strokes.entries()) {
    const from = nearest(s.points[0], null);
    const to = nearest(s.points[s.points.length - 1], from);
    if (from === null && to === null) continue;
    const label = edgeLabelOf.get(i);
    edges.push({
      from,
      to,
      label: label === undefined ? "" : labelText([label.t]),
      points: s.points,
      vertices: s.vertices,
      labelBox: label?.box ?? null,
    });
  }

  return { flavor: "unknown", viewBox, nodes, edges, clusters: [] };
};

/** Bounding box of everything drawn, for the clipping check. */
export const drawnBounds = (svg: string): Box | null => {
  const root = parseXml(svg);
  if (root.name !== "svg") return null;
  const ctx = buildCtx(root);
  const prune = (e: XmlElement): boolean => e.name === "defs" || e.name === "marker" || e.name === "clipPath";
  const boxes = [
    ...collect(root, (e) => isShape(e), prune).map((e) => shapeBox(ctx, e)),
    ...collect(root, (e) => e.name === "text", prune).map((e) => estimateTextBox(ctx, e)),
  ].filter((b): b is Box => b !== null);
  return boxOfPoints(boxes.flatMap((b) => [{ x: b.x, y: b.y }, { x: b.x + b.w, y: b.y + b.h }]));
};
