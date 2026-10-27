import { buildCtx, collect, labelText, parseViewBox, shapeBox } from "./extract.ts";
import { hasClass, parseXml } from "./xml.ts";
import type { Box } from "./geom.ts";

/** Resolved dates are checked together with a drawn mark and its visible label. */
export interface GanttTask {
  readonly id: string;
  readonly label: string;
  readonly section: string;
  readonly start: number;
  readonly end: number;
  readonly flags: readonly string[];
  readonly drawn: boolean;
  readonly markBox?: Box | undefined;
}

export interface GanttDrawing {
  readonly tasks: readonly GanttTask[];
  readonly ticks: readonly { time: number; x: number }[];
  readonly fits720: boolean;
  readonly validSvg: boolean;
}

export const extractGantt = (svg: string): GanttDrawing => {
  const root = parseXml(svg);
  const ctx = buildCtx(root);
  const box = parseViewBox(root);
  const tasks = collect(root, (e) => hasClass(e, "merlion-gantt-task")).map((g): GanttTask => {
    const hidden = (e: typeof g): boolean => e.attrs["display"] === "none" || e.attrs["visibility"] === "hidden" || /(?:display\s*:\s*none|visibility\s*:\s*hidden)/.test(e.attrs["style"] ?? "");
    const flags = (g.attrs["data-merlion-flags"] ?? "").split(/\s+/).filter(Boolean).sort();
    const labels = collect(g, (e) => e.name === "text" && hasClass(e, "merlion-gantt-task-text"));
    const marks = collect(g, (e) => hasClass(e, "merlion-shape") && !hidden(e) &&
      (flags.includes("milestone") ? ["path", "polygon"].includes(e.name) : flags.includes("vert") ? ["line", "rect"].includes(e.name) : e.name === "rect"));
    const markBox = marks.map((e) => {
      const b = shapeBox(ctx, e);
      return b !== null && Number.isFinite(b.x + b.y + b.w + b.h) && (b.w > 0 || b.h > 0) ? b : null;
    }).find((b) => b !== null) ?? undefined;
    return {
      id: g.attrs["data-merlion-id"] ?? "",
      label: labelText(labels),
      section: g.attrs["data-merlion-section"] ?? "",
      start: Number(g.attrs["data-merlion-start"] ?? Number.NaN),
      end: Number(g.attrs["data-merlion-end"] ?? Number.NaN),
      flags,
      drawn: !hidden(g) && markBox !== undefined && labels.some((e) => !hidden(e)),
      markBox,
    };
  });
  const ticks = collect(root, (e) => hasClass(e, "merlion-gantt-grid") && "data-merlion-time" in e.attrs).flatMap((e) => {
    const time = Number(e.attrs["data-merlion-time"]);
    const b = shapeBox(ctx, e);
    return Number.isFinite(time) && b !== null ? [{ time, x: b.x }] : [];
  });
  return { tasks, ticks, fits720: box !== null && box.w <= 720, validSvg: root.name === "svg" && box !== null && box.w > 0 && box.h > 0 };
};

/** Every mark shares one increasing time scale, independent of overall SVG width. */
export const ganttGeometry = (tasks: readonly GanttTask[], ticks: GanttDrawing["ticks"] = []): readonly string[] => {
  const sortedTicks = [...ticks].sort((a, b) => a.time - b.time);
  const axisFirst = sortedTicks[0], axisLast = sortedTicks.at(-1);
  if (axisFirst !== undefined && axisLast !== undefined && axisLast.time > axisFirst.time) {
    const scale = (axisLast.x - axisFirst.x) / (axisLast.time - axisFirst.time);
    if (!(scale > 0)) return ["axis grid does not follow increasing time"];
    const xAt = (time: number): number => axisFirst.x + (time - axisFirst.time) * scale;
    const differences: string[] = [];
    for (const tick of ticks) if (Math.abs(tick.x - xAt(tick.time)) > 1.5) differences.push("axis tick: position does not match resolved time scale");
    for (const t of tasks) {
      const b = t.markBox;
      if (b === undefined) continue;
      if (t.flags.includes("milestone")) {
        if (Math.abs(b.x + b.w / 2 - xAt((t.start + t.end) / 2)) > 1.5) differences.push(`${t.id}: milestone position differs from axis`);
      } else if (t.flags.includes("vert")) {
        if (Math.abs(b.x + b.w / 2 - xAt(t.start)) > 1.5) differences.push(`${t.id}: vertical-marker position differs from axis`);
      } else {
        if (Math.abs(b.x - xAt(t.start)) > 1.5) differences.push(`${t.id}: start position differs from axis`);
        // A short non-milestone span remains visible at the SVG contract's 2 px minimum.
        const expectedWidth = Math.max(2, (t.end - t.start) * scale);
        if (Math.abs(b.w - expectedWidth) > 0.1) differences.push(`${t.id}: drawn width differs from duration on axis`);
      }
    }
    return differences;
  }
  const anchors: Array<{ time: number; x: number; id: string }> = ticks.map((t) => ({ ...t, id: "axis tick" }));
  for (const t of tasks) {
    const b = t.markBox;
    if (b === undefined) continue;
    if (t.flags.includes("milestone")) anchors.push({ time: (t.start + t.end) / 2, x: b.x + b.w / 2, id: t.id });
    else if (t.flags.includes("vert")) anchors.push({ time: t.start, x: b.x + b.w / 2, id: t.id });
    else {
      anchors.push({ time: t.start, x: b.x, id: t.id });
      if (t.end > t.start) anchors.push({ time: t.end, x: b.x + b.w, id: t.id });
    }
  }
  const sorted = [...anchors].sort((a, b) => a.time - b.time);
  const first = sorted[0], last = sorted.at(-1);
  if (first === undefined || last === undefined || first.time === last.time) return [];
  const scale = (last.x - first.x) / (last.time - first.time);
  if (!(scale > 0)) return ["task marks do not follow an increasing time axis"];
  return anchors.filter((a) => Math.abs(a.x - (first.x + (a.time - first.time) * scale)) > 1.5)
    .map((a) => `${a.id}: mark position does not match resolved time scale`);
};
