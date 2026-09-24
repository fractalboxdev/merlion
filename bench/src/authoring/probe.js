// Browser side of the authoring legibility measure (specs/benchmark.md#authoring).
// Injected into the page as a classic script, plain JavaScript so nothing a TypeScript
// transform adds reaches the page. `window.legibilityProbe(svg)` inlines the SVG, lays it
// out and returns the three failures a reader sees: a label wider or taller than the shape
// it sits in, two node shapes overlapping, and ink outside the viewBox.
//
// Text advance is the reason this runs in a browser at all: it depends on the font the
// browser resolves, and nothing in the SVG source records it.
(() => {
  /** A label counts as overflowing when it exceeds its shape by more than this, in px. */
  const OVERFLOW_TOLERANCE = 1;
  /** Two shapes count as overlapping above this intersection area, in px². */
  const OVERLAP_MIN_AREA = 1;
  /** A shape covering at least this share of the viewBox is a background plate, not a node. */
  const BACKGROUND_AREA_RATIO = 0.9;

  const EMPTY = { labels: 0, overflowing: 0, worstOverflow: 0, clipped: 0, shapeOverlaps: 0, unplaced: 0 };

  const inDefs = (el) => el.closest("defs, marker, clipPath, mask, symbol, pattern") !== null;

  const contains = (outer, inner, tol) =>
    inner.x >= outer.x - tol &&
    inner.y >= outer.y - tol &&
    inner.x + inner.w <= outer.x + outer.w + tol &&
    inner.y + inner.h <= outer.y + outer.h + tol;

  const holds = (box, p) => p.x >= box.x && p.x <= box.x + box.w && p.y >= box.y && p.y <= box.y + box.h;

  const intersection = (a, b) =>
    Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x)) *
    Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));

  const overflowOf = (owner, box) =>
    Math.max(
      owner.x - box.x,
      box.x + box.w - (owner.x + owner.w),
      owner.y - box.y,
      box.y + box.h - (owner.y + owner.h),
    );

  window.legibilityProbe = (svg) => {
    const host = document.createElement("div");
    // Off-screen but laid out: `display: none` would make every bounding box zero.
    host.setAttribute("style", "position:absolute;left:-10000px;top:0;width:2000px");
    host.innerHTML = svg;
    document.body.appendChild(host);
    try {
      const root = host.querySelector("svg");
      if (root === null) return EMPTY;

      // Every box is taken in the root's own user space, so a transformed group
      // compares with an untransformed one.
      const rootMatrix = root.getCTM();
      const boxIn = (el) => {
        let b;
        try {
          b = el.getBBox();
        } catch {
          return null;
        }
        if (!(b.width > 0 || b.height > 0)) return null;
        const m = el.getCTM();
        if (m === null || rootMatrix === null) return { x: b.x, y: b.y, w: b.width, h: b.height };
        const rel = rootMatrix.inverse().multiply(m);
        const corners = [
          { x: b.x, y: b.y },
          { x: b.x + b.width, y: b.y },
          { x: b.x + b.width, y: b.y + b.height },
          { x: b.x, y: b.y + b.height },
        ].map((p) => ({ x: rel.a * p.x + rel.c * p.y + rel.e, y: rel.b * p.x + rel.d * p.y + rel.f }));
        const xs = corners.map((p) => p.x);
        const ys = corners.map((p) => p.y);
        const x = Math.min.apply(null, xs);
        const y = Math.min.apply(null, ys);
        return { x, y, w: Math.max.apply(null, xs) - x, h: Math.max.apply(null, ys) - y };
      };

      const vb = root.viewBox.baseVal;
      const view = vb && vb.width > 0 ? { x: vb.x, y: vb.y, w: vb.width, h: vb.height } : null;

      // Painted closed shapes, background plate excluded. `path` belongs here:
      // Merlion draws every node as a closed filled path, so a selector of
      // primitives alone would measure a hand-written SVG and nothing else.
      const shapes = [];
      for (const el of Array.from(root.querySelectorAll("rect, circle, ellipse, polygon, path"))) {
        if (inDefs(el)) continue;
        // An open path is a stroke — an edge, not a box.
        if (el.tagName === "path" && !/[zZ]/.test(el.getAttribute("d") || "")) continue;
        const box = boxIn(el);
        if (box === null) continue;
        if (view !== null && box.w * box.h >= view.w * view.h * BACKGROUND_AREA_RATIO) continue;
        const style = getComputedStyle(el);
        if (style.fill === "none" && style.stroke === "none") continue;
        shapes.push(box);
      }

      const texts = [];
      for (const el of Array.from(root.querySelectorAll("text"))) {
        if (inDefs(el)) continue;
        if ((el.textContent || "").trim() === "") continue;
        const box = boxIn(el);
        if (box !== null) texts.push(box);
      }

      let overflowing = 0;
      let worstOverflow = 0;
      let clipped = 0;
      let unplaced = 0;
      for (const box of texts) {
        if (view !== null && !contains(view, box, OVERFLOW_TOLERANCE)) clipped++;
        const centre = { x: box.x + box.w / 2, y: box.y + box.h / 2 };
        // The label's shape: the smallest painted shape whose box holds the text's centre.
        let owner = null;
        for (const s of shapes) {
          if (!holds(s, centre)) continue;
          if (owner === null || s.w * s.h < owner.w * owner.h) owner = s;
        }
        if (owner === null) {
          // No shape holds this text. A label that has slid off the box it
          // names still touches it, and that is the worst overflow there is;
          // text touching nothing is a title or a chipless edge label, which
          // is not a defect and is counted apart rather than scored as one.
          let touched = null;
          for (const s of shapes) {
            if (intersection(s, box) <= OVERLAP_MIN_AREA) continue;
            if (touched === null || s.w * s.h < touched.w * touched.h) touched = s;
          }
          if (touched === null) {
            unplaced++;
            continue;
          }
          owner = touched;
        }
        if (contains(owner, box, OVERFLOW_TOLERANCE)) continue;
        overflowing++;
        const over = overflowOf(owner, box);
        if (owner.w > 0 && over / owner.w > worstOverflow) worstOverflow = over / owner.w;
      }
      for (const s of shapes) {
        if (view !== null && !contains(view, s, OVERFLOW_TOLERANCE)) clipped++;
      }

      // Every painted shape but the background counts, because two node boxes
      // overlapping is a defect whether or not their labels stayed inside them.
      let shapeOverlaps = 0;
      for (let i = 0; i < shapes.length; i++) {
        for (let j = i + 1; j < shapes.length; j++) {
          if (intersection(shapes[i], shapes[j]) > OVERLAP_MIN_AREA) shapeOverlaps++;
        }
      }

      return { labels: texts.length, overflowing, worstOverflow, clipped, shapeOverlaps, unplaced };
    } finally {
      host.remove();
    }
  };
})();
