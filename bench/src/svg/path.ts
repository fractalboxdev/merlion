/**
 * SVG path `d` parsing into sampled polylines.
 *
 * Curves are sampled uniformly in their parameter (`CURVE_SAMPLES` segments
 * per curve), which is accurate enough for crossing counts and lengths at
 * diagram scale. Each subpath also keeps its *vertices*: the endpoints of every
 * command, without curve samples, which is what a router's bends are made of.
 */
import type { Point } from "./geom.ts";

export const CURVE_SAMPLES = 12;

export interface SubPath {
  readonly points: Point[];
  readonly vertices: Point[];
  readonly closed: boolean;
}

const TOKEN_RE = /([MmLlHhVvCcSsQqTtAaZz])|([-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)/g;

const ARGS: Readonly<Record<string, number>> = { m: 2, l: 2, h: 1, v: 1, c: 6, s: 4, q: 4, t: 2, a: 7, z: 0 };

const cubic = (p0: Point, p1: Point, p2: Point, p3: Point, t: number): Point => {
  const u = 1 - t;
  const a = u * u * u;
  const b = 3 * u * u * t;
  const c = 3 * u * t * t;
  const d = t * t * t;
  return { x: a * p0.x + b * p1.x + c * p2.x + d * p3.x, y: a * p0.y + b * p1.y + c * p2.y + d * p3.y };
};

const quad = (p0: Point, p1: Point, p2: Point, t: number): Point => {
  const u = 1 - t;
  return {
    x: u * u * p0.x + 2 * u * t * p1.x + t * t * p2.x,
    y: u * u * p0.y + 2 * u * t * p1.y + t * t * p2.y,
  };
};

/** Samples an elliptical arc (SVG 1.1 implementation notes, F.6.5 endpoint → centre conversion). */
const arcPoints = (
  p0: Point,
  rxIn: number,
  ryIn: number,
  phiDeg: number,
  largeArc: boolean,
  sweep: boolean,
  p1: Point,
): Point[] => {
  let rx = Math.abs(rxIn);
  let ry = Math.abs(ryIn);
  if (rx === 0 || ry === 0 || (p0.x === p1.x && p0.y === p1.y)) return [p1];
  const phi = (phiDeg * Math.PI) / 180;
  const cos = Math.cos(phi);
  const sin = Math.sin(phi);
  const dx = (p0.x - p1.x) / 2;
  const dy = (p0.y - p1.y) / 2;
  const x1 = cos * dx + sin * dy;
  const y1 = -sin * dx + cos * dy;
  const lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
  if (lambda > 1) {
    const s = Math.sqrt(lambda);
    rx *= s;
    ry *= s;
  }
  const num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
  const den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
  let coef = den === 0 ? 0 : Math.sqrt(Math.max(0, num / den));
  if (largeArc === sweep) coef = -coef;
  const cxp = (coef * rx * y1) / ry;
  const cyp = (-coef * ry * x1) / rx;
  const cx = cos * cxp - sin * cyp + (p0.x + p1.x) / 2;
  const cy = sin * cxp + cos * cyp + (p0.y + p1.y) / 2;
  const angle = (ux: number, uy: number, vx: number, vy: number): number => Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy);
  const theta1 = angle(1, 0, (x1 - cxp) / rx, (y1 - cyp) / ry);
  let delta = angle((x1 - cxp) / rx, (y1 - cyp) / ry, (-x1 - cxp) / rx, (-y1 - cyp) / ry);
  if (!sweep && delta > 0) delta -= 2 * Math.PI;
  if (sweep && delta < 0) delta += 2 * Math.PI;
  const out: Point[] = [];
  for (let k = 1; k <= CURVE_SAMPLES; k++) {
    const th = theta1 + (delta * k) / CURVE_SAMPLES;
    const ex = rx * Math.cos(th);
    const ey = ry * Math.sin(th);
    out.push({ x: cos * ex - sin * ey + cx, y: sin * ex + cos * ey + cy });
  }
  out[out.length - 1] = p1;
  return out;
};

/** Parses `d` into subpaths. Never throws; parsing stops at the first malformed command. */
export const parsePath = (d: string): SubPath[] => {
  const tokens: Array<string | number> = [];
  for (const m of d.matchAll(TOKEN_RE)) {
    if (m[1] !== undefined) tokens.push(m[1]);
    else if (m[2] !== undefined) tokens.push(Number(m[2]));
  }

  const subs: SubPath[] = [];
  let cur: { points: Point[]; vertices: Point[]; closed: boolean } | null = null;
  let pos: Point = { x: 0, y: 0 };
  let start: Point = { x: 0, y: 0 };
  let lastCtrl: Point | null = null;
  let lastCmd = "";
  let cmd = "";
  let i = 0;

  const emit = (pts: readonly Point[], end: Point): void => {
    if (cur === null) {
      cur = { points: [pos], vertices: [pos], closed: false };
      subs.push(cur);
    }
    cur.points.push(...pts);
    cur.vertices.push(end);
    pos = end;
  };

  while (i < tokens.length) {
    const tok = tokens[i];
    if (typeof tok === "string") {
      cmd = tok;
      i++;
      if (cmd === "z" || cmd === "Z") {
        if (cur !== null) {
          if (pos.x !== start.x || pos.y !== start.y) emit([start], start);
          (cur as { closed: boolean }).closed = true;
        }
        pos = start;
        cur = null;
        lastCtrl = null;
        lastCmd = "z";
        continue;
      }
    } else if (cmd === "" || cmd === "z" || cmd === "Z") {
      break; // numbers without a command
    }
    const lower = cmd.toLowerCase();
    const n = ARGS[lower] ?? 0;
    const args: number[] = [];
    for (let k = 0; k < n; k++) {
      const v = tokens[i + k];
      if (typeof v !== "number") return subs;
      args.push(v);
    }
    i += n;
    const rel = cmd !== cmd.toUpperCase();
    const ox = rel ? pos.x : 0;
    const oy = rel ? pos.y : 0;
    const P = (k: number): Point => ({ x: ox + args[k]!, y: oy + args[k + 1]! });
    switch (lower) {
      case "m": {
        pos = P(0);
        start = pos;
        cur = { points: [pos], vertices: [pos], closed: false };
        subs.push(cur);
        lastCtrl = null;
        // Subsequent pairs after a moveto are implicit linetos.
        cmd = rel ? "l" : "L";
        break;
      }
      case "l": {
        const p = P(0);
        emit([p], p);
        lastCtrl = null;
        break;
      }
      case "h": {
        const p = { x: ox + args[0]!, y: pos.y };
        emit([p], p);
        lastCtrl = null;
        break;
      }
      case "v": {
        const p = { x: pos.x, y: oy + args[0]! };
        emit([p], p);
        lastCtrl = null;
        break;
      }
      case "c":
      case "s": {
        const p0 = pos;
        let c1: Point;
        let c2: Point;
        let p3: Point;
        if (lower === "c") {
          c1 = P(0);
          c2 = P(2);
          p3 = P(4);
        } else {
          const reflect = lastCtrl !== null && "cs".includes(lastCmd);
          c1 = reflect && lastCtrl !== null ? { x: 2 * p0.x - lastCtrl.x, y: 2 * p0.y - lastCtrl.y } : p0;
          c2 = P(0);
          p3 = P(2);
        }
        const pts: Point[] = [];
        for (let k = 1; k < CURVE_SAMPLES; k++) pts.push(cubic(p0, c1, c2, p3, k / CURVE_SAMPLES));
        pts.push(p3);
        emit(pts, p3);
        lastCtrl = c2;
        break;
      }
      case "q":
      case "t": {
        const p0 = pos;
        let c: Point;
        let p2: Point;
        if (lower === "q") {
          c = P(0);
          p2 = P(2);
        } else {
          const reflect = lastCtrl !== null && "qt".includes(lastCmd);
          c = reflect && lastCtrl !== null ? { x: 2 * p0.x - lastCtrl.x, y: 2 * p0.y - lastCtrl.y } : p0;
          p2 = P(0);
        }
        const pts: Point[] = [];
        for (let k = 1; k < CURVE_SAMPLES; k++) pts.push(quad(p0, c, p2, k / CURVE_SAMPLES));
        pts.push(p2);
        emit(pts, p2);
        lastCtrl = c;
        break;
      }
      case "a": {
        const end = P(5);
        emit(arcPoints(pos, args[0]!, args[1]!, args[2]!, args[3] !== 0, args[4] !== 0, end), end);
        lastCtrl = null;
        break;
      }
    }
    lastCmd = lower;
  }
  return subs;
};
