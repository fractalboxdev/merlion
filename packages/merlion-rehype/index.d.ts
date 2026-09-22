import type { Root } from "hast";
import type { VFile } from "vfile";

/** A core diagnostic as returned by @fractalboxdev/merlion-wasm (specs/parser.md#diagnostics). */
export interface Diagnostic {
  severity: "error" | "warning" | "repair" | "info";
  code: string;
  span?: { line: number; column: number; byte_start?: number; byte_end?: number } | null;
  message: string;
  fix?: { span: unknown; replacement: string } | null;
}

export interface RenderResult {
  svg: string | null;
  outline: string | null;
  diagnostics: Diagnostic[];
}

/** Options the plugin passes to `render` (names follow the core's `RenderOptions`). */
export interface PluginRenderOptions {
  target_width: number;
  strict: boolean;
  id_prefix: string;
  /** The previous SVG for this block, as the layout hint. */
  hint?: string;
}

export type Render = (source: string, options: PluginRenderOptions) => RenderResult | Promise<RenderResult>;

export interface OutlineInfo {
  /** File path relative to the project root, `/`-separated. */
  path: string;
  /** 1-based block index within the file. */
  index: number;
  source: string;
  outline: string | null;
}

export interface Options {
  /** Container width in px (`RenderOptions.target_width`). Default 720. */
  width?: number;
  /** Promote warnings to errors and fail the file on any diagram error. Default false. */
  strict?: boolean;
  /** Keep the Mermaid source in a collapsed `<details>`. Default "details". */
  source?: "details" | "none";
  /** Directory, inside `root`, holding previous renders used as layout hints. Default: no cache. */
  cacheDir?: string;
  /** Wrap each SVG in `<merlion-view>`. Default true. */
  viewer?: boolean;
  /** The page loads merlion-font.css; silences the font warning. Default false. */
  fontCss?: boolean;
  /** Project root for relative paths and `cacheDir`. Default: the file's `cwd`. */
  root?: string;
  /** Renderer; default: @fractalboxdev/merlion-wasm via `initSync`. */
  render?: Render;
  /** Receives each rendered diagram's plain-text outline. */
  outline?: (info: OutlineInfo) => void;
}

/** Replaces `pre > code.language-mermaid` with a figure holding the inline SVG. Async: use `process`, not `processSync`. */
declare function rehypeMerlion(options?: Options): (tree: Root, file: VFile) => Promise<void>;
export default rehypeMerlion;

/** Caption text: `accTitle`, else the front-matter `title`, else null. */
export declare function diagramTitle(source: string): string | null;
/** FNV-1a 64 of the UTF-8 bytes, 16 lowercase hex digits. */
export declare function fnv1a64(str: string): string;
/** `m` + first 8 hex of FNV-1a 64 over `relPath + "-" + n`. */
export declare function idPrefix(relPath: string, n: number): string;
/** Cache entry name for block `n` of `relPath`. */
export declare function cacheKey(relPath: string, n: number): string;
