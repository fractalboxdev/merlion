# `<merlion-view>`

A custom element that adds pan, zoom and fullscreen to any SVG, and click-driven interaction to Merlion diagrams ([Interaction](#interaction)). It is optional: without JavaScript, or before the element upgrades, the SVG inside it renders as a normal static diagram.

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
| Drag from the background when the drawing is larger than the box; one-finger drag when zoomed in | Pan ([Gestures](#gestures)) |
| Drag on label text | Select text |
| Double-click / double-tap on the background | Reset to fit |
| `+` / `-` / `0` when focused | Zoom in, zoom out, reset |
| Arrow keys when focused | Pan by 10%; with the interaction module, `Shift` + arrows pan and plain arrows move between nodes |
| Fullscreen button | Moves the SVG element into a modal `<dialog>`, sized to the viewport, and moves it back when the dialog closes; `Esc` closes it. The SVG is moved, never cloned, so its ids stay unique on the page |

- Zooming applies a CSS `transform` to the SVG element and never re-rasterises, so text stays vector-sharp.
- The controls (zoom in, zoom out, reset, fullscreen) are `<button>` elements with `aria-label`s, shown on hover or focus and always shown on touch devices.
- The element is focusable (`tabindex="0"`) and announces the SVG's `<title>` as its accessible name.
- With `prefers-reduced-motion`, zoom and reset jump instead of animating.
- The element fills whatever height its container gives it and centres the SVG inside that box; the controls sit in a 38 px strip the element reserves above the drawing (except with `controls="never"`), so they never cover it.
- Every diagram gets the controls: drawn at 40% opacity at rest and fully on hover or focus, and always fully on touch devices. `controls="always"` keeps them fully visible; `controls="never"` removes them.
- Panning is clamped: a drawing smaller than the visible box stays entirely inside it, and a drawing larger than the box (zoomed in) always covers it, so no drag or zoom pushes content out of sight.

## Gestures

Label text is ordinary selectable text. No rule in the viewer, `merlion-themes.css` or the SVG's embedded style sets `user-select: none`.

What a primary pointer-down is over decides what its drag does. **Text** is any `<text>` or `<foreignObject>`; a **shape** is a Merlion node or edge group (or mermaid's `.node`, `.edgePath`, `.edgeLabel`); anything else, cluster boxes included, is **background**.

| Pointer-down on | Drawing fits the box | Drawing larger than the box |
|---|---|---|
| Text, mouse or pen | Native text selection | Native text selection; never pans |
| Shape, mouse or pen | Native behaviour; never pans | Native behaviour; never pans |
| Background, mouse or pen | Native behaviour; never pans | Pans; the default action (selection) is prevented and the host takes focus |
| Anywhere, one finger | The page scrolls | Pans once zoomed in (`touch-action: none`) |

- Only a pan prevents the default action and captures the pointer, so a drag that starts on text always selects it, zoomed in or not.
- Shift+pointer-down on a shape prevents the default action, so Shift+click never extends a text selection (the interaction module uses Shift+click for path mode).
- A double-click resets the zoom only on the background and only when it leaves no text selected: double-clicking a word selects the word and keeps the zoom.
- A **tap** is a click on the drawing, not on a viewer control, whose pointer moved less than 4 CSS px since it went down, that leaves no text selected, and that is not a double-click's second click. Only taps command the diagram ([interaction.md](interaction.md#gestures)).
- Cursors: `text` over label text (from the base element's light-DOM sheet, so it holds over a pannable drawing too); `grab` on the host while the drawing is larger than the box (host attribute `pannable`); `pointer` over nodes, edges and cluster titles once the interaction module is active.

## Interaction

`<merlion-view>` loads `@fractalboxdev/merlion-view/interact` with a dynamic `import()` the first time it adopts an SVG with class `merlion`, so every Merlion diagram is interactive by default: in the demo, in the documentation site and in any page that uses the element, with no integration change. `interactive="off"` on an element runs no extension on it and, if it is the only one, loads nothing. Pages whose diagrams are all non-Merlion SVGs never fetch the module. The behaviour is specified in [interaction.md](interaction.md).

### Extension hook

| API | Contract |
|---|---|
| `MerlionView.extend(fn)` | `fn(host, svg)` runs whenever a host adopts an SVG, and again for hosts already on the page when `fn` registers late. It returns a cleanup function, called when the SVG changes, the host disconnects or `interactive` becomes `"off"`; a `view` method on it runs after every view change |
| `MerlionView.style(css)` | Appends rules to one constructed stylesheet that every host adopts into its root (`document` or the enclosing shadow root), since the shadow style cannot reach the slotted SVG. Adopted sheets apply under a CSP without `'unsafe-inline'` |
| `host.tip(el, build)` | Shows the `part="tooltip"` popover next to `el`, filled by `build(popover)`; `tip(null)` hides it. The host places it ([interaction.md](interaction.md#placement)), moves it into the fullscreen dialog while that is open, and places it again after every view change and at the end of a zoom animation |
| `host.say(text)` | Writes `text` to a visually hidden `role="status"`, `aria-live="polite"` region in the shadow root |
| `host.tap(e)` | True when click `e` is a tap ([Gestures](#gestures)) |
| **Show all** control | A controls-bar button shown while the host has `data-merlion-hidden`; clicking it dispatches `merlion-show-all` on the host |
| Keys | The base skips a `keydown` whose default an extension prevented |

## Semantic zoom

When the user zooms out past the fit view and labels would render below 9 px on screen, the viewer adds `merlion-zoomed-out` to the host and sets `data-merlion-rank-limit="{0..15}"` on it. CSS can't compare an attribute numerically against a variable in every supported browser, so `merlion-themes.css` ships one generated selector per pair of limit and rank (`[data-merlion-rank-limit="2"] [data-merlion-rank="3"]`, …; 120 selectors for ranks 0–15). Those rules hide the labels of nodes whose rank exceeds the limit. Semantic zoom hides labels only: node shapes, edges and cluster members stay drawn at every zoom level. The fit view and every zoom-in keep every label, even when a wide diagram fitted to its container draws them below 9 px, so a diagram never loses content before the reader interacts with it. The viewer only toggles a class and an attribute on the host; the SVG is never modified, and no request is made.

## Constraints

- No dependencies. The base element ≤ 6 KB minified and gzipped (4,953 B), the interaction module ≤ 3.25 KB (3,237 B) and its sequence module ≤ 1 KB (986 B), each bundled on its own and enforced by `scripts/size.mjs`. The semantic-zoom selectors live in `merlion-themes.css`, which has its own CI budget of ≤ 8 KB gzipped.
- Three modules, each fetched only where it does something. The base loads `interact.js` for an SVG with class `merlion`; `interact.js` loads `interact-seq.js` for one that also carries `merlion-sequence`. Sequence support is therefore free on a page of flowcharts, and the interaction module carries the extension points — the keyboard's walk, a target's outline line, the element list a lit node lights, and a lit set supplied for a cluster — rather than the sequence rules themselves ([interaction.md](interaction.md#loading)).
- The interaction module's budget is 3.25 KB rather than 3 KB because those extension points and the `import()` that reaches them cost 214 bytes gzipped, of which the `import()` statement alone is 48. Moving the collapse rules out to buy the difference back would make the first collapse gesture wait on a network fetch, so the number moves instead of the behaviour.
- Works in every browser that supports custom elements, pointer events and `<dialog>`.
- Independent of the renderer: it wraps any inline SVG, including mermaid's.
