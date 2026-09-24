/**
 * Legibility of a drawing, measured in a browser (specs/benchmark.md#authoring).
 *
 * Text advance is the one quantity a model writing SVG cannot compute: it
 * depends on the font the reader's browser resolves, and nothing in the
 * generated text records it. So the measurement is taken where it is real —
 * Chromium through Playwright, already the benchmark's mermaid host — by
 * `probe.js`, which lays the SVG out and reads `getBBox()` off every `<text>`
 * and every painted shape.
 *
 * Three failures come out of that: a label wider or taller than its own shape,
 * two node shapes overlapping, and ink outside the `viewBox`, which the reader
 * never sees at all. All three are read the same way whoever drew the SVG, so
 * a Merlion drawing and a hand-written one are judged on one scale.
 */
import { Effect } from "effect";
import { readFileSync } from "node:fs";
import type { Page } from "playwright";

const PROBE = readFileSync(new URL("./probe.js", import.meta.url), "utf8");

export interface Legibility {
  /** Labels the page laid out; 0 means nothing was measurable. */
  readonly labels: number;
  /** Labels whose ink box leaves the shape they sit in. */
  readonly overflowing: number;
  /** Worst overflow as a fraction of its shape's width; 0 when nothing overflows. */
  readonly worstOverflow: number;
  /** Labels and shapes with ink outside the viewBox. */
  readonly clipped: number;
  /** Pairs of painted shapes intersecting by more than 1 px². */
  readonly shapeOverlaps: number;
  /** Labels touching no shape at all: a title, or an edge label drawn without a chip. */
  readonly unplaced: number;
}

export const EMPTY: Legibility = { labels: 0, overflowing: 0, worstOverflow: 0, clipped: 0, shapeOverlaps: 0, unplaced: 0 };

/** Injects the probe once; a page measures many drawings. */
export const preparePage = (page: Page): Effect.Effect<void, Error> =>
  Effect.tryPromise({
    try: () => page.addScriptTag({ content: PROBE }).then(() => undefined),
    catch: (e) => (e instanceof Error ? e : new Error(String(e))),
  });

export const measure = (page: Page, svg: string): Effect.Effect<Legibility, Error> =>
  Effect.tryPromise({
    try: () =>
      page.evaluate<Legibility, string>(
        (s) => (globalThis as unknown as { legibilityProbe: (svg: string) => Legibility }).legibilityProbe(s),
        svg,
      ),
    catch: (e) => (e instanceof Error ? e : new Error(String(e))),
  });
