/**
 * Pure comparison logic of the stylesheet parity gate (specs/benchmark.md): per-element
 * computed paint between the reference and a target, and pixels between the reference
 * screenshot and the rsvg-convert raster.
 */
import type { Raster } from "./png.ts";

/** An 8-bit sRGB colour with alpha, as a canvas `fillStyle` round trip yields it. */
export type Rgba = readonly [number, number, number, number];

export interface ElementRecord {
  /** Position among the diagram's compared elements, in document order. */
  readonly index: number;
  readonly tag: string;
  /** node, edge, cluster, marker, label or other. */
  readonly kind: string;
  /** Human-readable locator: kind, owning data-merlion-id and class. */
  readonly label: string;
  readonly props: {
    /** `null` is `none` (no paint). */
    readonly fill: Rgba | null;
    readonly stroke: Rgba | null;
    /** Normalised with {@link normaliseDash}. */
    readonly dash: string;
    /** Product of the element's and every ancestor's `opacity` with its paint opacities. */
    readonly opacity: number;
  };
}

export interface Mismatch {
  readonly label: string;
  readonly property: string;
  readonly reference: string;
  readonly target: string;
}

export interface Comparison {
  readonly compared: number;
  readonly structure: boolean;
  readonly mismatches: readonly Mismatch[];
}

export const sameColour = (a: Rgba, b: Rgba, tolerance: number): boolean => {
  if (a[3] === 0 && b[3] === 0) return true;
  return a.every((x, i) => Math.abs(x - (b[i] ?? 0)) <= tolerance);
};

/** Source-over compositing of `top` onto an opaque `bottom`, rounded to 8 bits. */
export const over = (top: Rgba, bottom: Rgba): Rgba => {
  const a = top[3] / 255;
  const ch = (i: 0 | 1 | 2) => Math.round(top[i] * a + bottom[i] * (1 - a));
  return [ch(0), ch(1), ch(2), 255];
};

export const normaliseDash = (value: string): string => {
  const parts = value
    .trim()
    .split(/[\s,]+/)
    .filter((p) => p.length > 0)
    .map((p) => Number.parseFloat(p));
  if (parts.length === 0 || parts.some((n) => Number.isNaN(n)) || parts.every((n) => n === 0)) return "none";
  return parts.map((n) => String(Math.round(n * 1000) / 1000)).join(",");
};

/** Base theme (`null`) and each named theme of compiled page CSS, in order of first use. */
export const themesOf = (compiledCss: string): (string | null)[] => {
  const out: (string | null)[] = [null];
  for (const m of compiledCss.matchAll(/\[data-theme="([a-z][a-z0-9-]*)"\]/g)) {
    const name = m[1] ?? "";
    if (!out.includes(name)) out.push(name);
  }
  return out;
};

const show = (c: Rgba | null): string => (c === null ? "none" : `rgba(${c.join(",")})`);

const paintEqual = (a: Rgba | null, b: Rgba | null, tolerance: number): boolean =>
  a === null || b === null ? a === b : sameColour(a, b, tolerance);

/** Compares fill, stroke, dash and opacity per element; lists must align by index. */
export const compareElements = (
  reference: readonly ElementRecord[],
  target: readonly ElementRecord[],
  tolerance: number,
): Comparison => {
  const shape = (xs: readonly ElementRecord[]) => xs.map((x) => `${x.tag}:${x.kind}`).join(" ");
  if (reference.length !== target.length || shape(reference) !== shape(target)) {
    return {
      compared: 0,
      structure: false,
      mismatches: [
        { label: "diagram", property: "structure", reference: `${reference.length} elements`, target: `${target.length} elements` },
      ],
    };
  }
  const mismatches: Mismatch[] = [];
  let compared = 0;
  reference.forEach((r, i) => {
    const t = target[i] as ElementRecord;
    const check = (property: string, ok: boolean, a: string, b: string) => {
      compared++;
      if (!ok) mismatches.push({ label: r.label, property, reference: a, target: b });
    };
    check("fill", paintEqual(r.props.fill, t.props.fill, tolerance), show(r.props.fill), show(t.props.fill));
    check("stroke", paintEqual(r.props.stroke, t.props.stroke, tolerance), show(r.props.stroke), show(t.props.stroke));
    check("dash", r.props.dash === t.props.dash, r.props.dash, t.props.dash);
    check("opacity", Math.abs(r.props.opacity - t.props.opacity) <= 0.005, String(r.props.opacity), String(t.props.opacity));
  });
  return { compared, structure: true, mismatches };
};

const pixel = (img: Raster, x: number, y: number): Rgba | null => {
  if (x < 0 || y < 0 || x >= img.width || y >= img.height) return null;
  const o = (y * img.width + x) * 4;
  return [img.rgba[o] ?? 0, img.rgba[o + 1] ?? 0, img.rgba[o + 2] ?? 0, img.rgba[o + 3] ?? 0];
};

/** True when every pixel within `radius` of (x, y) equals the centre exactly. */
export const uniformAt = (img: Raster, x: number, y: number, radius: number): boolean => {
  const c = pixel(img, x, y);
  if (c === null) return false;
  for (let dy = -radius; dy <= radius; dy++) {
    for (let dx = -radius; dx <= radius; dx++) {
      const p = pixel(img, x + dx, y + dy);
      if (p === null || !sameColour(p, c, 0)) return false;
    }
  }
  return true;
};

export interface PixelSample {
  readonly label: string;
  /** Device pixel coordinates in both rasters. */
  readonly x: number;
  readonly y: number;
}

const WHITE: Rgba = [255, 255, 255, 255];

/** Compares the reference raster with another at each sample, both composited over white. */
export const comparePixels = (
  reference: Raster,
  other: Raster,
  samples: readonly PixelSample[],
  tolerance: number,
): Comparison => {
  const mismatches: Mismatch[] = [];
  for (const s of samples) {
    const a = pixel(reference, s.x, s.y);
    const b = pixel(other, s.x, s.y);
    const ra = a === null ? null : over(a, WHITE);
    const rb = b === null ? null : over(b, WHITE);
    if (ra === null || rb === null || !sameColour(ra, rb, tolerance)) {
      mismatches.push({ label: s.label, property: "pixel", reference: show(ra), target: show(rb) });
    }
  }
  return { compared: samples.length, structure: true, mismatches };
};

/**
 * A text element's box in device pixels and the colour its glyphs must show. Glyph
 * outlines differ between Chromium's and librsvg's font stacks, so text is compared by
 * ink presence, not by position: somewhere in the box, a pixel must show the colour.
 */
export interface InkSample {
  readonly label: string;
  readonly expected: Rgba;
  readonly x0: number;
  readonly y0: number;
  readonly x1: number;
  readonly y1: number;
}

/** The pixel in the box closest to `colour` (channel-wise maximum difference, over white). */
const nearestInk = (img: Raster, s: InkSample, colour: Rgba): { distance: number; pixel: Rgba | null } => {
  let best: { distance: number; pixel: Rgba | null } = { distance: Number.POSITIVE_INFINITY, pixel: null };
  for (let y = Math.max(0, Math.floor(s.y0)); y <= Math.min(img.height - 1, Math.ceil(s.y1)); y++) {
    for (let x = Math.max(0, Math.floor(s.x0)); x <= Math.min(img.width - 1, Math.ceil(s.x1)); x++) {
      const p = pixel(img, x, y);
      if (p === null) continue;
      const c = over(p, WHITE);
      const d = Math.max(Math.abs(c[0] - colour[0]), Math.abs(c[1] - colour[1]), Math.abs(c[2] - colour[2]));
      if (d < best.distance) best = { distance: d, pixel: c };
      if (d === 0) return best;
    }
  }
  return best;
};

/**
 * Compares text colour between the reference raster and another: a box counts only when
 * the reference shows the expected colour in it (fully covered glyph pixels exist), and
 * then the other raster must show it too, within `tolerance` per channel.
 */
export const compareInk = (
  reference: Raster,
  other: Raster,
  samples: readonly InkSample[],
  tolerance: number,
): Comparison & { readonly rejected: number } => {
  const mismatches: Mismatch[] = [];
  let compared = 0;
  let rejected = 0;
  for (const s of samples) {
    if (nearestInk(reference, s, s.expected).distance > tolerance) {
      rejected++;
      continue;
    }
    compared++;
    const got = nearestInk(other, s, s.expected);
    if (got.distance > tolerance) {
      mismatches.push({ label: s.label, property: "text ink", reference: show(s.expected), target: `nearest ${show(got.pixel)}` });
    }
  }
  return { compared, structure: true, mismatches, rejected };
};
