/**
 * Quadrant chart extraction from a Merlion or a mermaid SVG into one shape: title,
 * quadrant boxes and labels, axis labels and points, with the placement of each label
 * relative to the plot (specs/quadrant.md#compatibility).
 */
import { buildCtx, collect, type Ctx, labelText, parseViewBox, shapeBox } from "./extract.ts";
import { applyMatrix, boxOfPoints, type Box, IDENTITY } from "./geom.ts";
import { hasClass, parseXml, type XmlElement } from "./xml.ts";

/** Average advance of a glyph, in em, for the estimated extent of a label. */
const EM_PER_CHAR = 0.55;

export interface QuadrantBox {
  /** 1 top right, 2 top left, 3 bottom left, 4 bottom right, from the box's position. */
  readonly n: number;
  readonly fill: string | null;
  readonly label: string;
  /** Where the label sits in its box: `top` band or vertical `centre`. */
  readonly placement: "top" | "centre" | null;
}

export interface AxisText {
  readonly axis: "x" | "y";
  /** `low` is left or bottom, `high` right or top. */
  readonly end: "low" | "high";
  readonly text: string;
  /** Side of the plot the label sits on. */
  readonly side: "top" | "bottom" | "left" | "right" | "inside";
  /** `start`: begins at the low end of the axis; `half`: centred on its half. */
  readonly align: "start" | "half" | "other";
}

export interface QuadrantPoint {
  readonly label: string;
  /** Coordinates in 0–1 recovered from the circle's position in the plot. */
  readonly x: number;
  readonly y: number;
  readonly r: number;
  readonly fill: string | null;
  readonly stroke: string | null;
  readonly strokeWidth: number;
  /** The label's centre lies below the point's centre. */
  readonly labelBelow: boolean;
}

export interface QuadrantDrawing {
  /** Paints by Mermaid theme key, one entry per painted element. */
  readonly themePaints: Readonly<Record<string, readonly (string | null)[]>>;
  readonly title: string | null;
  readonly plot: Box | null;
  readonly quadrants: readonly QuadrantBox[];
  readonly axes: readonly AxisText[];
  readonly points: readonly QuadrantPoint[];
  readonly validSvg: boolean;
  readonly fits720: boolean;
}

const num = (v: string | undefined, d = 0): number => {
  const n = Number.parseFloat(v ?? "");
  return Number.isFinite(n) ? n : d;
};

/** Font size of a text: its own attribute, else the nearest ancestor's, else 14. */
const fontSize = (ctx: Ctx, t: XmlElement): number => {
  for (let e: XmlElement | undefined = t; e !== undefined; e = ctx.parent.get(e)) {
    const v = Number.parseFloat(e.attrs["font-size"] ?? "");
    if (Number.isFinite(v)) return v;
  }
  return 14;
};

/**
 * Estimated absolute box of a label. Merlion writes start-anchored lines with explicit
 * `x`/`y` baselines; mermaid writes one text at its transform's origin with
 * `text-anchor` and `dominant-baseline`.
 */
export const textBox = (ctx: Ctx, t: XmlElement): Box | null => {
  const text = labelText([t]);
  if (text.length === 0) return null;
  const size = fontSize(ctx, t);
  const lines = collect(t, (e) => e.name === "tspan" && "y" in e.attrs);
  const m = ctx.ctm.get(t) ?? IDENTITY;
  if (lines.length > 0) {
    const w = (text.length / lines.length) * EM_PER_CHAR * size;
    const xs = lines.map((l) => num(l.attrs["x"]));
    const ys = lines.map((l) => num(l.attrs["y"]));
    const x0 = Math.min(...xs);
    const top = Math.min(...ys) - 0.8 * size;
    const bottom = Math.max(...ys) + 0.4 * size;
    return boxOfPoints([applyMatrix(m, { x: x0, y: top }), applyMatrix(m, { x: x0 + w, y: bottom })]);
  }
  const w = text.length * EM_PER_CHAR * size;
  const anchor = t.attrs["text-anchor"] ?? "start";
  const base = t.attrs["dominant-baseline"] ?? "alphabetic";
  const x0 = num(t.attrs["x"]) + (anchor === "middle" ? -w / 2 : anchor === "end" ? -w : 0);
  const y0 = num(t.attrs["y"]) + (base === "hanging" ? 0 : base === "middle" ? -size / 2 : -0.8 * size);
  return boxOfPoints([applyMatrix(m, { x: x0, y: y0 }), applyMatrix(m, { x: x0 + w, y: y0 + size })]);
};

const centre = (b: Box) => ({ x: b.x + b.w / 2, y: b.y + b.h / 2 });

/** A paint as written, lower-cased; `null` when absent. */
const paint = (e: XmlElement | undefined, name: string): string | null => {
  const inline = new RegExp(`(?:^|;)\\s*${name}\\s*:\\s*([^;]+)`).exec(e?.attrs["style"] ?? "")?.[1];
  const v = (inline ?? e?.attrs[name])?.trim().toLowerCase();
  return v === undefined || v === "" ? null : v;
};

/** A colour comparable across renderers: `#rrggbb` from `#rgb`, `#rrggbb` or a bare hex. */
export const hexColour = (v: string | null): string | null => {
  if (v === null) return null;
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(v.trim());
  if (m === null) return null;
  const h = m[1]!.toLowerCase();
  return `#${h.length === 3 ? [...h].map((c) => c + c).join("") : h}`;
};

/** Normalises literal paints, including Chromium's serialisation of border colours. */
export const paintColour = (v: string | null): string | null => {
  if (v === null) return null;
  const compact = v.toLowerCase().replace(/\s+/g, "");
  const hex = hexColour(compact);
  if (hex !== null) return hex;
  const rgb = /^rgb\((\d+),(\d+),(\d+)\)$/.exec(compact);
  if (rgb !== null) return `#${rgb.slice(1).map((n) => Number(n).toString(16).padStart(2, "0")).join("")}`;
  return compact;
};

const strokeWidth = (e: XmlElement): number => {
  const style = /stroke-width\s*:\s*([\d.]+)/.exec(e.attrs["style"] ?? "")?.[1];
  return num(style ?? e.attrs["stroke-width"]);
};

const quadrantNumber = (b: Box, plot: Box): number => {
  const c = centre(b);
  const right = c.x > plot.x + plot.w / 2;
  const top = c.y < plot.y + plot.h / 2;
  return top ? (right ? 1 : 2) : right ? 4 : 3;
};

export const extractQuadrant = (svg: string): QuadrantDrawing => {
  let root: XmlElement;
  try {
    root = parseXml(svg);
  } catch {
    return { themePaints: {}, title: null, plot: null, quadrants: [], axes: [], points: [], validSvg: false, fits720: false };
  }
  const ctx = buildCtx(root);
  const view = parseViewBox(root);
  const merlion = hasClass(root, "merlion-quadrant");
  const groups = merlion
    ? collect(root, (e) => hasClass(e, "merlion-quadrant-region"))
    : collect(root, (e) => e.name === "g" && hasClass(e, "quadrant"));
  const boxes = groups.map((g) => {
    const rect = collect(g, (e) => e.name === "rect")[0];
    const text = collect(g, (e) => e.name === "text")[0];
    return { rect, text, box: rect === undefined ? null : shapeBox(ctx, rect) };
  });
  const plot = boxOfPoints(boxes.flatMap((b) => (b.box === null ? [] : [{ x: b.box.x, y: b.box.y }, { x: b.box.x + b.box.w, y: b.box.y + b.box.h }])));
  const quadrants: QuadrantBox[] = plot === null ? [] : boxes.flatMap((b) => {
    if (b.box === null) return [];
    const label = b.text === undefined ? "" : labelText([b.text]);
    const tb = b.text === undefined ? null : textBox(ctx, b.text);
    const ratio = tb === null ? null : (centre(tb).y - b.box.y) / b.box.h;
    const placement: QuadrantBox["placement"] = ratio === null ? null : ratio < 0.3 ? "top" : ratio > 0.35 && ratio < 0.65 ? "centre" : null;
    return [{ n: quadrantNumber(b.box, plot), fill: paint(b.rect, "fill"), label, placement }];
  }).sort((a, b) => a.n - b.n);

  const titleEl = merlion
    ? collect(root, (e) => e.name === "text" && hasClass(e, "merlion-quadrant-title"))[0]
    : collect(root, (e) => e.name === "g" && hasClass(e, "title")).flatMap((g) => collect(g, (e) => e.name === "text"))[0];
  const title = titleEl === undefined ? null : labelText([titleEl]) || null;

  const axisTexts = merlion
    ? collect(root, (e) => hasClass(e, "merlion-quadrant-axis")).map((g) => ({
        axis: (g.attrs["data-merlion-axis"] === "y" ? "y" : "x") as "x" | "y",
        text: collect(g, (e) => e.name === "text")[0],
      }))
    : collect(root, (e) => e.name === "g" && hasClass(e, "labels"))
        .flatMap((g) => collect(g, (e) => e.name === "text"))
        .map((t) => ({ axis: (/rotate\(\s*-90/.test(t.attrs["transform"] ?? "") ? "y" : "x") as "x" | "y", text: t }));
  const axes: AxisText[] = [];
  for (const axis of ["x", "y"] as const) {
    const items = axisTexts
      .filter((a) => a.axis === axis && a.text !== undefined)
      .map((a) => ({ text: labelText([a.text!]), box: textBox(ctx, a.text!) }))
      .filter((a): a is { text: string; box: Box } => a.box !== null && a.text !== "");
    // Low end first: left to right, bottom to top.
    items.sort((a, b) => (axis === "x" ? centre(a.box).x - centre(b.box).x : centre(b.box).y - centre(a.box).y));
    items.forEach((it, i) => {
      const c = centre(it.box);
      let side: AxisText["side"] = "inside";
      let align: AxisText["align"] = "other";
      if (plot !== null) {
        if (axis === "x") side = c.y < plot.y ? "top" : c.y > plot.y + plot.h ? "bottom" : "inside";
        else side = c.x < plot.x ? "left" : c.x > plot.x + plot.w ? "right" : "inside";
        const high = items.length > 1 && i === items.length - 1;
        if (axis === "x") {
          const quarter = plot.x + plot.w * (high ? 0.75 : 0.25);
          if (Math.abs(it.box.x - plot.x) < 2) align = "start";
          else if (Math.abs(c.x - quarter) < 0.15 * plot.w) align = "half";
        } else {
          const quarter = plot.y + plot.h * (high ? 0.25 : 0.75);
          if (Math.abs(it.box.y + it.box.h - (plot.y + plot.h)) < 2) align = "start";
          else if (Math.abs(c.y - quarter) < 0.15 * plot.h) align = "half";
        }
      }
      axes.push({ axis, end: items.length > 1 && i === items.length - 1 ? "high" : "low", text: it.text, side, align });
    });
  }

  const pointGroups = merlion
    ? collect(root, (e) => hasClass(e, "merlion-quadrant-point"))
    : collect(root, (e) => e.name === "g" && hasClass(e, "data-point"));
  const points: QuadrantPoint[] = plot === null ? [] : pointGroups.flatMap((g) => {
    const circle = collect(g, (e) => e.name === "circle")[0];
    const text = collect(g, (e) => e.name === "text")[0];
    if (circle === undefined) return [];
    const m = ctx.ctm.get(circle) ?? IDENTITY;
    const c = applyMatrix(m, { x: num(circle.attrs["cx"]), y: num(circle.attrs["cy"]) });
    const tb = text === undefined ? null : textBox(ctx, text);
    return [{
      label: text === undefined ? "" : labelText([text]),
      x: (c.x - plot.x) / plot.w,
      y: 1 - (c.y - plot.y) / plot.h,
      r: num(circle.attrs["r"]),
      fill: paint(circle, "fill"),
      stroke: paint(circle, "stroke"),
      strokeWidth: strokeWidth(circle),
      labelBelow: tb !== null && centre(tb).y > c.y,
    }];
  });
  const validSvg = view !== null && view.w > 0 && view.h > 0 && Number.isFinite(view.w + view.h);
  const themePaints: Record<string, Array<string | null>> = {};
  const add = (key: string, e: XmlElement | undefined, attribute = "fill") => {
    if (e !== undefined) (themePaints[key] ??= []).push(paint(e, attribute));
  };
  for (const b of boxes) {
    if (plot === null || b.box === null) continue;
    const n = quadrantNumber(b.box, plot);
    add(`quadrant${n}Fill`, b.rect);
    add(`quadrant${n}TextFill`, b.text);
  }
  for (const a of axisTexts) add(a.axis === "x" ? "quadrantXAxisTextFill" : "quadrantYAxisTextFill", a.text);
  for (const g of pointGroups) {
    add("quadrantPointFill", collect(g, (e) => e.name === "circle")[0]);
    add("quadrantPointTextFill", collect(g, (e) => e.name === "text")[0]);
  }
  add("quadrantTitleFill", titleEl);
  const borders = merlion
    ? collect(root, (e) => e.name === "line" && (hasClass(e, "merlion-quadrant-border") || hasClass(e, "merlion-quadrant-divider")))
    : collect(root, (e) => e.name === "g" && hasClass(e, "border")).flatMap((g) => collect(g, (e) => e.name === "line"));
  borders.forEach((line, i) => add(merlion
    ? hasClass(line, "merlion-quadrant-border") ? "quadrantExternalBorderStrokeFill" : "quadrantInternalBorderStrokeFill"
    : i < 4 ? "quadrantExternalBorderStrokeFill" : "quadrantInternalBorderStrokeFill", line, "stroke"));
  return { themePaints, title, plot, quadrants, axes, points, validSvg, fits720: validSvg && view!.w <= 720 };
};
