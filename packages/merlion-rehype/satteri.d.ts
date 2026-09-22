import type { Options } from "./index.js";

/** A diagnostic from a diagram or the stylesheet, with its place in the Markdown file. */
export interface SatteriMessage {
  reason: string;
  /** `E0xx`/`W0xx`/`R0xx`, or `font-css`, `cache-dir`, `cache-write`, `merlion-strict`, `stylesheet`. */
  ruleId: string;
  /** Absolute path of the document; '' when Sätteri was given no `fileURL`. */
  file: string;
  /** 1-based; 0 without a location. */
  line: number;
  column: number;
  /** An error under `strict`: the document fails after this message. */
  fatal: boolean;
}

export interface SatteriOptions extends Options {
  /** Receives every message; default prints `file:line:col: reason` with console.warn. */
  onMessage?: (message: SatteriMessage) => void;
}

/** What Sätteri tells a plugin factory about the document (satteri's `PluginFactoryContext`). */
export interface SatteriFactoryContext {
  readonly fileURL: URL | undefined;
}

/** A Sätteri hast plugin definition (satteri's `HastPluginDefinition`), structurally. */
export interface SatteriHastPlugin {
  name: string;
  options: { position: true };
  element: { filter: ["pre"]; visit(node: unknown, ctx: unknown): Promise<void> };
  after(): void;
}

export type SatteriPluginFactory = (ctx?: SatteriFactoryContext) => SatteriHastPlugin;

/**
 * The rehype plugin as a Sätteri hast plugin factory, for `hastPlugins` (Astro 7's
 * default processor). One call is one build: the renderer loads and the stylesheet
 * compiles once. Figures, ids, cache and diagnostics match the rehype plugin; a fatal
 * message throws and fails the document.
 */
export default function merlionSatteri(options?: SatteriOptions): SatteriPluginFactory;
