import { describe, expect, it } from "vitest";
import { ganttCompat, ganttGate, ganttOutcome, timeOnlyFormat } from "../src/metrics/gantt.ts";
import { extractGantt, ganttGeometry, type GanttTask } from "../src/svg/gantt.ts";

const task: GanttTask = { id: "build", label: "Build & test", section: "Delivery", start: 0, end: 86400000, flags: ["crit"], drawn: true };
const source = "gantt\ndateFormat YYYY-MM-DD";
const svg = `<svg viewBox="0 0 720 200"><g class="merlion-gantt-task" data-merlion-id="build" data-merlion-section="Delivery" data-merlion-start="0" data-merlion-end="86400000" data-merlion-flags="crit"><rect class="merlion-shape" x="100" y="50" width="200" height="20"/><text class="merlion-gantt-task-text">Build &amp; test</text></g></svg>`;

describe("Gantt schedule comparison", () => {
  it("requires a visible mark and task label alongside schedule metadata", () => {
    expect(ganttCompat([task], extractGantt(svg).tasks, source).pass).toBe(true);
    expect(ganttCompat([task], extractGantt(svg.replace(/<rect[^>]*\/>/, "")).tasks, source).pass).toBe(false);
    expect(ganttCompat([task], extractGantt(svg.replace(/<text.*?<\/text>/, "")).tasks, source).pass).toBe(false);
    expect(extractGantt(svg.replace('<rect ', '<rect display="none" ')).tasks[0]?.drawn).toBe(false);
    expect(extractGantt(svg.replace('data-merlion-flags="crit"', 'data-merlion-flags="milestone"')).tasks[0]?.drawn).toBe(false);
  });
  it("detects dropped, renamed, rescheduled and restyled tasks", () => {
    for (const change of [{ id: "other" }, { label: "Deploy" }, { section: "Other" }, { start: 1 }, { end: 1 }, { flags: ["done"] }, { drawn: false }]) {
      expect(ganttCompat([task], [{ ...task, ...change }], source).pass).toBe(false);
    }
    expect(ganttCompat([task], [], source).pass).toBe(false);
    expect(ganttCompat([], [], source).pass).toBe(true);
    expect(extractGantt("").validSvg).toBe(false);
    expect(ganttCompat([task], [task, task], source).pass).toBe(false);
    expect(ganttCompat([task, task], [task, task], source).pass).toBe(true);
    expect(ganttCompat([{ ...task, section: "Delivery<br\t/>plan" }], [{ ...task, section: "Delivery\nplan" }], source).pass).toBe(true);
  });
  it("compares day-of-month and duration with an explicit month-origin mode", () => {
    const ref = [{ ...task, start: Date.UTC(2026, 9, 2), end: Date.UTC(2026, 9, 3) }];
    const cand = [{ ...task, start: Date.UTC(1970, 0, 2), end: Date.UTC(1970, 0, 3) }];
    const result = ganttCompat(ref, cand, "gantt\ndateFormat D");
    expect(result.pass).toBe(true);
    expect(result.mode).toBe("day-of-month (month origin normalized)");
    expect(ganttCompat(ref, [{ ...cand[0]!, start: Date.UTC(1970, 0, 3) }], "gantt\ndateFormat D").pass).toBe(false);
    expect(ganttCompat(ref, cand, source).pass).toBe(false);
  });
  it("normalizes only common day origins for time-only formats", () => {
    const ref = [{ ...task, start: 20400 * 86400000 + 3600000, end: 20400 * 86400000 + 7200000 }];
    const cand = [{ ...task, start: 3600000, end: 7200000 }];
    const result = ganttCompat(ref, cand, "gantt\ndateFormat HH:mm");
    expect(result.pass).toBe(true);
    expect(result.mode).toBe("time-of-day (day origin normalized)");
    expect(ganttCompat(ref, cand, source).pass).toBe(false);
    expect(ganttCompat(ref, [{ ...cand[0]!, end: 7200001 }], "gantt\ndateFormat HH:mm").pass).toBe(false);
    expect(timeOnlyFormat("gantt\ndateFormat x")).toBe(false);
    expect(timeOnlyFormat("gantt\ndateFormat SSS")).toBe(true);
  });
  it("gives no successful-render credit to rejected or unmeasured sources", () => {
    expect(ganttOutcome("invalid date", "invalid date", null)).toBe("both rejected");
    expect(ganttOutcome(null, "unsupported", null)).toBe("candidate rejected");
    expect(ganttOutcome("invalid", null, null)).toBe("reference rejected");
    expect(ganttOutcome(null, null, null)).toBe("different");
  });
  it("rejects marks at the wrong time despite correct metadata", () => {
    const a = { ...task, start: 0, end: 10, markBox: { x: 100, y: 10, w: 100, h: 20 } };
    const b = { ...task, id: "later", start: 10, end: 20, markBox: { x: 200, y: 40, w: 100, h: 20 } };
    expect(ganttGeometry([a, b])).toEqual([]);
    expect(ganttGeometry([a, { ...b, markBox: { ...b.markBox, x: 150 } }]).length).toBeGreaterThan(0);
    expect(ganttGeometry([a, { ...b, flags: ["milestone"], markBox: { x: 240, y: 40, w: 20, h: 20 } }])).toEqual([]);
    expect(ganttGeometry([a, b], [{ time: 0, x: 100 }, { time: 20, x: 300 }])).toEqual([]);
    expect(ganttGeometry([a, b], [{ time: 0, x: 120 }, { time: 20, x: 320 }]).length).toBeGreaterThan(0);
    const tiny = { ...a, start: 0, end: 0.001, markBox: { x: 100, y: 10, w: 2, h: 20 } };
    expect(ganttGeometry([tiny], [{ time: 0, x: 100 }, { time: 20, x: 300 }])).toEqual([]);
    expect(ganttGeometry([{ ...tiny, markBox: { ...tiny.markBox, w: 5 } }], [{ time: 0, x: 100 }, { time: 20, x: 300 }]).length).toBeGreaterThan(0);
    const axis = extractGantt(svg.replace('<g ', '<line class="merlion-gantt-grid" data-merlion-time="0" x1="100" x2="100" y1="0" y2="100"/><g '));
    expect(axis.ticks).toEqual([{ time: 0, x: 100 }]);
  });
  it("gates the full denominator without rounding a shortfall into a pass", () => {
    expect(ganttGate(64, 64, 58, 90)).toBeNull();
    expect(ganttGate(64, 64, 57, 90)).toContain("below 90%");
    expect(ganttGate(64, 1, 1, 90)).toContain("complete");
    expect(ganttGate(0, 0, 0, 90)).toContain("complete");
    expect(ganttGate(10000, 10000, 8999, 90)).toContain("below 90%");
    expect(ganttGate(64, 1, 0, 0)).toBeNull();
    expect(ganttGate(64, 64, 64, -1)).toContain("between 0 and 100");
    expect(ganttGate(64, 64, 64, 101)).toContain("between 0 and 100");
  });
});
