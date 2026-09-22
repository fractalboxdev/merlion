/**
 * Graph extraction from rendered SVG (specs/benchmark.md#output).
 *
 * Both renderers are read the same way: node boxes from their shapes, labels
 * from `<text>`, edges from path polylines. Merlion output is read exactly
 * through its data attributes (specs/svg-output.md#ids-and-data-attributes);
 * mermaid output (rendered with `htmlLabels: false`, so labels are `<text>`)
 * through its class names and element ids, with edge endpoints resolved from
 * the edge's `data-id` (`L_<from>_<to>_<n>`) against the known node ids and, when
 * that fails, geometrically from the nearest node box.
 */
import {
  applyMatrix,
  type Box,
  boxOfPoints,
  distPointBox,
  IDENTITY,
  type Matrix,
  multiply,
  parseNumbers,
  parseTransform,
  type Point,
  polylineLength,
  unionBoxes,
} from "./geom.ts";
import { parsePath } from "./path.ts";
import { childElements, classList, hasClass, parseXml, type XmlElement } from "./xml.ts";

export type Flavor = "merlion" | "mermaid" | "unknown";

export interface ExtractedNode {
  readonly id: string;
  readonly label: string;
  readonly box: Box;
}

export interface ExtractedEdge {
  readonly from: string | null;
  readonly to: string | null;
  readonly label: string;
  /** Sampled polyline in absolute coordinates (curves sampled). */
  readonly points: readonly Point[];
  /** Command endpoints only (no curve samples). */
  readonly vertices: readonly Point[];
  readonly labelBox: Box | null;
}

export interface ExtractedCluster {
  readonly id: string;
  readonly label: string;
  readonly box: Box;
}

export interface ExtractedGraph {
  readonly flavor: Flavor;
  readonly viewBox: Box | null;
  readonly nodes: readonly ExtractedNode[];
  readonly edges: readonly ExtractedEdge[];
  readonly clusters: readonly ExtractedCluster[];
}

/** Maximum distance (px) from an edge endpoint to a node box for geometric endpoint matching. */
const ENDPOINT_TOLERANCE = 30;
const DEFAULT_FONT_SIZE = 14;
/** Average advance used only when a label has no background box to measure (em per character). */
const EM_PER_CHAR = 0.55;

/**
 * Canonical label text for comparison: icon tokens (`fa:fa-car`) removed,
 * whitespace (including no-break space) collapsed, trimmed.
 */
export const normalizeLabel = (s: string): string =>
  s
    .replace(/\bfa[a-z]?:fa-[\w-]+/g, " ")
    .replace(/[\s ]+/g, " ")
    .trim();

// ---------------------------------------------------------------------------
// Tree context: absolute transforms and parents.

interface Ctx {
  readonly ctm: Map<XmlElement, Matrix>;
  readonly parent: Map<XmlElement, XmlElement>;
}

const buildCtx = (root: XmlElement): Ctx => {
  const ctm = new Map<XmlElement, Matrix>();
  const parent = new Map<XmlElement, XmlElement>();
  // The root's viewBox maps to the viewport; all coordinates are reported in
  // viewBox units, so the root's own transform is the identity.
  ctm.set(root, IDENTITY);
  const stack: XmlElement[] = [root];
  while (stack.length > 0) {
    const e = stack.pop();
    if (e === undefined) break;
    const m = ctm.get(e) ?? IDENTITY;
    for (const c of childElements(e)) {
      parent.set(c, e);
      ctm.set(c, multiply(m, parseTransform(c.attrs["transform"])));
      stack.push(c);
    }
  }
  return { ctm, parent };
};

/** Descendants of `el` matching `pred`, not descending into elements matching `prune`. */
const collect = (
  el: XmlElement,
  pred: (e: XmlElement) => boolean,
  prune: (e: XmlElement) => boolean = () => false,
): XmlElement[] => {
  const out: XmlElement[] = [];
  const stack: XmlElement[] = [...childElements(el)].reverse();
  while (stack.length > 0) {
    const e = stack.pop();
    if (e === undefined) break;
    if (prune(e)) continue;
    if (pred(e)) out.push(e);
    const kids = childElements(e);
    for (let k = kids.length - 1; k >= 0; k--) stack.push(kids[k]!);
  }
  return out;
};

// ---------------------------------------------------------------------------
// Shapes and text.

const SHAPES = new Set(["rect", "circle", "ellipse", "polygon", "polyline", "path", "line"]);

const num = (e: XmlElement, a: string, dflt = 0): number => {
  const v = Number.parseFloat(e.attrs[a] ?? "");
  return Number.isFinite(v) ? v : dflt;
};

/** Local (pre-transform) outline points of a shape; empty for zero-size shapes. */
const shapePoints = (e: XmlElement): Point[] => {
  switch (e.name) {
    case "rect":
    case "foreignObject": {
      const x = num(e, "x");
      const y = num(e, "y");
      const w = num(e, "width");
      const h = num(e, "height");
      if (!(w > 0 && h > 0)) return [];
      return [
        { x, y },
        { x: x + w, y: y + h },
      ];
    }
    case "circle": {
      const r = num(e, "r");
      if (!(r > 0)) return [];
      const cx = num(e, "cx");
      const cy = num(e, "cy");
      return [
        { x: cx - r, y: cy - r },
        { x: cx + r, y: cy + r },
      ];
    }
    case "ellipse": {
      const rx = num(e, "rx");
      const ry = num(e, "ry");
      if (!(rx > 0 && ry > 0)) return [];
      const cx = num(e, "cx");
      const cy = num(e, "cy");
      return [
        { x: cx - rx, y: cy - ry },
        { x: cx + rx, y: cy + ry },
      ];
    }
    case "polygon":
    case "polyline": {
      const ns = parseNumbers(e.attrs["points"] ?? "");
      const pts: Point[] = [];
      for (let k = 0; k + 1 < ns.length; k += 2) pts.push({ x: ns[k]!, y: ns[k + 1]! });
      return pts;
    }
    case "line":
      return [
        { x: num(e, "x1"), y: num(e, "y1") },
        { x: num(e, "x2"), y: num(e, "y2") },
      ];
    case "path":
      return parsePath(e.attrs["d"] ?? "").flatMap((s) => s.points);
    default:
      return [];
  }
};

/** Absolute bounding box of a shape, transforming every outline point (and both rect corners pairs). */
const shapeBox = (ctx: Ctx, e: XmlElement): Box | null => {
  const m = ctx.ctm.get(e) ?? IDENTITY;
  let pts = shapePoints(e);
  if (e.name !== "path" && pts.length === 2 && e.name !== "line") {
    // Two-point boxes: transform all four corners so rotation is handled.
    const [a, b] = pts as [Point, Point];
    pts = [a, { x: b.x, y: a.y }, b, { x: a.x, y: b.y }];
  }
  const box = boxOfPoints(pts.map((p) => applyMatrix(m, p)));
  return box !== null && (box.w > 0 || box.h > 0) ? box : null;
};

const isShape = (e: XmlElement): boolean => SHAPES.has(e.name);

/**
 * Text of a label: `<text>` content with a space inserted before every line
 * start (a `<tspan>` carrying `x`, `y` or `dy`, or mermaid's `row` class), so wrapped
 * lines and `<br>` compare equal to the source label with spaces.
 */
const labelText = (texts: readonly XmlElement[]): string => {
  let out = "";
  const walk = (e: XmlElement): void => {
    for (const c of e.children) {
      if (typeof c === "string") {
        out += c;
      } else {
        if (c.name === "tspan" && ("x" in c.attrs || "y" in c.attrs || "dy" in c.attrs || hasClass(c, "row"))) out += " ";
        walk(c);
      }
    }
  };
  for (const t of texts) {
    out += " ";
    walk(t);
  }
  return normalizeLabel(out);
};

/** Elements whose start begins a new line in an HTML label. */
const HTML_BREAKS = new Set(["br", "p", "div", "li", "tr"]);

/**
 * Text of mermaid `htmlLabels` labels: `<foreignObject>` content with a space
 * at every HTML line break or block boundary, normalised like `labelText`.
 */
const htmlLabelText = (objects: readonly XmlElement[]): string => {
  let out = "";
  const walk = (e: XmlElement): void => {
    for (const c of e.children) {
      if (typeof c === "string") {
        out += c;
      } else {
        if (HTML_BREAKS.has(c.name)) out += " ";
        walk(c);
      }
    }
  };
  for (const o of objects) {
    out += " ";
    walk(o);
  }
  return normalizeLabel(out);
};

/** A mermaid label's text: `<text>` when present, else `<foreignObject>` (a diagram that turns `htmlLabels` back on). */
const mermaidLabel = (scope: XmlElement): { label: string; texts: XmlElement[]; objects: XmlElement[] } => {
  const texts = collect(scope, (e) => e.name === "text");
  const objects = collect(scope, (e) => e.name === "foreignObject");
  const fromText = labelText(texts);
  return { label: fromText !== "" || objects.length === 0 ? fromText : htmlLabelText(objects), texts, objects };
};

/** Estimated box of a `<text>` element, used only when no background box exists. */
const estimateTextBox = (ctx: Ctx, t: XmlElement): Box | null => {
  const text = labelText([t]);
  if (text.length === 0) return null;
  const lines = Math.max(1, collect(t, (e) => e.name === "tspan" && ("x" in e.attrs || "dy" in e.attrs || hasClass(e, "row"))).length);
  const size = num(t, "font-size", DEFAULT_FONT_SIZE);
  const w = (text.length / lines) * EM_PER_CHAR * size;
  const h = lines * 1.2 * size;
  const firstTspan = collect(t, (e) => e.name === "tspan" && "x" in e.attrs)[0];
  const x = num(t, "x", firstTspan !== undefined ? num(firstTspan, "x") : 0);
  const y = num(t, "y", firstTspan !== undefined ? num(firstTspan, "y") : 0);
  const anchor = t.attrs["text-anchor"] ?? firstTspan?.attrs["text-anchor"] ?? "start";
  const x0 = anchor === "middle" ? x - w / 2 : anchor === "end" ? x - w : x;
  const m = ctx.ctm.get(t) ?? IDENTITY;
  return boxOfPoints([applyMatrix(m, { x: x0, y: y - 0.8 * size }), applyMatrix(m, { x: x0 + w, y: y - 0.8 * size + h })]);
};

const parseViewBox = (root: XmlElement): Box | null => {
  const ns = parseNumbers(root.attrs["viewBox"] ?? "");
  if (ns.length < 4) return null;
  const [x, y, w, h] = ns as [number, number, number, number];
  return { x, y, w, h };
};

// ---------------------------------------------------------------------------
// Edge endpoint matching.

const nearestNode = (p: Point | undefined, nodes: readonly ExtractedNode[]): string | null => {
  if (p === undefined) return null;
  let best: string | null = null;
  let bestD = ENDPOINT_TOLERANCE;
  for (const n of nodes) {
    const d = distPointBox(p, n.box);
    if (d <= bestD) {
      best = n.id;
      bestD = d;
    }
  }
  return best;
};

/** Resolves `L_<from>_<to>_<n>` against known ids, preferring the longest `from`. */
export const resolveMermaidEdgeId = (dataId: string, ids: ReadonlySet<string>): [string, string] | null => {
  if (!dataId.startsWith("L_")) return null;
  const body = dataId.slice(2);
  let found: [string, string] | null = null;
  for (let k = 1; k < body.length; k++) {
    if (body[k] !== "_") continue;
    const u = body.slice(0, k);
    if (!ids.has(u)) continue;
    const m = /^(.+)_\d+$/.exec(body.slice(k + 1));
    if (m?.[1] !== undefined && ids.has(m[1])) found = [u, m[1]];
  }
  return found;
};

// ---------------------------------------------------------------------------
// Merlion.

const extractMerlion = (root: XmlElement, ctx: Ctx): Omit<ExtractedGraph, "flavor" | "viewBox"> => {
  const isGroup = (cls: string) => (e: XmlElement) => e.name === "g" && hasClass(e, cls);
  const isAnyItem = (e: XmlElement) => isGroup("merlion-node")(e) || isGroup("merlion-edge")(e) || isGroup("merlion-cluster")(e);

  const nodes: ExtractedNode[] = [];
  for (const g of collect(root, isGroup("merlion-node"))) {
    const shapes = collect(g, isShape, (e) => e.name === "text" || isAnyItem(e));
    const box = unionBoxes(shapes.map((s) => shapeBox(ctx, s)).filter((b): b is Box => b !== null));
    if (box === null) continue;
    nodes.push({ id: g.attrs["data-merlion-id"] ?? "", label: labelText(collect(g, (e) => e.name === "text", isAnyItem)), box });
  }

  const clusters: ExtractedCluster[] = [];
  for (const g of collect(root, isGroup("merlion-cluster"))) {
    const shapes = collect(g, isShape, (e) => e.name === "text" || isAnyItem(e));
    const box = shapeBox(ctx, shapes[0] ?? g);
    if (box === null) continue;
    clusters.push({ id: g.attrs["data-merlion-id"] ?? "", label: labelText(collect(g, (e) => e.name === "text", isAnyItem)), box });
  }

  const edges: ExtractedEdge[] = [];
  for (const g of collect(root, isGroup("merlion-edge"))) {
    const m = ctx.ctm;
    // The edge line is the longest path in the group; others are arrowheads or decorations.
    let best: { points: Point[]; vertices: Point[]; len: number } | null = null;
    for (const p of collect(g, (e) => e.name === "path", isAnyItem)) {
      const mat = m.get(p) ?? IDENTITY;
      for (const sp of parsePath(p.attrs["d"] ?? "")) {
        const points = sp.points.map((q) => applyMatrix(mat, q));
        const len = polylineLength(points);
        if (best === null || len > best.len) best = { points, vertices: sp.vertices.map((q) => applyMatrix(mat, q)), len };
      }
    }
    if (best === null) continue;
    const texts = collect(g, (e) => e.name === "text", isAnyItem);
    const label = labelText(texts);
    const chips = collect(g, (e) => e.name === "rect", isAnyItem)
      .map((r) => shapeBox(ctx, r))
      .filter((b): b is Box => b !== null);
    const labelBox =
      label.length === 0
        ? null
        : (unionBoxes(chips) ?? unionBoxes(texts.map((t) => estimateTextBox(ctx, t)).filter((b): b is Box => b !== null)));
    edges.push({
      from: g.attrs["data-merlion-from"] ?? nearestNode(best.points[0], nodes),
      to: g.attrs["data-merlion-to"] ?? nearestNode(best.points[best.points.length - 1], nodes),
      label,
      points: best.points,
      vertices: best.vertices,
      labelBox,
    });
  }
  return { nodes, edges, clusters };
};

// ---------------------------------------------------------------------------
// Mermaid.

const MERMAID_NODE_ID = /(?:^|-)flowchart-(.+)-\d+$/;

/** Decodes mermaid's `data-points` attribute; null when absent or malformed. */
export const decodeDataPoints = (attr: string | undefined): Point[] | null => {
  if (attr === undefined) return null;
  try {
    const parsed: unknown = JSON.parse(Buffer.from(attr, "base64").toString("utf8"));
    if (!Array.isArray(parsed)) return null;
    const pts: Point[] = [];
    for (const q of parsed) {
      const x = (q as { x?: unknown }).x;
      const y = (q as { y?: unknown }).y;
      if (typeof x !== "number" || typeof y !== "number" || !Number.isFinite(x) || !Number.isFinite(y)) return null;
      pts.push({ x, y });
    }
    return pts.length >= 2 ? pts : null;
  } catch {
    return null;
  }
};

const extractMermaid = (root: XmlElement, ctx: Ctx): Omit<ExtractedGraph, "flavor" | "viewBox"> => {
  const svgId = root.attrs["id"] ?? "";
  const stripPrefix = (id: string): string => (svgId !== "" && id.startsWith(`${svgId}-`) ? id.slice(svgId.length + 1) : id);
  const isLabelGroup = (e: XmlElement) => e.name === "g" && (hasClass(e, "label") || hasClass(e, "cluster-label"));

  const nodes: ExtractedNode[] = [];
  // Node groups carry `node` (classic look), `rough-node` (hand-drawn look) or a
  // shape class such as `icon-shape`; all carry a `…flowchart-<id>-<n>` id.
  const nodeGroups = collect(
    root,
    (e) =>
      e.name === "g" &&
      !hasClass(e, "cluster") &&
      (hasClass(e, "node") || hasClass(e, "rough-node") || MERMAID_NODE_ID.test(e.attrs["id"] ?? "")),
    (e) => e.name === "defs" || e.name === "marker",
  );
  for (const g of nodeGroups) {
    const shapes = collect(g, isShape, (e) => e.name === "text" || isLabelGroup(e));
    const box = unionBoxes(shapes.map((s) => shapeBox(ctx, s)).filter((b): b is Box => b !== null));
    if (box === null) continue;
    const rawId = g.attrs["id"] ?? "";
    const id = MERMAID_NODE_ID.exec(rawId)?.[1] ?? stripPrefix(rawId);
    nodes.push({ id, label: mermaidLabel(g).label, box });
  }

  const clusters: ExtractedCluster[] = [];
  for (const g of collect(root, (e) => e.name === "g" && hasClass(e, "cluster"))) {
    const shapes = collect(g, isShape, (e) => e.name === "text" || isLabelGroup(e));
    const box = shapes.length > 0 ? shapeBox(ctx, shapes[0]!) : null;
    if (box === null) continue;
    clusters.push({ id: stripPrefix(g.attrs["id"] ?? ""), label: mermaidLabel(g).label, box });
  }

  // Edge labels keyed by the edge's data-id.
  const labels = new Map<string, { label: string; box: Box | null }>();
  for (const lg of collect(root, (e) => e.name === "g" && hasClass(e, "edgeLabel"))) {
    const inner = collect(lg, (e) => e.name === "g" && "data-id" in e.attrs)[0];
    const key = inner?.attrs["data-id"];
    if (key === undefined) continue;
    const { label, texts, objects } = mermaidLabel(lg);
    const bg = [...collect(lg, (e) => e.name === "rect"), ...objects]
      .map((r) => shapeBox(ctx, r))
      .filter((b): b is Box => b !== null);
    const box =
      label.length === 0
        ? null
        : (unionBoxes(bg) ?? unionBoxes(texts.map((t) => estimateTextBox(ctx, t)).filter((b): b is Box => b !== null)));
    labels.set(key, { label, box });
  }

  // Edges may end at a subgraph, so cluster ids resolve endpoints too.
  const ids = new Set([...nodes.map((n) => n.id), ...clusters.map((c) => c.id)]);
  const edges: ExtractedEdge[] = [];
  const edgePaths = collect(
    root,
    (e) => e.name === "path" && (hasClass(e, "flowchart-link") || e.attrs["data-edge"] === "true"),
    (e) => e.name === "defs" || e.name === "marker",
  );
  for (const p of edgePaths) {
    const mat = ctx.ctm.get(p) ?? IDENTITY;
    const subs = parsePath(p.attrs["d"] ?? "");
    // A multi-stroke path (hand-drawn look) is not one polyline; its routed
    // points are in `data-points` (base64 JSON of {x, y}).
    const routed = subs.length > 1 ? decodeDataPoints(p.attrs["data-points"]) : null;
    const points = (routed ?? subs.flatMap((s) => s.points)).map((q) => applyMatrix(mat, q));
    const vertices = (routed ?? subs.flatMap((s) => s.vertices)).map((q) => applyMatrix(mat, q));
    if (points.length < 2) continue;
    const dataId = p.attrs["data-id"] ?? stripPrefix(p.attrs["id"] ?? "");
    const ends = resolveMermaidEdgeId(dataId, ids);
    const lab = labels.get(dataId);
    edges.push({
      from: ends?.[0] ?? nearestNode(points[0], nodes),
      to: ends?.[1] ?? nearestNode(points[points.length - 1], nodes),
      label: lab?.label ?? "",
      points,
      vertices,
      labelBox: lab?.box ?? null,
    });
  }
  return { nodes, edges, clusters };
};

// ---------------------------------------------------------------------------

export const detectFlavor = (root: XmlElement): Flavor => {
  if (root.name !== "svg") return "unknown";
  if ("data-merlion-version" in root.attrs || classList(root).includes("merlion")) return "merlion";
  return "mermaid";
};

/** Extracts the drawn graph from an SVG string. Never throws; unknown input yields an empty graph. */
export const extractSvg = (svg: string): ExtractedGraph => {
  const root = parseXml(svg);
  const flavor = detectFlavor(root);
  if (flavor === "unknown") return { flavor, viewBox: null, nodes: [], edges: [], clusters: [] };
  const ctx = buildCtx(root);
  const body = flavor === "merlion" ? extractMerlion(root, ctx) : extractMermaid(root, ctx);
  return { flavor, viewBox: parseViewBox(root), ...body };
};
