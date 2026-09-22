import type { View } from "./zoom.js";

/**
 * `<merlion-view>`: pan, zoom and fullscreen for the inline SVG it contains.
 * Importing the module registers the element when `customElements` exists.
 *
 * Attributes: `controls="always"` shows the controls even when the SVG fits.
 * Set by the element: `tabindex`, `role`, `aria-label` (from the SVG `<title>`),
 * `zoomed`, the class `merlion-zoomed-out` and `data-merlion-rank-limit`.
 */
export declare class MerlionView extends HTMLElement {
  /** Current view. */
  readonly view: View;
  /** Reset to fit. */
  reset(): void;
}

declare global {
  interface HTMLElementTagNameMap {
    "merlion-view": MerlionView;
  }
}

export type { View };
