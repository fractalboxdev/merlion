// Browser side of the stylesheet parity gate (specs/benchmark.md). Injected into each page
// as a classic script; plain JavaScript so nothing a TypeScript transform adds reaches the
// page. `window.parityProbe(withSamples)` returns, for every `.parity-d` container, the
// computed paint of each compared element and, on the reference page, sample points inside
// the shapes for the pixel comparison.
(() => {
  const canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });

  // 8-bit sRGB through a canvas fillStyle round trip; null is `none`.
  const rgba = (value, paintOpacity) => {
    if (!value || value === "none" || value.startsWith("url(")) return null;
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillStyle = "#000";
    ctx.fillStyle = value;
    ctx.globalAlpha = 1;
    ctx.fillRect(0, 0, 1, 1);
    const d = ctx.getImageData(0, 0, 1, 1).data;
    return [d[0], d[1], d[2], Math.round(d[3] * paintOpacity)];
  };

  const GEOMETRY = "path,rect,circle,ellipse,line,polyline,polygon,text,tspan";
  const EXCLUDED = "title,desc,style,clipPath,mask,pattern,linearGradient,radialGradient";

  const kindOf = (el) => {
    if (el.closest("marker")) return "marker";
    if (el.closest(".merlion-edge-label")) return "label";
    if (el.closest(".merlion-edge")) return "edge";
    if (el.closest(".merlion-node")) return "node";
    if (el.closest(".merlion-cluster")) return "cluster";
    return "other";
  };

  const ownerOf = (el, svg) => {
    const marker = el.closest("marker");
    if (marker) return marker.id.slice(svg.id.length);
    const edge = el.closest(".merlion-edge");
    if (edge) return `${edge.dataset.merlionFrom}->${edge.dataset.merlionTo}`;
    const owner = el.closest("[data-merlion-id]");
    return owner ? owner.dataset.merlionId : "";
  };

  const opacityChain = (el, svg) => {
    let o = 1;
    for (let n = el; n && n !== svg.parentElement; n = n.parentElement) {
      o *= Number.parseFloat(getComputedStyle(n).opacity);
    }
    return o;
  };

  const compared = (svg) =>
    [...svg.querySelectorAll(GEOMETRY)].filter(
      (el) => !el.closest(EXCLUDED) && (!el.closest("defs") || el.closest("marker")),
    );

  // Point in the svg's user space to a point in page (client) CSS pixels.
  const toClient = (el, x, y) => {
    const m = el.getScreenCTM();
    return new DOMPoint(x, y).matrixTransform(m);
  };

  const textBoxes = (svg) =>
    [...svg.querySelectorAll("text")].map((t) => {
      const r = t.getBoundingClientRect();
      const padX = r.width * 0.2 + 3;
      return { x0: r.left - padX, x1: r.right + padX, y0: r.top - 3, y1: r.bottom + 3 };
    });

  const inText = (boxes, p) => boxes.some((b) => p.x >= b.x0 && p.x <= b.x1 && p.y >= b.y0 && p.y <= b.y1);

  const RING = [
    [0, 0],
    [2, 0],
    [-2, 0],
    [0, 2],
    [0, -2],
    [1.5, 1.5],
    [-1.5, 1.5],
    [1.5, -1.5],
    [-1.5, -1.5],
  ];

  const spread = (xs, n) => {
    if (xs.length <= n) return xs;
    const out = [];
    for (let i = 0; i < n; i++) out.push(xs[Math.floor(((i + 0.5) * xs.length) / n)]);
    return out;
  };

  const fillSamples = (el, boxes) => {
    const b = el.getBBox();
    const out = [];
    const N = 9;
    for (let i = 1; i < N; i++) {
      for (let j = 1; j < N; j++) {
        const x = b.x + (b.width * i) / N;
        const y = b.y + (b.height * j) / N;
        const ok = RING.every(([dx, dy]) => {
          const p = new DOMPoint(x + dx, y + dy);
          return el.isPointInFill(p) && !el.isPointInStroke(p);
        });
        if (!ok) continue;
        const c = toClient(el, x, y);
        if (inText(boxes, c) || document.elementFromPoint(c.x, c.y) !== el) continue;
        out.push(c);
      }
    }
    return spread(out, 3);
  };

  // True when arc position `at` lies inside a dash, at least `margin` from either end of it,
  // so a small difference in how two rasterisers measure arc length cannot move a dash
  // boundary onto the sample.
  const insideDash = (el, at, margin) => {
    const cs = getComputedStyle(el);
    if (cs.strokeDasharray === "none") return true;
    let dashes = cs.strokeDasharray.split(/[\s,]+/).map((x) => Number.parseFloat(x));
    if (dashes.some((x) => !(x >= 0))) return false;
    if (dashes.length % 2 === 1) dashes = dashes.concat(dashes);
    const period = dashes.reduce((a, b) => a + b, 0);
    if (!(period > 0)) return true;
    let pos = (((at + Number.parseFloat(cs.strokeDashoffset || "0")) % period) + period) % period;
    for (let i = 0; i < dashes.length; i++) {
      const len = dashes[i];
      if (pos < len) return i % 2 === 0 && pos >= margin && len - pos >= margin;
      pos -= len;
    }
    return false;
  };

  const strokeSamples = (el, boxes, avoidEnds) => {
    let len;
    try {
      len = el.getTotalLength();
    } catch {
      return [];
    }
    if (!(len > 0)) return [];
    const out = [];
    for (let k = 1; k < 20; k++) {
      const at = (len * k) / 20;
      if (avoidEnds && (at < 12 || len - at < 12)) continue;
      if (!insideDash(el, at, 1)) continue;
      const p = el.getPointAtLength(at);
      const c = toClient(el, p.x, p.y);
      if (inText(boxes, c) || document.elementFromPoint(c.x, c.y) !== el) continue;
      out.push(c);
    }
    return spread(out, 3);
  };

  const MARKER_GRID = 21;

  // A point inside the marker's own shape, in marker content coordinates.
  const markerInterior = (shape, paint) => {
    const b = shape.getBBox();
    const cx = b.x + b.width / 3;
    const cy = b.y + b.height / 2;
    let best = null;
    for (let i = 0; i <= MARKER_GRID; i++) {
      for (let j = 0; j <= MARKER_GRID; j++) {
        const x = b.x + (b.width * i) / MARKER_GRID;
        const y = b.y + (b.height * j) / MARKER_GRID;
        const inside = (dx, dy) => {
          const p = new DOMPoint(x + dx, y + dy);
          return paint === "fill" ? shape.isPointInFill(p) : shape.isPointInStroke(p);
        };
        const r = paint === "fill" ? 0.8 : 0.2;
        if (![[0, 0], [r, 0], [-r, 0], [0, r], [0, -r]].every(([dx, dy]) => inside(dx, dy))) continue;
        const d = (x - cx) ** 2 + (y - cy) ** 2;
        if (best === null || d < best.d) best = { x, y, d };
      }
    }
    return best;
  };

  const markerSamples = (path, svg, index, boxes) => {
    const out = [];
    const cs = getComputedStyle(path);
    const len = path.getTotalLength();
    if (!(len > 1)) return out;
    for (const [prop, atEnd] of [
      ["markerEnd", true],
      ["markerStart", false],
    ]) {
      const m = /url\("?#([^")]+)"?\)/.exec(cs[prop]);
      if (!m) continue;
      const marker = svg.querySelector(`marker[id="${CSS.escape(m[1])}"]`);
      if (!marker) continue;
      const shape = marker.querySelector("path,rect,circle,ellipse,polygon,line,polyline");
      if (!shape) continue;
      const shapeIndex = index.get(shape);
      const paint = getComputedStyle(shape).fill !== "none" ? "fill" : "stroke";
      const inner = markerInterior(shape, paint);
      if (!inner) continue;
      const vb = marker.viewBox.baseVal;
      const mw = marker.markerWidth.baseVal.value;
      const mh = marker.markerHeight.baseVal.value;
      let s = vb && vb.width > 0 ? Math.min(mw / vb.width, mh / vb.height) : 1;
      if (marker.getAttribute("markerUnits") !== "userSpaceOnUse") s *= Number.parseFloat(cs.strokeWidth);
      const refX = marker.refX.baseVal.value;
      const refY = marker.refY.baseVal.value;
      const tip = path.getPointAtLength(atEnd ? len : 0);
      const near = path.getPointAtLength(atEnd ? len - 0.5 : 0.5);
      let angle = atEnd ? Math.atan2(tip.y - near.y, tip.x - near.x) : Math.atan2(near.y - tip.y, near.x - tip.x);
      const orient = marker.getAttribute("orient") || "0";
      if (!atEnd && orient === "auto-start-reverse") angle += Math.PI;
      if (orient !== "auto" && orient !== "auto-start-reverse") angle = (Number.parseFloat(orient) * Math.PI) / 180;
      const lx = (inner.x - refX - (vb ? vb.x : 0)) * s;
      const ly = (inner.y - refY - (vb ? vb.y : 0)) * s;
      const ux = tip.x + lx * Math.cos(angle) - ly * Math.sin(angle);
      const uy = tip.y + lx * Math.sin(angle) + ly * Math.cos(angle);
      const c = toClient(path, ux, uy);
      if (inText(boxes, c)) continue;
      out.push({ index: shapeIndex, paint, c });
    }
    return out;
  };

  // Hit testing only sees the viewport, so each diagram is probed alone at the page origin
  // (the caller sizes the viewport to the largest diagram) and put back afterwards.
  const isolate = (d, all) => {
    const top = d.style.top;
    for (const o of all) if (o !== d) o.style.visibility = "hidden";
    d.style.top = "0px";
    window.scrollTo(0, 0);
    return () => {
      d.style.top = top;
      for (const o of all) o.style.visibility = "";
    };
  };

  window.parityProbe = (withSamples) => {
    const all = [...document.querySelectorAll(".parity-d")];
    return all.map((d) => {
      const restore = isolate(d, all);
      try {
        return probeOne(d, withSamples);
      } finally {
        restore();
      }
    });
  };

  const probeOne = (d, withSamples) => {
    {
      const svg = d.querySelector("svg");
      const origin = d.getBoundingClientRect();
      const els = compared(svg);
      const index = new Map(els.map((el, i) => [el, i]));
      const elements = els.map((el, i) => {
        const cs = getComputedStyle(el);
        const kind = kindOf(el);
        return {
          index: i,
          tag: el.tagName,
          kind,
          label: `${kind}:${ownerOf(el, svg)}:${el.tagName}.${[...el.classList].join(".")}#${i}`,
          fill: rgba(cs.fill, Number.parseFloat(cs.fillOpacity)),
          stroke: rgba(cs.stroke, Number.parseFloat(cs.strokeOpacity)),
          strokeWidth: Number.parseFloat(cs.strokeWidth),
          dash: cs.strokeDasharray,
          opacity: opacityChain(el, svg),
        };
      });
      const samples = [];
      if (withSamples) {
        const boxes = textBoxes(svg);
        const rel = (c) => ({ x: c.x - origin.left, y: c.y - origin.top });
        els.forEach((el, i) => {
          if (el.tagName === "text" || el.tagName === "tspan" || el.closest("marker")) return;
          const e = elements[i];
          if (e.fill && el.classList.contains("merlion-edge-path") === false) {
            for (const c of fillSamples(el, boxes)) samples.push({ index: i, paint: "fill", ...rel(c) });
          }
          if (e.stroke && e.strokeWidth >= 1) {
            const isEdge = el.classList.contains("merlion-edge-path");
            for (const c of strokeSamples(el, boxes, isEdge)) samples.push({ index: i, paint: "stroke", ...rel(c) });
          }
          if (el.tagName === "path" && el.closest(".merlion-edge")) {
            for (const s of markerSamples(el, svg, index, boxes)) samples.push({ index: s.index, paint: s.paint, ...rel(s.c) });
          }
        });
      }
      return { name: d.dataset.name, svgId: svg.id, elements, samples };
    }
  };
})();
