# `<merlion-view>`

A custom element that adds pan, zoom and fullscreen to any SVG. It is optional: without JavaScript, or before the element upgrades, the SVG inside it renders as a normal static diagram.

```html
<merlion-view>
  <svg class="merlion merlion-flowchart" …>…</svg>
</merlion-view>
```

## Behaviour

| Input | Action |
|---|---|
| Ctrl/⌘ + wheel, trackpad pinch, touch pinch | Zoom around the pointer, 0.25×–8× |
| Plain wheel | Scrolls the page; the diagram doesn't capture it |
| Drag (pointer), one-finger drag when zoomed in | Pan |
| Double-click / double-tap | Reset to fit |
| `+` / `-` / `0` / arrow keys, when focused | Zoom in, zoom out, reset, pan by 10% |
| Fullscreen button | Moves the SVG element into a modal `<dialog>`, sized to the viewport, and moves it back when the dialog closes; `Esc` closes it. The SVG is moved, never cloned, so its ids stay unique on the page |

- Zooming applies a CSS `transform` to the SVG element and never re-rasterises, so text stays vector-sharp.
- The controls (zoom in, zoom out, reset, fullscreen) are `<button>` elements with `aria-label`s, shown on hover or focus and always shown on touch devices.
- The element is focusable (`tabindex="0"`) and announces the SVG's `<title>` as its accessible name.
- With `prefers-reduced-motion`, zoom and reset jump instead of animating.
- The element fills whatever height its container gives it and centres the SVG inside that box; the controls sit in the element's top-right corner. A container taller than the diagram therefore keeps the controls clear of the drawing.
- The element shows the controls only when the SVG's natural width exceeds its container, or with the `controls="always"` attribute.

## Semantic zoom

When the zoom scale falls below a threshold, labels would render below 9 px on screen. At that point, the viewer adds `merlion-zoomed-out` to the host and sets `data-merlion-rank-limit="{0..15}"` on it. CSS can't compare an attribute numerically against a variable in every supported browser, so `merlion-themes.css` ships one generated selector per pair of limit and rank (`[data-merlion-rank-limit="2"] [data-merlion-rank="3"]`, …; 120 selectors for ranks 0–15). Those rules hide the labels of nodes whose rank exceeds the limit and collapse clusters to their titles. The viewer only toggles a class and an attribute on the host; the SVG is never modified, and no request is made.

## Constraints

- No dependencies. Target size ≤ 6 KB minified and gzipped (enforced in CI). The semantic-zoom selectors live in `merlion-themes.css`, which has its own CI budget of ≤ 8 KB gzipped.
- Works in every browser that supports custom elements, pointer events and `<dialog>`.
- Independent of the renderer: it wraps any inline SVG, including mermaid's.
