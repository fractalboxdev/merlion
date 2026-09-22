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
