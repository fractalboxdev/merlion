import type { View } from "./zoom.js";

/**
 * `<merlion-view>`: pan, zoom and fullscreen for the inline SVG it contains.
 * Importing the module registers the element when `customElements` exists.
 *
 * Attributes: `controls="always"` keeps the controls fully visible, `controls="never"` removes
 * them; `interactive="off"` runs no extension and skips loading the interaction module.
 * Set by the element: `tabindex`, `role`, `aria-label` (from the SVG `<title>`), `zoomed`,
 * `pannable`, the class `merlion-zoomed-out` and `data-merlion-rank-limit`.
 */
export declare class MerlionView extends HTMLElement {
  /** Current view. */
  readonly view: View;
  /** Reset to fit. */
  reset(): void;
  /** Popover next to `el` (an element of the drawing), filled by `build`; `null` hides it. */
  tip(el: Element | null, build?: ((popover: HTMLElement) => void) | null): void;
  /** Read `text` through the polite live region. */
  say(text: string): void;
  /** True when click `e` is a tap on the drawing (no drag, no selection, not a double-click's second click). */
  tap(e: MouseEvent): boolean;
  /**
   * Extension hook: `fn(host, svg)` runs whenever a host adopts an SVG and returns a cleanup
   * function, called when the SVG changes or the host disconnects; its optional `view` method
   * runs after every view change.
   */
  static extend(fn: (host: MerlionView, svg: SVGSVGElement) => ((() => void) & { view?: () => void }) | undefined): void;
  /** Add rules for the SVG inside every host (adopted by each host's root). */
  static style(css: string): void;
}

declare global {
  interface HTMLElementTagNameMap {
    "merlion-view": MerlionView;
  }
}

export type { View };
