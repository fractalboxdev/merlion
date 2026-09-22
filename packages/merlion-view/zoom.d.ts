/** A view: `transform: translate(x px, y px) scale(s)` with `transform-origin: 0 0`. */
export interface View {
  readonly s: number;
  readonly x: number;
  readonly y: number;
}

export declare const MIN_SCALE: 0.25;
export declare const MAX_SCALE: 8;
export declare const MIN_LABEL_PX: 9;
export declare const MAX_RANK: 15;
export declare const STEP: number;
export declare const IDENTITY: View;

export declare function clampScale(s: number): number;
export declare function zoomAt(v: View, factor: number, px: number, py: number): View;
export declare function panBy(v: View, dx: number, dy: number): View;
export declare function wheelFactor(deltaY: number, deltaMode: number): number;
export declare function pinchFactor(d0: number, d1: number): number;
export declare function keyView(
  key: string,
  v: View,
  box: { cx: number; cy: number; w: number; h: number },
): View | null;
export declare function labelPx(fontSize: number, fit: number, s: number): number;
export declare function rankLimit(px: number): number | null;
export declare function needsControls(natural: number | undefined, container: number, attr: string | null): boolean;
export declare function viewBoxSize(attr: string | null | undefined): { w: number; h: number } | null;
export declare function transformOf(v: View): string;
export declare function semanticLimit(fontSize: number, fit: number, s: number): number | null;
export declare function clampView(
  v: View,
  content: { w: number; h: number },
  box: { x: number; y: number; w: number; h: number },
): View;
export declare function fitsBox(v: View, content: { w: number; h: number }, box: { w: number; h: number }): boolean;

/** What a pointer went down on. */
export type Hit = "text" | "shape" | "bg";
/** Whether a primary pointer-down starts a pan (specs/viewer.md#gestures). */
export declare function panIntent(hit: Hit, pointerType: string, fits: boolean, zoomed: boolean): boolean;
/** Whether a click is a tap: moved < 4 px, not a double-click's second click, no text selected. */
export declare function isTap(dx: number, dy: number, detail: number, selected: boolean): boolean;
export declare function resetsOnDoubleClick(hit: Hit, selected: boolean): boolean;

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}
/** Popover placement next to `E` inside `C` (specs/interaction.md#placement); null when no side has 48 px. */
export declare function place(
  E: Rect,
  C: Rect,
  w: number,
  h: number,
  gap?: number,
): { side: "below" | "above" | "right" | "left"; x: number; y: number; w?: number; h?: number } | null;
