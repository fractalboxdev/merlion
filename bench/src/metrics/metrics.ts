/**
 * Layout-quality, compatibility and stability metrics over extracted graphs
 * (specs/benchmark.md#metrics). Every function is pure and renderer-agnostic.
 */
import type { ExtractedGraph, ExtractedNode } from "../svg/extract.ts";
import {
  type Box,
  boxCenter,
  boxCorners,
  boxIntersectionArea,
  dist,
  distPointBox,
  inflate,
  type Point,
  polylineLength,
  segmentIntersection,
} from "../svg/geom.ts";

/** Crossings within this distance (px) of a shared endpoint's box are not counted. */
export const SHARED_ENDPOINT_MARGIN = 8;
/** Douglas–Peucker tolerance (px) applied before counting bends. */
export const BEND_TOLERANCE = 3;
/** Minimum summed turn (degrees) for a group of simplified vertices to count as a bend. */
export const BEND_MIN_ANGLE = 10;
/** Simplified vertices closer than this (px) merge into one bend. */
export const BEND_MERGE_DISTANCE = 12;
/** Minimum intersection area (px²) for two boxes to count as overlapping. */
export const OVERLAP_MIN_AREA = 1;
export const FIT_WIDTH = 720;

// ---------------------------------------------------------------------------
// Crossings.

/**
 * Crossings between distinct edges: proper intersections of their polylines
 * (sampled curves). For two edges sharing an endpoint node, intersections
 * within `SHARED_ENDPOINT_MARGIN` of that node's box are excluded: they are
 * where adjacent edges meet, not crossings a reader has to resolve.
 * Intersections of one pair closer than 1 px are counted once.
 */
export const crossings = (g: ExtractedGraph): { total: number; maxPerEdge: number } => {
  const boxes = new Map<string, Box>([...g.clusters.map((c) => [c.id, c.box] as const), ...g.nodes.map((n) => [n.id, n.box] as const)]);
  const per = new Array<number>(g.edges.length).fill(0);
  let total = 0;
  for (let i = 0; i < g.edges.length; i++) {
    const a = g.edges[i]!;
    for (let j = i + 1; j < g.edges.length; j++) {
      const b = g.edges[j]!;
      const shared = [a.from, a.to]
        .filter((id): id is string => id !== null && (id === b.from || id === b.to))
        .map((id) => boxes.get(id))
        .filter((bx): bx is Box => bx !== undefined)
        .map((bx) => inflate(bx, SHARED_ENDPOINT_MARGIN));
      const found: Point[] = [];
      for (let s = 1; s < a.points.length; s++) {
        for (let t = 1; t < b.points.length; t++) {
          const p = segmentIntersection(a.points[s - 1]!, a.points[s]!, b.points[t - 1]!, b.points[t]!);
          if (p === null) continue;
          if (shared.some((bx) => distPointBox(p, bx) === 0)) continue;
          if (found.some((q) => dist(p, q) < 1)) continue;
          found.push(p);
        }
      }
      total += found.length;
      per[i]! += found.length;
      per[j]! += found.length;
    }
  }
  return { total, maxPerEdge: per.reduce((m, v) => Math.max(m, v), 0) };
};

// ---------------------------------------------------------------------------
// Bends.

const pointSegmentDistance = (p: Point, a: Point, b: Point): number => {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy;
  if (len2 === 0) return dist(p, a);
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2));
  return dist(p, { x: a.x + t * dx, y: a.y + t * dy });
};

/** Douglas–Peucker simplification (Douglas and Peucker, 1973), iterative. */
export const simplify = (pts: readonly Point[], tol: number): Point[] => {
  if (pts.length <= 2) return [...pts];
  const keep = new Array<boolean>(pts.length).fill(false);
  keep[0] = true;
  keep[pts.length - 1] = true;
  const stack: Array<[number, number]> = [[0, pts.length - 1]];
  while (stack.length > 0) {
    const [lo, hi] = stack.pop()!;
    let far = -1;
    let farD = tol;
    for (let k = lo + 1; k < hi; k++) {
      const d = pointSegmentDistance(pts[k]!, pts[lo]!, pts[hi]!);
      if (d > farD) {
        far = k;
        farD = d;
      }
    }
    if (far >= 0) {
      keep[far] = true;
      stack.push([lo, far], [far, hi]);
    }
  }
  return pts.filter((_, k) => keep[k]);
};

/**
 * Bends of one edge: interior vertices of its polyline after Douglas–Peucker
 * simplification at `BEND_TOLERANCE`, grouped while closer than
 * `BEND_MERGE_DISTANCE`; a group whose summed signed turn exceeds
 * `BEND_MIN_ANGLE` is one bend. This makes the count comparable between
 * orthogonal routes (a rounded corner counts once) and sampled splines (a
 * gentle S-curve counts as its visible turns, not as its sample count).
 */
export const countBends = (pts: readonly Point[]): number => {
  const s = simplify(pts, BEND_TOLERANCE);
  let bends = 0;
  // Consecutive vertices closer than BEND_MERGE_DISTANCE form one bend whose
  // turn is the sum of their signed turns (a rounded corner is one bend).
  let clusterTurn = 0;
  let last: Point | null = null;
  const flush = (): void => {
    if ((Math.abs(clusterTurn) * 180) / Math.PI > BEND_MIN_ANGLE) bends++;
    clusterTurn = 0;
  };
  for (let k = 1; k + 1 < s.length; k++) {
    const a = s[k - 1]!;
    const b = s[k]!;
    const c = s[k + 1]!;
    let turn = Math.atan2(c.y - b.y, c.x - b.x) - Math.atan2(b.y - a.y, b.x - a.x);
    if (turn > Math.PI) turn -= 2 * Math.PI;
    if (turn < -Math.PI) turn += 2 * Math.PI;
    if (last !== null && dist(last, b) >= BEND_MERGE_DISTANCE) flush();
    clusterTurn += turn;
    last = b;
  }
  flush();
  return bends;
};

// ---------------------------------------------------------------------------
// Label overlaps.

/**
 * Pairs of overlapping label boxes: every node box (the node's label lives in
 * it) and every edge-label box, counting each intersecting pair with area over
 * `OVERLAP_MIN_AREA` once.
 */
export const labelOverlaps = (g: ExtractedGraph): number => {
  const boxes: Box[] = [...g.nodes.map((n) => n.box), ...g.edges.flatMap((e) => (e.labelBox === null ? [] : [e.labelBox]))];
  let count = 0;
  for (let i = 0; i < boxes.length; i++) {
    for (let j = i + 1; j < boxes.length; j++) {
      if (boxIntersectionArea(boxes[i]!, boxes[j]!) > OVERLAP_MIN_AREA) count++;
    }
  }
  return count;
};

const inBox = (p: Point, b: Box): boolean => p.x >= b.x && p.x <= b.x + b.w && p.y >= b.y && p.y <= b.y + b.h;

/** Whether the polyline enters `b`: a point inside it, or a segment crossing a side. */
const polylineHitsBox = (pts: readonly Point[], b: Box): boolean => {
  if (pts.some((p) => inBox(p, b))) return true;
  const corners = boxCorners(b);
  for (let k = 1; k < pts.length; k++) {
    for (let c = 0; c < corners.length; c++) {
      const s = corners[c]!;
      const t = corners[(c + 1) % corners.length]!;
      if (segmentIntersection(pts[k - 1]!, pts[k]!, s, t) !== null) return true;
    }
  }
  return false;
};

/**
 * Edges drawn through another edge's label chip: pairs `(edge, chip)` where the
 * edge's polyline enters the chip of a different edge, counted once per pair.
 *
 * Box-against-box overlap misses this. Each edge group draws its own path and
 * then its chip, so a later edge covers an earlier chip and the text under it is
 * unreadable, while the two chips never touch and `labelOverlaps` stays zero.
 * Cyclic graphs that label their back-edges hit it most
 * (specs/benchmark.md#metrics).
 */
export const edgesThroughLabels = (g: ExtractedGraph): number => {
  let count = 0;
  for (let i = 0; i < g.edges.length; i++) {
    const e = g.edges[i]!;
    for (let j = 0; j < g.edges.length; j++) {
      if (i === j) continue;
      const chip = g.edges[j]!.labelBox;
      if (chip === null) continue;
      if (polylineHitsBox(e.points, chip)) count++;
    }
  }
  return count;
};

// ---------------------------------------------------------------------------
// Stress.

/**
 * Normalised stress of the drawing (Kamada and Kawai, 1989; Gansner, Koren and
 * North, 2004), with the drawing scaled by the factor that minimises it, so
 * the value does not depend on the drawing's unit. Mooney et al. (GD 2025)
 * motivate it as a reader-perceivable quality measure (specs/benchmark.md):
 *
 *   stress = (1/P) · Σ_{i<j} ((s·‖x_i − x_j‖ − d_ij) / d_ij)²,
 *   s = Σ (‖x_i − x_j‖ / d_ij) / Σ (‖x_i − x_j‖² / d_ij²)
 *
 * where d_ij is the undirected shortest-path length between nodes i and j,
 * x is the node box centre, the sum runs over the P pairs connected in the
 * graph (weights w_ij = d_ij⁻²), and s minimises the sum. 0 means graph distance
 * is reproduced exactly. Returns null when no pair of nodes is connected.
 */
export const stress = (g: ExtractedGraph): number | null => {
  const idx = new Map<string, number>();
  const nodes: ExtractedNode[] = [];
  for (const n of g.nodes) {
    if (idx.has(n.id)) continue;
    idx.set(n.id, nodes.length);
    nodes.push(n);
  }
  const adj: number[][] = nodes.map(() => []);
  for (const e of g.edges) {
    const a = e.from === null ? undefined : idx.get(e.from);
    const b = e.to === null ? undefined : idx.get(e.to);
    if (a === undefined || b === undefined || a === b) continue;
    adj[a]!.push(b);
    adj[b]!.push(a);
  }
  const pos = nodes.map((n) => boxCenter(n.box));
  const ratios: Array<[number, number]> = []; // [euclidean, graph distance]
  for (let s = 0; s < nodes.length; s++) {
    // Breadth-first search from s; pairs i < j only.
    const d = new Array<number>(nodes.length).fill(-1);
    d[s] = 0;
    const queue = [s];
    for (let q = 0; q < queue.length; q++) {
      const u = queue[q]!;
      for (const v of adj[u]!) {
        if (d[v] === -1) {
          d[v] = d[u]! + 1;
          queue.push(v);
        }
      }
    }
    for (let t = s + 1; t < nodes.length; t++) {
      if (d[t]! > 0) ratios.push([dist(pos[s]!, pos[t]!), d[t]!]);
    }
  }
  if (ratios.length === 0) return null;
  let num = 0;
  let den = 0;
  for (const [e, d] of ratios) {
    num += e / d;
    den += (e * e) / (d * d);
  }
  const scale = den === 0 ? 0 : num / den;
  let sum = 0;
  for (const [e, d] of ratios) sum += ((scale * e - d) / d) ** 2;
  return sum / ratios.length;
};

// ---------------------------------------------------------------------------
// Whole-drawing metrics.

export interface LayoutMetrics {
  readonly nodes: number;
  readonly edges: number;
  readonly crossings: number;
  readonly maxCrossingsPerEdge: number;
  readonly bends: number;
  readonly edgeLength: number;
  readonly width: number;
  readonly height: number;
  readonly area: number;
  readonly aspectRatio: number | null;
  readonly labelOverlaps: number;
  /** Edges drawn through another edge's label chip ([`edgesThroughLabels`]). */
  readonly edgesThroughLabels: number;
  readonly stress: number | null;
  readonly fits720: boolean;
}

export const layoutMetrics = (g: ExtractedGraph): LayoutMetrics => {
  const c = crossings(g);
  const width = g.viewBox?.w ?? 0;
  const height = g.viewBox?.h ?? 0;
  return {
    nodes: g.nodes.length,
    edges: g.edges.length,
    crossings: c.total,
    maxCrossingsPerEdge: c.maxPerEdge,
    bends: g.edges.reduce((s, e) => s + countBends(e.points), 0),
    edgeLength: g.edges.reduce((s, e) => s + polylineLength(e.points), 0),
    width,
    height,
    area: width * height,
    aspectRatio: height > 0 ? width / height : null,
    labelOverlaps: labelOverlaps(g),
    edgesThroughLabels: edgesThroughLabels(g),
    stress: stress(g),
    fits720: width > 0 && width <= FIT_WIDTH,
  };
};

// ---------------------------------------------------------------------------
// Compatibility.

export interface Compat {
  readonly pass: boolean;
  readonly nodeLabels: boolean;
  readonly edgeCount: boolean;
  readonly edgeLabels: boolean;
  readonly missingNodeLabels: readonly string[];
  readonly extraNodeLabels: readonly string[];
  readonly refEdges: number;
  readonly candEdges: number;
}

/** Multiset difference `a − b` of strings, sorted. */
const multisetMinus = (a: readonly string[], b: readonly string[]): string[] => {
  const counts = new Map<string, number>();
  for (const s of b) counts.set(s, (counts.get(s) ?? 0) + 1);
  const out: string[] = [];
  for (const s of a) {
    const c = counts.get(s) ?? 0;
    if (c > 0) counts.set(s, c - 1);
    else out.push(s);
  }
  return out.sort();
};

/**
 * Same graph (specs/benchmark.md#metrics, Compatibility): equal multisets of
 * node labels and of non-empty edge labels, and the same edge count.
 */
export const compareGraphs = (ref: ExtractedGraph, cand: ExtractedGraph): Compat => {
  // Renderers wrap labels at different points, so whitespace does not count.
  const key = (l: string): string => l.replace(/\s+/g, "");
  const rn = ref.nodes.map((n) => key(n.label));
  const cn = cand.nodes.map((n) => key(n.label));
  const missing = multisetMinus(rn, cn);
  const extra = multisetMinus(cn, rn);
  const rel = ref.edges.map((e) => key(e.label)).filter((l) => l !== "");
  const cel = cand.edges.map((e) => key(e.label)).filter((l) => l !== "");
  const nodeLabels = missing.length === 0 && extra.length === 0;
  const edgeCount = ref.edges.length === cand.edges.length;
  const edgeLabels = multisetMinus(rel, cel).length === 0 && multisetMinus(cel, rel).length === 0;
  return {
    pass: nodeLabels && edgeCount && edgeLabels,
    nodeLabels,
    edgeCount,
    edgeLabels,
    missingNodeLabels: missing,
    extraNodeLabels: extra,
    refEdges: ref.edges.length,
    candEdges: cand.edges.length,
  };
};

// ---------------------------------------------------------------------------
// Stability.

/**
 * Displacement of nodes surviving an edit: nodes are matched by id (by label
 * when either drawing carries no ids), positions are box centres relative to
 * each drawing's viewBox origin, and each distance is divided by the diagonal
 * of the drawing before the edit.
 */
export const displacement = (before: ExtractedGraph, after: ExtractedGraph): { matched: number; values: number[] } => {
  const useIds = before.nodes.every((n) => n.id !== "") && after.nodes.every((n) => n.id !== "");
  const key = (n: ExtractedNode): string => (useIds ? n.id : n.label);
  const origin = (g: ExtractedGraph): Point => ({ x: g.viewBox?.x ?? 0, y: g.viewBox?.y ?? 0 });
  const ob = origin(before);
  const oa = origin(after);
  const diag = before.viewBox === null ? 0 : Math.hypot(before.viewBox.w, before.viewBox.h);
  const afterBy = new Map<string, ExtractedNode>();
  for (const n of after.nodes) if (!afterBy.has(key(n))) afterBy.set(key(n), n);
  const values: number[] = [];
  for (const n of before.nodes) {
    const m = afterBy.get(key(n));
    if (m === undefined || diag === 0) continue;
    const p = boxCenter(n.box);
    const q = boxCenter(m.box);
    values.push(dist({ x: p.x - ob.x, y: p.y - ob.y }, { x: q.x - oa.x, y: q.y - oa.y }) / diag);
  }
  return { matched: values.length, values };
};

// ---------------------------------------------------------------------------
// Statistics.

/** Quantile with linear interpolation between closest ranks; null for no data. */
export const quantile = (xs: readonly number[], q: number): number | null => {
  if (xs.length === 0) return null;
  const s = [...xs].sort((a, b) => a - b);
  const pos = (s.length - 1) * Math.max(0, Math.min(1, q));
  const lo = Math.floor(pos);
  const hi = Math.ceil(pos);
  return s[lo]! + (s[hi]! - s[lo]!) * (pos - lo);
};

export const mean = (xs: readonly number[]): number | null =>
  xs.length === 0 ? null : xs.reduce((a, b) => a + b, 0) / xs.length;
