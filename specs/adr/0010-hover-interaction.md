# ADR-0010: Diagram interaction: click to pin in the viewer, hover as a preview

- **Status:** Proposed
- **Date:** 2026-09-22

## Context

Readers of a dense diagram want to pick a box and see what it connects to, fold away the parts they are not reading, and read or copy text that is too small or too clipped at the current zoom. Interactive diagrams in current documentation and chat tools do this: a click highlights a node and its neighbours, dims the rest and reveals detail, and the labels stay selectable text. Merlion's output is static SVG with no script, inlined without a sanitiser ([security.md](../security.md)), and `<merlion-view>` is an optional ≤ 6 KB enhancement that also wraps renderers other than Merlion ([viewer.md](../viewer.md)).

Constraints, measured in headless Chromium 153 (Playwright 1.63, Apple M3 Ultra) unless marked:

- **The SVG carries the graph.** `data-merlion-from` and `data-merlion-to` on every edge group give adjacency in O(V + E) with no new data. Nothing in the SVG today lets a CSS selector name one node or edge without source text: `data-merlion-id` values are arbitrary source ids, and source text never reaches the embedded style.
- **`:has()` with `:hover` works in an inline SVG's embedded style,** scoped under `#{id}`: on the 28-element `two-tier-labels` fixture, per-element rules lit exactly the hovered node's 5 edges and 5 neighbours. It needs one rule per node and per edge, because CSS cannot compare one element's attribute with another's.
- **Its cost grows faster than linearly.** Rules cost about 125 raw bytes (17 gzip bytes) per node or edge; style recalculation per hover change was 0.44 ms at V + E = 80, 1.74 ms at 160, 11.9 ms at 640 and 42.8 ms at 1,250. A class-toggling script over the same graphs stayed between 0.025 and 0.045 ms.
- **CSS reaches fewer contexts than it seems.** It does nothing in `<img>` (GitHub READMEs, most standalone embeds) and nothing under a page CSP whose `style-src` lacks `'unsafe-inline'`, which blocks the embedded `<style>`; constructed style sheets through `adoptedStyleSheets` still apply there. Firefox and WebKit are unmeasured.
- **Edges are thin.** A point 1 px off a 1.25 px path misses it, so hover on an edge needs a hit area wider than its stroke; SVG has no hit-only stroke width.
- **Keyboard and screen readers.** `<g tabindex="0">` is a Tab stop in Chromium, so per-node tab stops are possible but put every node in the page's Tab order. Children of `role="img"` are presentational in WAI-ARIA, so the SVG's own nodes are not a dependable target for `aria-activedescendant`.
- **Tones must survive highlighting** ([ADR-0009](0009-stylesheet.md)). Setting `--merlion-node-border` on a highlighted node recolours only untoned nodes, measured: a `danger` node, a `classDef` stroke and a `linkStyle` stroke keep their colour.
- **Hover alone fails the reading task.** A hover highlight vanishes the moment the pointer moves to read or copy the text it revealed, does not exist on touch, and competes with text selection for the same pointer. A pinned highlight survives all three.
- **Text selection and panning share one gesture.** Before this decision every mouse drag panned and suppressed selection. A drag has to mean one thing per starting point: text selects, background pans (only when there is anything to reveal).
- **The viewer budget is tight.** The base element was 3.36 KB gzip of its 6 KB; the full behaviour below (pin, path mode, collapse, hide, popover, keyboard) measured 4.3 KB as one module.

## Decision

Two layers, specified in [interaction.md](../interaction.md):

1. **CSS layer, in the SVG.** Every node and edge group gets an id, `{id}-n{k}` (declaration index) and `{id}-e{k}` (link index). When the diagram draws at most 128 nodes plus edges and the render option `hover` is `"css"` (the default), the embedded style adds one dim rule, one thickening rule, and one `#{id}:has(#{id}-n{k}:hover) :is(…){opacity:1}` rule per node and per edge. Selectors hold only core-generated ids and fixed class names; `#{id}:has(` joins the allowed rule prefixes. Larger diagrams get the ids and `I034 HoverRulesOmitted`.
2. **Viewer layer, in `@fractalbox/merlion-view/interact`: click is the primary interaction.** A tap pins a highlight (Shift+tap: the transitive path), a tap on a cluster title collapses it, Alt+tap hides a node, and hover only thickens the pointed node's border. A drag on label text always selects it; a drag pans only from the background of a drawing larger than its box; a click that ends a drag or a selection, or is a double-click's second click, commands nothing. The module computes everything from `data-merlion-*` attributes and label text, toggles classes, and adds a popover and keyboard traversal in outline order with a polite live region. It has its own ≤ 3 KB gzip budget and attaches through an extension hook; the base element loads it on demand for every Merlion SVG unless `interactive="off"`, and owns the chrome it shares (popover, live region, tap qualification, the **Show all** control and the light-DOM sheet).

## Options considered

| Option | Buys | Costs |
|---|---|---|
| CSS only, in the SVG | Works inline with JavaScript off and in SVG opened as a document; no module to load | Per-element rules: 16 KB at 128 elements, 158 KB at 1,250; recalculation 42.8 ms per hover at 1,250; no popover, keyboard, pinning or wide edge hit area; inert in `<img>` and under a strict CSP |
| Viewer JavaScript only | Flat hover cost (≤ 0.045 ms at 1,250 elements); full behaviour; works under a strict CSP; SVG unchanged except ids | Nothing without JavaScript or before the module loads; every host must ship the module |
| Script inside the SVG | Self-contained behaviour wherever the SVG is opened as a document | Breaks the guarantee that the SVG inlines without a sanitiser: sanitisers strip it, CSPs block it, and a script in untrusted diagram output is the exact thing [security.md](../security.md) forbids. Rejected on safety alone |
| Native `<title>` per node and edge | Browser tooltips with no script or CSS | Duplicates every label in the bytes; fixed browser delay and placement; changes accessible names; no highlight |
| **Both layers: CSS capped at N = 128, viewer module for the rest** | Hover works with JavaScript off on 100% of `compat` flowcharts (largest: 88 elements); full behaviour and flat cost where the viewer runs; strict-CSP pages still get the viewer layer | Two implementations of one highlight set, held together by an acceptance test; per-diagram bytes for the rules; a larger allow-list of rule prefixes |

Within the chosen design:

| Option | Buys | Costs |
|---|---|---|
| **Ids `{id}-n{k}` / `{id}-e{k}`** | Short selectors (13 bytes for `#m8c6811a0-n3`); the index gives the outline's order for keyboard traversal; fragment-addressable | Inserting a node renumbers later ids, adding lines to committed-SVG diffs that stable layout otherwise keeps unchanged |
| Ids from the encoded source id (`{id}-n-{enc}`) | Stable under insertion | Longer selectors, and edges need a composite key; order still needs its own attribute, which renumbers the same way |
| Data attributes (`data-merlion-index`) | Matches the `data-merlion-*` naming | Longer selectors than ids, and not usable as idrefs or fragments |
| **Viewer-managed focus with a live region** | One tab stop; works across the shadow boundary; no SVG changes | Screen-reader users hear each node through announcements rather than a navigable tree |
| Hover as the primary viewer interaction (pin on click as a secondary state) | Zero-click discovery on desktop | Highlight and popover chase the pointer, so reading and copying the revealed text fights the highlight; no touch equivalent; every pointer move costs a style change |
| **Click to pin, hover as a preview** | Stable state to read and select from; one model for mouse, touch and keyboard; hover work is one CSS rule | One click before anything dims; discovery relies on the pointer cursor and the hover preview |
| Interaction module opt-in (`import …/interact`) | Pages pay nothing they did not ask for | Every integration must remember the import; the documentation site ships without it by default |
| **Base loads the module on first Merlion SVG** | Interactive by default everywhere `<merlion-view>` runs, including the docs site, with no integration change; mermaid-only pages never fetch it | One dynamic import per page; a host that copies only `merlion-view.js` silently gets pan and zoom only |
| Popover, live region and controls inside the interaction module | Base stays smallest (4.0 KB) | Module measured 4.3 KB, over its 3 KB budget; the popover must still know about the base's fullscreen dialog |
| **Viewer chrome in the base, behaviour in the module** | Base 4.9 KB, module 3.0 KB, both within budget; the base already owns the dialog and view changes the popover follows | About 0.9 KB more for pages that wrap non-Merlion SVGs |
| `tabindex` on every node group | Native focus and focus ring | One Tab stop per node; 13 bytes per node; descendants of `role="img"` are presentational |

## Criteria

Host safety (no script, no source text in CSS, no sanitiser needed), reach without JavaScript, bytes per diagram, hover cost at the core's size limits, keyboard and screen-reader access, and keeping the viewer within its budget. Safety removes script in the SVG. Hover cost removes CSS-only as the whole answer: above a few hundred elements it drops frames, and it cannot do keyboard or popovers at any size. Reach without JavaScript is the one criterion the viewer alone fails, and it decided adding the CSS layer; bytes and recalculation cost set its cap at 128, which covers every `compat` flowchart and bounds the rules at the same 16 KB as a palette's embedded style. Reading and copying text decided click over hover as the primary interaction. The viewer budget decided the separate module and the split of chrome into the base.

## Consequences

- Easier: tapping any node or edge pins its neighbourhood and full text, which stay put while the reader selects and copies label text; clusters fold away; keyboard users walk the diagram in outline order and hear each node's outline line; hover works on inline diagrams with JavaScript off; the highlight respects every tone and source colour by construction.
- Harder: every SVG's bytes change once, when the ids land (about 19 bytes per element) and again for diagrams under the cap (about +20% raw, +13% gzip on the `two-tier-labels` fixture). The fuzz target and the scoping check accept a fourth prefix, `#{id}:has(`, and check its arguments. The CSS and viewer layers must compute identical sets, enforced over `compat`. With the module loaded, plain arrow keys traverse nodes and panning moves to `Shift` + arrows. A mouse drag on a node shape or label no longer pans; zoomed-in readers pan from the background. A tapped node stays in `:hover` on touch devices without the viewer. Node ids shift when a node is inserted above them.
- Expensive to reverse: the id scheme `{id}-n{k}` / `{id}-e{k}` once pages link to fragments or style them; the tokens `--merlion-highlight`, `--merlion-highlight-stroke` and `--merlion-dim-opacity`; the `hover` render option; the base element's extension API (`extend`, `style`, `tip`, `say`, `tap`); the gesture and key bindings.
- Uncertainty: every browser measurement is Chromium 153 only; Firefox and WebKit behaviour of `:has()` with `:hover` in inline SVG, sticky hover on touch, and recalculation cost are unmeasured. Recalculation times are from one desktop machine; the frame-budget argument assumes a phone 4–5× slower. Module sizes are esbuild 0.28 minified output gzipped at level 9.
