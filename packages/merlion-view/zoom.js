// View-state math for <merlion-view> (specs/viewer.md). Pure functions, no DOM.
//
// A view is { s, x, y }: the SVG is drawn with `transform-origin: 0 0` and
// `transform: translate(x, y) scale(s)`, so a content point q appears at
// x + s·q on screen, relative to the SVG's untransformed top-left corner.

/** Zoom bounds (specs/viewer.md#behaviour). */
export const MIN_SCALE = 0.25;
export const MAX_SCALE = 8;

/** Labels smaller than this on screen trigger semantic zoom (specs/viewer.md#semantic-zoom). */
export const MIN_LABEL_PX = 9;

/** Highest `data-merlion-rank` the SVG emits (specs/svg-output.md#ids-and-data-attributes). */
export const MAX_RANK = 15;

/** Zoom factor of one `+` / `-` key press or button click. */
export const STEP = 1.25;

export const IDENTITY = Object.freeze({ s: 1, x: 0, y: 0 });

const finite = Number.isFinite;

/** Clamp a scale into [MIN_SCALE, MAX_SCALE]; NaN maps to 1. */
export const clampScale = (s) =>
  s !== s ? 1 : Math.min(MAX_SCALE, Math.max(MIN_SCALE, s));

/**
 * Multiply the scale by `factor`, keeping the content point under (px, py) fixed.
 * The anchor holds even when the scale clamps, because the translation is
 * derived from the scale actually applied.
 */
export const zoomAt = (v, factor, px, py) => {
  if (!finite(factor) || factor <= 0 || !finite(px) || !finite(py)) return v;
  const s = clampScale(v.s * factor);
  const k = s / v.s;
  return { s, x: px - (px - v.x) * k, y: py - (py - v.y) * k };
};

export const panBy = (v, dx, dy) =>
  finite(dx) && finite(dy) ? { s: v.s, x: v.x + dx, y: v.y + dy } : v;

/**
 * Zoom factor for one wheel event. `deltaMode` 1 counts lines (~16 px) and 2
 * pages (~800 px). Each event is bounded to 100 px so a fast mouse wheel or a
 * page-mode delta moves at most one step of ~1.41×.
 */
export const wheelFactor = (deltaY, deltaMode) => {
  if (!finite(deltaY)) return 1;
  const px = deltaY * (deltaMode === 1 ? 16 : deltaMode === 2 ? 800 : 1);
  return 2 ** (-Math.max(-100, Math.min(100, px)) / 200);
};

/** Zoom factor of a two-finger pinch: current distance over previous distance. */
export const pinchFactor = (d0, d1) => (d0 > 0 && d1 > 0 && finite(d0) && finite(d1) ? d1 / d0 : 1);

/**
 * Keyboard (specs/viewer.md#behaviour): `+`/`=` zoom in and `-`/`_` zoom out
 * around the centre (cx, cy), `0` resets, arrows pan by 10% of the box (w, h).
 * An arrow moves the view in its direction, so the content moves the other way.
 * Returns null for any other key so the caller leaves the event alone.
 */
export const keyView = (key, v, { cx, cy, w, h }) => {
  switch (key) {
    case "+":
    case "=":
      return zoomAt(v, STEP, cx, cy);
    case "-":
    case "_":
      return zoomAt(v, 1 / STEP, cx, cy);
    case "0":
      return IDENTITY;
    case "ArrowLeft":
      return panBy(v, w * 0.1, 0);
    case "ArrowRight":
      return panBy(v, -w * 0.1, 0);
    case "ArrowUp":
      return panBy(v, 0, h * 0.1);
    case "ArrowDown":
      return panBy(v, 0, -h * 0.1);
    default:
      return null;
  }
};

/**
 * On-screen label size: font size in SVG units × CSS px per SVG unit at fit
 * (`fit`, from the rendered box over the viewBox) × zoom scale.
 */
export const labelPx = (fontSize, fit, s) => fontSize * fit * s;

/**
 * Semantic-zoom rank limit for a label drawn `px` pixels tall, or null when
 * labels are legible (≥ 9 px). The limit falls linearly with the label size:
 * just under 9 px hides only rank 15; below 9/16 px only rank 0 stays labelled.
 */
export const rankLimit = (px) => {
  if (!(px < MIN_LABEL_PX)) return null;
  const r = Math.floor((Math.max(0, px) / MIN_LABEL_PX) * (MAX_RANK + 1)) - 1;
  return Math.min(MAX_RANK, Math.max(0, r));
};

/**
 * Rank limit for the current view: semantic zoom declutters only once the user zooms
 * out past the fit view (s < 1). The fit view and every zoom-in keep every label, even
 * when a wide diagram fitted to its container draws them below 9 px.
 */
export const semanticLimit = (fontSize, fit, s) => (s < 1 ? rankLimit(labelPx(fontSize, fit, s)) : null);

/** Every diagram gets controls (shown faintly until hover or focus) unless controls="never". */
export const needsControls = (_natural, _container, attr) => attr !== "never";

// One axis: content of length `len` (already scaled) starting at `pos`, inside a box
// [lo, lo + size]. Content that fits stays inside the box; larger content covers it.
const clampAxis = (pos, len, lo, size) =>
  len <= size ? Math.min(lo + size - len, Math.max(lo, pos)) : Math.min(lo, Math.max(lo + size - len, pos));

/**
 * Clamp a view so panning never pushes the drawing out of sight. `content` is the
 * SVG's untransformed size; `box` is the visible box relative to the SVG's
 * untransformed top-left corner.
 */
export const clampView = (v, content, box) => {
  if (![content.w, content.h, box.x, box.y, box.w, box.h].every(finite)) return v;
  return {
    s: v.s,
    x: clampAxis(v.x, v.s * content.w, box.x, box.w),
    y: clampAxis(v.y, v.s * content.h, box.y, box.h),
  };
};

/** True while the scaled content fits the box on both axes, so a drag has nothing to reveal. */
export const fitsBox = (v, content, box) => v.s * content.w <= box.w + 0.5 && v.s * content.h <= box.h + 0.5;

/** Width and height of a `viewBox` attribute, or null when it is absent or invalid. */
export const viewBoxSize = (attr) => {
  if (typeof attr !== "string") return null;
  const n = attr.trim().split(/[\s,]+/).map(Number);
  if (n.length !== 4 || !n.every(finite) || n[2] <= 0 || n[3] <= 0) return null;
  return { w: n[2], h: n[3] };
};

/** CSS transform for a view; the identity view clears the transform. */
export const transformOf = ({ s, x, y }) =>
  s === 1 && x === 0 && y === 0 ? "" : `translate(${x}px,${y}px) scale(${s})`;

/**
 * Whether a primary pointer-down starts a pan (specs/viewer.md#gestures). `hit` is what
 * the pointer went down on: "text" (a label), "shape" (a node or edge) or "bg" (anything
 * else). A mouse or pen pans only from the background, so a drag on text selects it; one
 * finger pans from anywhere, but only once zoomed in, so the page still scrolls at fit.
 * Nothing pans while the whole drawing fits the box.
 */
export const panIntent = (hit, pointerType, fits, zoomed) =>
  !fits && (pointerType === "touch" ? zoomed : hit === "bg");

/**
 * Whether a click is a tap, the only kind of click that commands the diagram: the pointer moved
 * less than 4 px since it went down, the click leaves no text selected, and it is not the second
 * click of a double-click (`detail` > 1), which belongs to word selection.
 */
export const isTap = (dx, dy, detail, selected) => Math.hypot(dx, dy) < 4 && detail < 2 && !selected;

/** Double-click resets the zoom only on the background and never while text is selected. */
export const resetsOnDoubleClick = (hit, selected) => hit === "bg" && !selected;

const clamp = (v, lo, hi) => Math.max(lo, Math.min(v, hi));

/**
 * Popover position against an element's rectangle `E` inside the visible box `C` (both
 * DOMRect-shaped: x, y, width, height) for a popover of size w × h, 8 px away
 * (specs/interaction.md#placement). Tries below, above, right and left; otherwise the roomiest
 * side, with the popover's height (`h`) or width (`w`) capped to it; null when no side has
 * 48 px. The popover never overlaps `E`.
 */
export const place = (E, C, w, h, gap = 8) => {
  const room = [
    C.y + C.height - E.y - E.height - gap,
    E.y - gap - C.y,
    C.x + C.width - E.x - E.width - gap,
    E.x - gap - C.x,
  ];
  let i = room.findIndex((r, k) => r >= (k < 2 ? h : w));
  let s = i < 2 ? h : w;
  if (i < 0) {
    s = Math.max(...room);
    if (s < 48) return null;
    i = room.indexOf(s);
  }
  const x = clamp(E.x + E.width / 2 - w / 2, C.x, C.x + C.width - w);
  const y = clamp(E.y + E.height / 2 - h / 2, C.y, C.y + C.height - h);
  return [
    { side: "below", x, y: E.y + E.height + gap, h: s },
    { side: "above", x, y: E.y - gap - s, h: s },
    { side: "right", x: E.x + E.width + gap, y, w: s },
    { side: "left", x: E.x - gap - s, y, w: s },
  ][i];
};
