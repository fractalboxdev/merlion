// Pure view-state math behind <merlion-view> (specs/viewer.md), tested without a DOM.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  MIN_SCALE,
  MAX_SCALE,
  IDENTITY,
  clampScale,
  zoomAt,
  panBy,
  wheelFactor,
  pinchFactor,
  keyView,
  rankLimit,
  labelPx,
  semanticLimit,
  needsControls,
  clampView,
  fitsBox,
  viewBoxSize,
  transformOf,
} from "../zoom.js";

const near = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) <= eps, `${a} ≉ ${b}`);

test("scale bounds are 0.25x to 8x", () => {
  assert.equal(MIN_SCALE, 0.25);
  assert.equal(MAX_SCALE, 8);
  assert.equal(clampScale(0.01), 0.25);
  assert.equal(clampScale(100), 8);
  assert.equal(clampScale(2), 2);
  assert.equal(clampScale(Number.NaN), 1);
  assert.equal(clampScale(Infinity), 8);
});

test("identity view", () => {
  assert.deepEqual(IDENTITY, { s: 1, x: 0, y: 0 });
});

test("zoomAt keeps the point under the pointer fixed", () => {
  const v = { s: 1.5, x: 20, y: -10 };
  const px = 130;
  const py = 70;
  // Content point under the pointer before the zoom.
  const qx = (px - v.x) / v.s;
  const qy = (py - v.y) / v.s;
  const w = zoomAt(v, 2, px, py);
  near(w.s, 3);
  near(w.x + qx * w.s, px);
  near(w.y + qy * w.s, py);
});

test("zoomAt clamps the scale and keeps the anchor fixed at the clamp", () => {
  const v = { s: 6, x: 0, y: 0 };
  const w = zoomAt(v, 4, 100, 100);
  assert.equal(w.s, 8);
  near(w.x + (100 / 6) * 8, 100);
  const z = zoomAt({ s: 0.3, x: 5, y: 5 }, 0.1, 0, 0);
  assert.equal(z.s, 0.25);
});

test("zoomAt ignores non-finite input", () => {
  const v = { s: 2, x: 3, y: 4 };
  assert.deepEqual(zoomAt(v, Number.NaN, 1, 1), v);
  assert.deepEqual(zoomAt(v, 2, Number.NaN, 1), v);
  assert.deepEqual(zoomAt(v, -1, 1, 1), v);
});

test("panBy translates without scaling", () => {
  assert.deepEqual(panBy({ s: 2, x: 1, y: 1 }, 10, -5), { s: 2, x: 11, y: -4 });
  assert.deepEqual(panBy({ s: 2, x: 1, y: 1 }, Number.NaN, 5), { s: 2, x: 1, y: 1 });
});

test("wheelFactor: scrolling up zooms in, down zooms out, symmetric", () => {
  const up = wheelFactor(-100, 0);
  const down = wheelFactor(100, 0);
  assert.ok(up > 1);
  assert.ok(down < 1);
  near(up * down, 1);
  // Line mode (deltaMode 1) counts ~16 px per line.
  near(wheelFactor(-100 / 16, 1), up, 1e-12);
  // A huge delta is bounded to one step.
  near(wheelFactor(-100000, 0), wheelFactor(-400, 0));
  assert.equal(wheelFactor(Number.NaN, 0), 1);
});

test("pinchFactor is the ratio of finger distances", () => {
  near(pinchFactor(100, 150), 1.5);
  assert.equal(pinchFactor(0, 150), 1);
  assert.equal(pinchFactor(100, 0), 1);
});

test("keyView: + - 0 and arrows", () => {
  const v = { s: 1, x: 0, y: 0 };
  const box = { cx: 200, cy: 100, w: 400, h: 200 };
  const zin = keyView("+", v, box);
  assert.ok(zin.s > 1);
  near(zin.x + (200 / 1) * zin.s, 200);
  assert.deepEqual(keyView("=", v, box), zin);
  assert.ok(keyView("-", v, box).s < 1);
  assert.deepEqual(keyView("0", { s: 3, x: 9, y: 9 }, box), IDENTITY);
  assert.deepEqual(keyView("ArrowLeft", v, box), { s: 1, x: 40, y: 0 });
  assert.deepEqual(keyView("ArrowRight", v, box), { s: 1, x: -40, y: 0 });
  assert.deepEqual(keyView("ArrowUp", v, box), { s: 1, x: 0, y: 20 });
  assert.deepEqual(keyView("ArrowDown", v, box), { s: 1, x: 0, y: -20 });
  assert.equal(keyView("a", v, box), null);
  assert.equal(keyView("Enter", v, box), null);
});

test("labelPx is the on-screen label size", () => {
  // 14px font, a 1000-unit viewBox drawn 500px wide, zoomed 1x: 7px.
  near(labelPx(14, 0.5, 1), 7);
  near(labelPx(14, 0.5, 2), 14);
});

test("rankLimit is null at or above 9px and shrinks with the label size", () => {
  assert.equal(rankLimit(9), null);
  assert.equal(rankLimit(14), null);
  assert.equal(rankLimit(8.99), 14);
  assert.equal(rankLimit(4.5), 7);
  assert.equal(rankLimit(0.5), 0);
  assert.equal(rankLimit(0), 0);
  assert.equal(rankLimit(Number.NaN), null);
  for (let px = 0; px < 9; px += 0.25) {
    const r = rankLimit(px);
    assert.ok(Number.isInteger(r) && r >= 0 && r <= 15, `${px} → ${r}`);
  }
  // Monotone: smaller labels never show more ranks.
  let prev = -1;
  for (let px = 0; px < 9; px += 0.1) {
    const r = rankLimit(px);
    assert.ok(r >= prev);
    prev = r;
  }
});

test("needsControls: every diagram has controls unless controls=never", () => {
  assert.equal(needsControls(800, 600, null), true);
  assert.equal(needsControls(400, 600, null), true);
  assert.equal(needsControls(400, 600, "always"), true);
  assert.equal(needsControls(800, 600, "never"), false);
});

test("clampView keeps content that fits inside the box", () => {
  // 400x200 content in a 600x300 box whose left/top edge sits at -100/-50 of the SVG.
  const box = { x: -100, y: -50, w: 600, h: 300 };
  const c = { w: 400, h: 200 };
  assert.deepEqual(clampView({ s: 1, x: 0, y: 0 }, c, box), { s: 1, x: 0, y: 0 });
  // Dragged down-right past the box edge: stops flush with the box.
  assert.deepEqual(clampView({ s: 1, x: 500, y: 400 }, c, box), { s: 1, x: 100, y: 50 });
  assert.deepEqual(clampView({ s: 1, x: -900, y: -900 }, c, box), { s: 1, x: -100, y: -50 });
});

test("clampView keeps zoomed content covering the box", () => {
  const box = { x: 0, y: 0, w: 400, h: 200 };
  const c = { w: 400, h: 200 };
  // At 2x the content is 800x400: x ranges over [-400, 0], y over [-200, 0].
  assert.deepEqual(clampView({ s: 2, x: 50, y: 50 }, c, box), { s: 2, x: 0, y: 0 });
  assert.deepEqual(clampView({ s: 2, x: -999, y: -999 }, c, box), { s: 2, x: -400, y: -200 });
  assert.deepEqual(clampView({ s: 2, x: -100, y: -60 }, c, box), { s: 2, x: -100, y: -60 });
});

test("clampView leaves the view alone without finite sizes", () => {
  const v = { s: 1, x: 3, y: 4 };
  assert.equal(clampView(v, { w: Number.NaN, h: 1 }, { x: 0, y: 0, w: 1, h: 1 }), v);
});

test("fitsBox: panning is pointless while the scaled content fits", () => {
  assert.equal(fitsBox({ s: 1, x: 0, y: 0 }, { w: 400, h: 200 }, { w: 600, h: 300 }), true);
  assert.equal(fitsBox({ s: 2, x: 0, y: 0 }, { w: 400, h: 200 }, { w: 600, h: 300 }), false);
});

test("viewBoxSize parses the viewBox attribute", () => {
  assert.deepEqual(viewBoxSize("0 0 640 320"), { w: 640, h: 320 });
  assert.deepEqual(viewBoxSize(" 0,0, 10.5 2 "), { w: 10.5, h: 2 });
  assert.equal(viewBoxSize(null), null);
  assert.equal(viewBoxSize("0 0 0 10"), null);
  assert.equal(viewBoxSize("0 0 x 10"), null);
  assert.equal(viewBoxSize("0 0 10"), null);
});

test("transformOf renders a CSS transform with origin at the top-left", () => {
  assert.equal(transformOf({ s: 2, x: 10, y: -5.5 }), "translate(10px,-5.5px) scale(2)");
  assert.equal(transformOf(IDENTITY), "");
});

test("semanticLimit never declutters the fit view or a zoomed-in view", () => {
  // A wide diagram fitted at 0.3x draws 4.2 px labels, but the fit view keeps everything.
  assert.equal(semanticLimit(14, 0.3, 1), null);
  assert.equal(semanticLimit(14, 0.3, 2), null);
  // Zooming out past the fit view engages the rank limit.
  assert.equal(semanticLimit(14, 0.3, 0.5), rankLimit(labelPx(14, 0.3, 0.5)));
  // Legible labels never engage it, even below the fit view.
  assert.equal(semanticLimit(14, 1, 0.9), null);
});
