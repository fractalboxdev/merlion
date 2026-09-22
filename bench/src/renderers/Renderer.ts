/** The renderer service every baseline and Merlion itself are measured through. */
import { Context, Duration, type Effect } from "effect";

export interface RenderOpts {
  /** Previous SVG of the same diagram, passed as a layout hint where the renderer supports one. */
  readonly hint?: string;
}

export interface RenderOutcome {
  readonly svg: string | null;
  readonly error: string | null;
  /** Render time in milliseconds, measured as close to the renderer as the adapter can. */
  readonly ms: number;
  readonly fuelUsed: number | null;
}

export interface RendererApi {
  readonly name: string;
  readonly version: string;
  /** Renders one diagram. Never fails: a failed render is an outcome with `error` set. */
  readonly render: (source: string, opts?: RenderOpts) => Effect.Effect<RenderOutcome>;
}

export class Renderer extends Context.Tag("bench/Renderer")<Renderer, RendererApi>() {}

export const RENDERER_NAMES = ["merlion", "mermaid-dagre", "mermaid-elk"] as const;
export type RendererName = (typeof RENDERER_NAMES)[number];

export const failed = (error: string, ms = 0): RenderOutcome => ({ svg: null, error, ms, fuelUsed: null });

/** Wall-clock limit for one diagram; a render past it is recorded as failed (the mermaid page is replaced). */
export const RENDER_TIMEOUT = Duration.seconds(30);
