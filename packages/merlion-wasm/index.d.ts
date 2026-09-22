// Types for the hand-written glue (specs/integrations.md#fractalboxdevmerlion-wasm).

export interface RenderOptions {
  /** Container width in px the layout fits (`target_width`); default 720. */
  width?: number;
  /** `"auto"` lets the engine choose TB or LR to fit `width`; default `"source"`. */
  direction?: "auto" | "source";
  edgeStyle?: "orthogonal" | "polyline" | "spline";
  font?: "link" | "embed" | "system";
  /** Every warning and repair becomes an error. */
  strict?: boolean;
  /** SVG root id, `[a-z][a-z0-9-]{0,31}`; pass one per diagram when a page holds several. */
  idPrefix?: string;
  /** The previous SVG of this diagram, for stable layout; over 1 MiB it is ignored with I022. */
  hint?: string;
  /** Work budget in fuel units (a whole number up to 2^53). */
  fuel?: number;
}

export interface Fix {
  byteStart: number;
  byteEnd: number;
  replacement: string;
}

export interface Diagnostic {
  severity: "error" | "warning" | "repair" | "info";
  /** `E0xx`, `W0xx`, `R0xx` or `I0xx`. */
  code: string;
  /** 1-based; 0 when the diagnostic has no location. */
  line: number;
  /** 1-based, in Unicode scalar values; 0 when the diagnostic has no location. */
  column: number;
  /** UTF-8 byte offsets into the source. */
  byteStart: number;
  byteEnd: number;
  message: string;
  /** Present for every repair. */
  fix: Fix | null;
}

export type RenderError =
  | { kind: "parse" }
  | { kind: "unsupported_diagram"; header: string }
  | { kind: "too_large"; what: string }
  | { kind: "invalid_input"; message: string }
  | { kind: "internal" };

export interface RenderResult {
  svg: string | null;
  outline: string | null;
  diagnostics: Diagnostic[];
  error: RenderError | null;
  fuelUsed: number;
}

/** Browser: instantiates from `wasm`, or fetches `merlion.wasm` next to this module. */
export function init(
  wasm?: BufferSource | WebAssembly.Module | URL | string | Response | Promise<Response>,
): Promise<void>;
/** Node, at build time. */
export function initSync(wasm: BufferSource | WebAssembly.Module): void;
/** Synchronous after initialisation; throws `TypeError` on invalid arguments or options. */
export function render(source: string, options?: RenderOptions): RenderResult;
export function check(source: string, options?: { strict?: boolean }): Diagnostic[];

/** @internal Exercises the trap-recovery path of a module built with `test-trap`. */
export function __trapForTests(): RenderResult;
