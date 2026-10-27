import type { GanttTask } from "../svg/gantt.ts";

export interface GanttCompat {
  readonly pass: boolean;
  readonly mode: "absolute UTC" | "time-of-day (day origin normalized)" | "day-of-month (month origin normalized)";
  readonly differences: readonly string[];
}
const visibleText = (value: string): string => value.replace(/<br\s*\/?>/gi, " ").replace(/\s+/g, "");

/** Only time-only formats may discard the reference renderer's current day. */
export const timeOnlyFormat = (source: string): boolean => {
  const format = /^\s*dateFormat\s+(.+)$/im.exec(source)?.[1]?.trim();
  return format !== undefined && !/[YMDXx]/.test(format.replace(/\[[^\]]*\]/g, "")) && /[HhmsS]/.test(format);
};

export const ganttCompat = (reference: readonly GanttTask[], candidate: readonly GanttTask[], source: string): GanttCompat => {
  const normalized = timeOnlyFormat(source);
  const format = /^\s*dateFormat\s+(.+)$/im.exec(source)?.[1]?.trim().replace(/\[[^\]]*\]/g, "");
  const dayOnly = format !== undefined && /D/.test(format) && !format.includes("DDD") && !/[YMQXx]/.test(format);
  const origin = (tasks: readonly GanttTask[]): number => {
    if (tasks.length === 0) return 0;
    const earliest = Math.min(...tasks.map((t) => t.start));
    if (normalized) return Math.floor(earliest / 86_400_000) * 86_400_000;
    if (dayOnly) { const d = new Date(earliest); return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), 1); }
    return 0;
  };
  const refOrigin = origin(reference);
  const candOrigin = origin(candidate);
  const differences: string[] = [];
  if (reference.length !== candidate.length) differences.push(`task count ${candidate.length}, expected ${reference.length}`);
  const byId = new Map<string, GanttTask[]>();
  for (const t of candidate) byId.set(t.id, [...(byId.get(t.id) ?? []), t]);
  for (const r of reference) {
    const c = byId.get(r.id)?.shift();
    if (c === undefined) { differences.push(`${r.id}: missing task`); continue; }
    if (!r.drawn || !c.drawn) differences.push(`${r.id}: missing visible mark or label`);
    if (visibleText(r.label) !== visibleText(c.label)) differences.push(`${r.id}: label differs`);
    if (visibleText(r.section) !== visibleText(c.section)) differences.push(`${r.id}: section differs`);
    if (!Number.isFinite(c.start) || c.start - candOrigin !== r.start - refOrigin) differences.push(`${r.id}: start differs (${c.start} vs ${r.start})`);
    if (!Number.isFinite(c.end) || c.end - candOrigin !== r.end - refOrigin) differences.push(`${r.id}: end differs (${c.end} vs ${r.end})`);
    if ([...r.flags].sort().join(" ") !== [...c.flags].sort().join(" ")) differences.push(`${r.id}: flags differ`);
  }
  return { pass: differences.length === 0, mode: normalized ? "time-of-day (day origin normalized)" : dayOnly ? "day-of-month (month origin normalized)" : "absolute UTC", differences };
};

export const ganttOutcome = (referenceError: string | null, candidateError: string | null, compat: GanttCompat | null) => {
  if (referenceError !== null) return candidateError !== null ? "both rejected" : "reference rejected";
  if (candidateError !== null) return "candidate rejected";
  return compat?.pass === true ? "matched" : "different";
};

/** A nonzero compatibility gate always measures the complete retained corpus. */
export const ganttGate = (total: number, measured: number, passed: number, minimum: number): string | null => {
  if (!Number.isFinite(minimum) || minimum < 0 || minimum > 100) return "min-compat must be between 0 and 100";
  if (minimum === 0) return null;
  if (measured !== total || total === 0) return "min-compat requires the complete nonempty corpus; omit --limit";
  const percent = 100 * passed / total;
  return percent < minimum ? `Gantt compatibility ${percent.toFixed(3)}% (${passed}/${total}) is below ${minimum}%` : null;
};
