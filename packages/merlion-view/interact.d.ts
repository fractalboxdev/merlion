/**
 * `@fractalbox/merlion-view/interact`: click to highlight, collapse and hide, a detail popover
 * and keyboard traversal for Merlion flowcharts in `<merlion-view>` (specs/interaction.md).
 * Importing it registers the extension with `MerlionView.extend`. The base element loads it on
 * its own for any Merlion SVG unless `interactive="off"`.
 */
export declare function interact(host: HTMLElement, svg: SVGSVGElement): (() => void) | undefined;
