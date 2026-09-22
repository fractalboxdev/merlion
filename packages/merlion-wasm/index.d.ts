// Types for the hand-written glue (specs/integrations.md#fractalboxdevmerlion-wasm).
// This file is the JavaScript contract of Merlion: @fractalboxdev/merlion-rehype, the
// docs playground and every other JS caller use these names and shapes. Option names are
// camelCase; the core's snake_case names (`target_width`, `id_prefix`, `edge_style`)
// throw a TypeError naming the camelCase one, and every other unknown key throws a
// TypeError naming it, so an older module never silently ignores an option.

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
  /**
   * Literals to bake into a standalone SVG, from `compileStylesheet`. The glue checks
   * the shape and the core checks every value against its token's grammar; either
   * failure throws a TypeError. Inline SVGs on a page render without one and follow the
   * page's compiled CSS. It never affects layout.
   */
  palette?: Palette;
}

/** A per-role tone and dash. `dash: []` is `none`. */
export interface PaletteTone {
  /** `#rrggbb` or `#rrggbbaa`. */
  tone?: string;
  /** Up to 8 numbers from 0 to 100. */
  dash?: number[];
}

/**
 * The literals of one theme resolved from a stylesheet (specs/svg-output.md#palette).
 * Role keys are token names without `--merlion-`: colour roles (`bg`, `accent`, …),
 * `series-1` … `series-8`, `stroke` (a number from 0 to 20, as a string) and
 * `c-{class}-{fill|stroke|color}` (a colour, or `none` for fill and stroke). Colours
 * are `#` hex. Tone records keep cascade order: a later role wins on an element that
 * carries both.
 */
export interface Palette {
  roles: Record<string, string>;
  /** Node and edge roles. */
  tones?: Record<string, PaletteTone>;
  /** Cluster roles. */
  clusterTones?: Record<string, PaletteTone>;
  /** The `prefers-color-scheme: dark` variant, same keys as `roles`. */
  dark?: Record<string, string>;
  darkTones?: Record<string, PaletteTone>;
  darkClusterTones?: Record<string, PaletteTone>;
}

export interface StylesheetOptions {
  /** A `[data-theme="<name>"]` block resolved over `:root`; default `:root` alone. */
  theme?: string;
  /** A named block baked as the `prefers-color-scheme: dark` variant. */
  autoDark?: string;
  /** W017-W019 become errors; any error returns `css: null` and `palette: null`. */
  strict?: boolean;
}

export interface CompiledStylesheet {
  /** Page CSS: literal values under fixed selector shapes; null on an error. */
  css: string | null;
  palette: Palette | null;
  diagnostics: Diagnostic[];
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
/**
 * Compiles a stylesheet in Merlion's CSS subset. Over 64 KiB of UTF-8 returns `E013`
 * without calling the module; a theme name the stylesheet does not define throws
 * `RangeError`; an unknown option throws `TypeError`.
 */
export function compileStylesheet(css: string, options?: StylesheetOptions): CompiledStylesheet;

/** @internal Exercises the trap-recovery path of a module built with `test-trap`. */
export function __trapForTests(): RenderResult;
