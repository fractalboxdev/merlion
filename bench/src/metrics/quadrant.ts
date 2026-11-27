/**
 * Quadrant chart compatibility (specs/quadrant.md#compatibility): the same title,
 * quadrant labels, axis labels and points as the mermaid drawing, each placed the same
 * way relative to the plot.
 */
import { hexColour, type QuadrantDrawing } from "../svg/quadrant.ts";

/** Largest difference in a recovered 0–1 coordinate that still matches. */
const COORDINATE_TOLERANCE = 0.002;

export interface QuadrantCompat {
  readonly pass: boolean;
  readonly differences: readonly string[];
}

/**
 * Visible text compared without whitespace, since the renderers wrap at different
 * points. The one-sided axis marker is `⟶` in mermaid and `→` in Merlion, whose font
 * table holds `→` but not `⟶`.
 */
const visible = (s: string | null): string => (s ?? "").replace(/⟶/g, "→").replace(/\s+/g, "");

export const quadrantCompat = (reference: QuadrantDrawing, candidate: QuadrantDrawing, source: string): QuadrantCompat => {
  const d: string[] = [];
  if (!candidate.validSvg) d.push("candidate has no valid SVG viewport");
  if (visible(reference.title) !== visible(candidate.title)) d.push(`title ${JSON.stringify(candidate.title)}, expected ${JSON.stringify(reference.title)}`);
  if (candidate.quadrants.length !== 4) d.push(`${candidate.quadrants.length} quadrant boxes, expected 4`);
  for (const r of reference.quadrants) {
    const c = candidate.quadrants.find((q) => q.n === r.n);
    if (c === undefined) continue;
    if (visible(r.label) !== visible(c.label)) d.push(`quadrant ${r.n} label differs`);
    if (r.label.trim() !== "" && r.placement !== c.placement) d.push(`quadrant ${r.n} label at ${c.placement}, expected ${r.placement}`);
    // Fills are compared where the source sets them; the default quadrant colours are
    // each renderer's theme.
    if (new RegExp(`quadrant${r.n}Fill`).test(source) && hexColour(r.fill) !== hexColour(c.fill)) d.push(`quadrant ${r.n} fill ${c.fill}, expected ${r.fill}`);
  }
  const key = (a: { axis: string; end: string }) => `${a.axis}-${a.end}`;
  const refAxes = new Map(reference.axes.map((a) => [key(a), a]));
  const candAxes = new Map(candidate.axes.map((a) => [key(a), a]));
  for (const k of new Set([...refAxes.keys(), ...candAxes.keys()])) {
    const r = refAxes.get(k);
    const c = candAxes.get(k);
    if (r === undefined || c === undefined) { d.push(`${k} label ${r === undefined ? "unexpected" : "missing"}`); continue; }
    if (visible(r.text) !== visible(c.text)) d.push(`${k} label differs`);
    if (r.side !== c.side) d.push(`${k} label on the ${c.side}, expected ${r.side}`);
    if (r.align !== c.align) d.push(`${k} label aligned ${c.align}, expected ${r.align}`);
  }
  if (reference.points.length !== candidate.points.length) d.push(`point count ${candidate.points.length}, expected ${reference.points.length}`);
  const pool = [...candidate.points];
  for (const r of reference.points) {
    const i = pool.findIndex((c) => visible(c.label) === visible(r.label) && Math.abs(c.x - r.x) <= COORDINATE_TOLERANCE && Math.abs(c.y - r.y) <= COORDINATE_TOLERANCE);
    if (i < 0) { d.push(`point ${JSON.stringify(r.label)} at (${r.x.toFixed(3)}, ${r.y.toFixed(3)}) missing`); continue; }
    const c = pool.splice(i, 1)[0]!;
    if (Math.abs(c.r - r.r) > 0.01) d.push(`point ${JSON.stringify(r.label)} radius ${c.r}, expected ${r.r}`);
    // An explicit colour is a hex in mermaid's drawing; its default point colour is not.
    const refFill = hexColour(r.fill);
    if (refFill !== null && refFill !== hexColour(c.fill)) d.push(`point ${JSON.stringify(r.label)} fill ${c.fill}, expected ${r.fill}`);
    if (Math.abs(c.strokeWidth - r.strokeWidth) > 0.01) d.push(`point ${JSON.stringify(r.label)} stroke width ${c.strokeWidth}, expected ${r.strokeWidth}`);
    const refStroke = hexColour(r.stroke);
    if (r.strokeWidth > 0 && refStroke !== null && refStroke !== hexColour(c.stroke)) d.push(`point ${JSON.stringify(r.label)} stroke ${c.stroke}, expected ${r.stroke}`);
    if (r.label.trim() !== "" && r.labelBelow !== c.labelBelow) d.push(`point ${JSON.stringify(r.label)} label ${c.labelBelow ? "below" : "not below"} its point`);
  }
  return { pass: d.length === 0, differences: d };
};

export const quadrantOutcome = (referenceError: string | null, candidateError: string | null, compat: QuadrantCompat | null) => {
  if (referenceError !== null) return candidateError !== null ? "both rejected" : "reference rejected";
  if (candidateError !== null) return "candidate rejected";
  return compat?.pass === true ? "matched" : "different";
};

/** A nonzero compatibility gate always measures the complete retained corpus. */
export const quadrantGate = (total: number, measured: number, passed: number, minimum: number): string | null => {
  if (!Number.isFinite(minimum) || minimum < 0 || minimum > 100) return "min-compat must be between 0 and 100";
  if (minimum === 0) return null;
  if (measured !== total || total === 0) return "min-compat requires the complete nonempty corpus; omit --limit";
  const percent = (100 * passed) / total;
  return percent < minimum ? `quadrant compatibility ${percent.toFixed(3)}% (${passed}/${total}) is below ${minimum}%` : null;
};
