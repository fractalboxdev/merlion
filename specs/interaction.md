# Hover interaction

Pointing at a node highlights it, the edges that touch it and the nodes at their other ends, dims everything else, and shows the node's full text in a popover. Pointing at an edge highlights the edge, its label and both endpoints. Interaction is presentation only: it never changes layout or re-renders, and interaction state never reaches the SVG's bytes ([Determinism](#determinism)).

Two layers deliver it ([ADR-0010](adr/0010-hover-interaction.md)):

| Layer | Where it lives | What it does |
|---|---|---|
| **CSS layer** | Rules in the SVG's embedded `<style>`, for diagrams of at most 128 nodes plus edges | Dims the rest and keeps the hovered element's neighbourhood at full opacity; thickens the hovered shape or path. No popover, no keyboard, no pinning |
| **Viewer layer** | `@fractalboxdev/merlion-view/interact`, an optional module for `<merlion-view>` | Highlight with stroke width and a highlight colour on the primary node, upstream/downstream paths, a popover, pinning, keyboard traversal, screen-reader announcements, touch, a widened hit area for edges |

Both layers compute the same highlight set for a plain hover; the acceptance tests hold them to it ([Testing](#testing)). This spec extends [svg-output.md](svg-output.md) (ids, embedded rules, the allowed rule prefixes) and [viewer.md](viewer.md) (keys, extension hook).

## Highlight set

| Target | Lit set |
|---|---|
| Node `n` | `n`; every edge with `n` as `data-merlion-from` or `data-merlion-to`; every node at the other end of those edges |
| Edge `e` | `e`, including its label; the nodes named by its `data-merlion-from` and `data-merlion-to` |
| Node `n`, path mode | `n`; every node reachable from `n` along edge direction (downstream) and every node that reaches `n` (upstream); every edge traversed by either search |
| Edge `e`, path mode | `e`; the upstream set of its `from` node and the downstream set of its `to` node, with their edges |

- Edge direction is the source direction (`data-merlion-from` → `data-merlion-to`), whatever `data-merlion-back` says about the drawn direction. Undirected links (`---`) are followed both ways in path mode.
- A self-loop is an incident edge of its node; parallel edges between the same pair are all incident.
- Nodes and edges outside the lit set are dimmed. Clusters are never dimmed: a cluster group contains its members, so group opacity would dim them too, and the box and title are low-contrast already.
- Invisible links (`~~~`) are not drawn and take no part.

## SVG additions

Every render, whatever the options:

| Element | Addition | Value |
|---|---|---|
| Node group `.merlion-node` | `id` | `{id}-n{k}`, `k` = the node's 0-based declaration index: the order in which the parser first meets the node id, which is the order of the outline's lines ([svg-output.md](svg-output.md#text-alternative)) |
| Edge group `.merlion-edge` | `id` | `{id}-e{k}`, `k` = the link's 0-based source index, the same number `linkStyle` uses; indices of invisible links are skipped, so the sequence can have gaps |
| Label line `<tspan>` that continues a word broken at the wrap width | `data-merlion-join` | `"true"`; joining it to the previous line with no space rebuilds the source word ([Text reconstruction](#text-reconstruction)) |

- The ids follow the internal-id rule (`{id}-…`) and use only `[a-z0-9-]` after the prefix, so they are safe in CSS selectors and in fragment URLs (`page.html#m3f2a91c0-n4` scrolls to the node).
- `k` depends on the source only, never on layout or the layout hint. Inserting a node renumbers the ids of the nodes declared after it; stable layout still keeps their coordinates ([ADR-0010](adr/0010-hover-interaction.md#consequences)).
- No `tabindex`, `<title>` per element, or per-node outline text is added. Keyboard focus is managed by the viewer ([Keyboard](#keyboard-and-screen-readers)), and the popover and announcement text is rebuilt from what the SVG already carries.
- Cost: about 19 bytes per node and per edge (530 bytes on the 13-node, 15-edge `two-tier-labels` fixture, 2.7% of its 19,580 bytes).

### Text reconstruction

The viewer layer rebuilds each element's text from the SVG alone:

- A label's lines are its `<tspan>` elements that carry a `y` attribute; a `<tspan>` without `y` is a run appended to the current line. Whitespace inside a line collapses to single spaces.
- A node's **title** is its lines before the first `.merlion-detail` line; the `.merlion-detail` lines are its **detail**. A node without detail lines has only a title.
- A node's **name** is all its lines joined by one space, except that a line with `data-merlion-join="true"` joins with none. This equals `plain_label` of the source label, the name the outline prints.
- A node's **cluster path** is the `.merlion-cluster-title` text of each enclosing `.merlion-cluster`, outermost first, joined by ` / `, as in the outline's headings.
- An edge's **glyph** is `→`, `←`, `↔` or `—` from the presence of `marker-end` and `marker-start` on its `.merlion-edge-path`, the same mapping as the outline. Its **label** is the text of its `.merlion-edge-text`, if any.

## CSS layer

### Rules

Emitted after all theming rules when the render option `hover` is `"css"` (the default) and the diagram draws at most **N = 128** nodes plus edges. Above N, the rules are left out with `I034 HoverRulesOmitted` (Info), and the ids are still emitted.

```css
/* 1. dim everything while any node or edge is hovered */
#{id}:has(:is(.merlion-node,.merlion-edge):hover) :is(.merlion-node,.merlion-edge){opacity:var(--merlion-dim-opacity, 0.3);}
/* 2. thicken the hovered shape or path */
#{id} .merlion-node:hover>.merlion-shape,#{id} .merlion-edge:hover>.merlion-edge-path{stroke-width:calc(var(--merlion-stroke, 1.25px) * 2);}
/* 3. one rule per node: itself, its incident edges, its neighbours */
#{id}:has(#{id}-n4:hover) :is(#{id}-n4,#{id}-e3,#{id}-e7,#{id}-n3,#{id}-n9){opacity:1;}
/* 4. one rule per edge: itself and its endpoints */
#{id}:has(#{id}-e7:hover) :is(#{id}-e7,#{id}-n4,#{id}-n9){opacity:1;}
```

- Rule 1 has specificity (1,3,0); rules 3 and 4 have (3,1,0) or more, so the lit set wins over the dim.
- Rule 3 lists the node itself, then its incident edges, then its neighbours; rule 4 lists the edge, then its endpoints. Each group is in ascending index order and names each id once, so the rules are a pure function of the graph.
- The rules change opacity and stroke width only, never a colour, so every tone, role, `classDef`, `style` and `linkStyle` colour shows unchanged when lit.
- Every selector names only `#{id}`, `#{id}-n{k}`, `#{id}-e{k}`, `.merlion-node`, `.merlion-edge`, `.merlion-shape`, `.merlion-edge-path` and `:hover`. No source text, no attribute selector, no host-page element.
- No transition is emitted: an animated dim would need `@media (prefers-reduced-motion: no-preference)`, a fourth at-rule in the allow-list, for a cosmetic gain.

### Where it works

Measured in headless Chromium 153 (Playwright 1.63, Apple M3 Ultra) on the `two-tier-labels` fixture with the rules above:

| Context | Result |
|---|---|
| SVG inlined in HTML, no JavaScript | Hovering `q` lights exactly its 5 incident edges and 5 neighbours (11 elements at opacity 1, 17 dimmed); hovering the `max retries` label lights 3; moving off restores all 28 to 1 |
| SVG opened as a document | Same: 17 dimmed |
| SVG in `<img>` | No effect: screenshots before and after hovering are identical. Images are not interactive; GitHub READMEs embed SVG this way |
| Page CSP `style-src 'self'` (no `'unsafe-inline'`) | No effect: the inline SVG's embedded `<style>` is blocked (0 dimmed), and theming falls back to presentation attributes as it already does under such a CSP |
| Touch tap (Chromium touch emulation) | The tapped node still matches `:hover` after the finger lifts, and its neighbourhood stays lit (17 dimmed) |
| Browser without `:has()` | Rules 1, 3 and 4 are dropped as invalid, rule 2 applies: consistent, only the thickening shows |

Firefox and WebKit are unmeasured: the installed Playwright 1.63 has no matching Firefox or WebKit build. `:has()` shipped in Safari 15.4 and Firefox 121, by their release notes.

Limits of the CSS layer, all covered by the viewer layer: an edge is hoverable only on its painted stroke and its label (a point 1 px off a 1.25 px path hits the cluster box behind it, measured), there is no popover, no keyboard access and no pinning.

### Choice of N

Style recalculation per hover change and added bytes, measured on synthetic flowcharts (E = 1.5 V, rendered by `merlion render`); recalculation is the Chrome DevTools Protocol `RecalcStyleDuration` averaged over up to 240 pointer moves, and the viewer-layer column comes from a class-toggling prototype of the rules in [Rendering the highlight](#rendering-the-highlight):

| V + E | Hover CSS, raw | SVG gzip growth (ids + rules) | Recalc per move, CSS layer | Recalc per move, viewer layer |
|---|---|---|---|---|
| 40 | 5.0 KB | +0.70 KB | 0.20 ms | — |
| 80 | 9.9 KB | +1.38 KB | 0.44 ms | — |
| 160 | 19.7 KB | +2.76 KB | 1.74 ms | 0.025 ms |
| 320 | 39.8 KB | +5.77 KB | 3.41 ms | 0.032 ms |
| 640 | 80.6 KB | +11.9 KB | 11.9 ms | 0.036 ms |
| 1,250 | 158 KB | +22.3 KB | 42.8 ms | 0.045 ms |

Rules cost about 125 raw bytes and 17 gzip bytes per element, linear in V + E; recalculation grows faster than linearly, because every `:has()` rule is re-matched when `:hover` moves. N = 128 bounds the rules at about 16 KB, the same cap as a palette's embedded style ([architecture.md](architecture.md#boundaries)), and recalculation near 1.2 ms on this machine (interpolated), which leaves a 16.7 ms frame intact on a phone 4–5× slower. It covers every `compat` flowchart: the largest has V + E = 88 and the 99th percentile is 28 nodes and 40 edges. The viewer layer toggles classes on the lit set and its cost stays flat, so large diagrams rely on it.

## Viewer layer

### Loading

```js
import "@fractalboxdev/merlion-view/interact"; // registers <merlion-view> and the extension
```

- `@fractalboxdev/merlion-view` gains one extension point, `MerlionView.extend(fn)`, where `fn(host, svg)` runs whenever a host adopts an SVG and returns a cleanup function, called when the SVG changes or the host disconnects. It also tells extensions about view changes (zoom, pan, fullscreen) with a `view` callback. The hook adds at most 200 bytes gzip to the base element.
- The extension activates only for an SVG with class `merlion` whose `.merlion-edge` groups carry `data-merlion-from` and `data-merlion-to`. Any other SVG (mermaid's, for example) keeps pan and zoom only.
- `<merlion-view interactive="off">` opts one element out.
- Adjacency is built lazily on the first `pointerenter` or `focus`: one pass over the node groups into a `Map` from `data-merlion-id` to the group, one pass over the edge groups into per-node incidence lists. O(V + E); 0.4 ms for 500 nodes and 750 edges, measured.

### Pointer

- One delegated `pointerover`/`pointerout` pair on the host resolves the target with `closest(".merlion-node, .merlion-edge")`. Pointer events with `pointerType: "touch"` never hover ([Touch](#touch)).
- **Edge hit area.** When the pointer is over no node or edge, a `pointermove` handler (coalesced to one per animation frame) finds the nearest edge within 6 CSS px. On adoption, each `.merlion-edge-path` is sampled with `getPointAtLength` every 8 viewBox units (spacing doubled until the total is at most 20,000 samples) into a uniform grid of 32-unit cells; a query converts the pointer through `getScreenCTM().inverse()`, divides the 6 px tolerance by the current scale, and scans the 3 × 3 cells around it.
- Holding **Shift** switches the current hover to path mode; releasing it switches back.
- Click on a node or edge (movement under 4 px, so a drag still pans) pins it; click on the pinned element or on the diagram background clears. A node wrapped in `<a href>` keeps its native click: the link is followed, nothing is pinned.

### States

| State | Lit set shown | Popover |
|---|---|---|
| Idle | None; nothing dimmed | Hidden |
| Hover | The hovered element's | The hovered element's, after 150 ms over the same element |
| Pinned | The pinned element's | The pinned element's |
| Pinned, hovering another element | The hovered element's; leaving it returns to the pinned one | The hovered element's |

**Esc** clears the pin; with nothing pinned it falls through to the base viewer, which closes fullscreen. Hover never announces to screen readers; pinning and keyboard moves do.

### Rendering the highlight

The extension changes only `class` tokens on SVG elements and attributes on the host, and removes every one of them when it returns to idle, so `svg.outerHTML` is identical before hovering and after clearing:

| Where | Token | Meaning |
|---|---|---|
| Host | `data-merlion-state="hover"` / `"pinned"` | Something is lit |
| Host | `data-merlion-path` | Path mode |
| Node and edge groups in the lit set | class `merlion-lit` | Lit |
| The hovered or pinned element | class `merlion-primary` | The element the popover describes |
| The keyboard's current node | class `merlion-active` | Focus ring |

The styles live in one constructed `CSSStyleSheet`, added once to the host's root node (`document` or the enclosing shadow root) through `adoptedStyleSheets`. Adopted sheets cascade after the document's own sheets (measured: at equal specificity the adopted rule wins), and they apply under a CSP without `'unsafe-inline'` (measured):

```css
merlion-view[data-merlion-state] :is(.merlion-node,.merlion-edge){opacity:var(--merlion-dim-opacity,.3)!important}
merlion-view[data-merlion-state] .merlion-lit{opacity:1!important;--merlion-stroke:var(--merlion-highlight-stroke,2.5px)}
merlion-view[data-merlion-state] .merlion-node.merlion-primary{--merlion-node-border:var(--merlion-highlight,var(--merlion-accent,#0969da))}
merlion-view:focus-visible .merlion-active{outline:2px solid var(--merlion-highlight,var(--merlion-accent,#0969da));outline-offset:3px}
```

- `!important` on opacity makes the viewer layer override the CSS layer's `#{id}` rules when the two disagree (path mode, pinned state). Nothing else needs it: the other declarations set tokens that the embedded rules already read.
- **Tones survive.** The highlight colour sets `--merlion-node-border`, which only an untoned node's border reads; a tone, a role or a source colour sits earlier in the fallback chain. Measured with `--merlion-highlight: #ff8800` set this way on every node, and `--merlion-edge` on every edge: untoned nodes and edges turn `rgb(255, 136, 0)`; a `danger` node and a `failure` edge stay `rgb(207, 34, 46)`, a `classDef` stroke stays `#ff00ff`, a `linkStyle` stroke stays `#00ff00`; every stroke width doubles to 2.5 px, `thick` edges to 5 px through their existing `calc`.
- Edges are emphasised by width and full opacity, never by colour: an arrowhead inherits from its `<marker>` in `<defs>`, not from the edge ([Roles](svg-output.md#roles)), so a recoloured path would end in an arrowhead of the old colour.
- An outline on a focused SVG `<g>` paints in Chromium (measured).
- With `prefers-reduced-motion: no-preference`, opacity and the popover fade over 120 ms; with `reduce`, they switch instantly.

## Detail popover

### Content

Built from [Text reconstruction](#text-reconstruction), top to bottom:

| Part | Node | Edge |
|---|---|---|
| Heading | Title lines; bold, italic and code runs (`.merlion-b`, `.merlion-i`, `.merlion-code`) become `<b>`, `<i>`, `<code>` | `{from name} {glyph} {to name}` |
| Body | Detail lines, one per line | The edge label, if any |
| Context | Cluster path, if any | — |
| Outgoing | One row per outgoing edge: `{glyph} {target name}` and the label in brackets | — |
| Incoming | One row per incoming edge: `← {source name}` and the label in brackets | — |

- Each list shows at most 8 rows, then `and {n} more`. A self-loop appears in both lists.
- Maximum width 320 px or 90% of the container, whichever is smaller; long words wrap with `overflow-wrap: anywhere`.
- The popover is a `<div part="tooltip" aria-hidden="true">` in the host's shadow root, or inside the fullscreen `<dialog>` while the SVG is there, since the modal dialog sits in the top layer above the host. `part` lets a page restyle it through `merlion-view::part(tooltip)`. It is `aria-hidden` because the live region already speaks the same text ([Keyboard](#keyboard-and-screen-readers)). It has `pointer-events: none`, so it never steals hover from the diagram.
- Colours: `--merlion-bg` background, `--merlion-fg` text, `--merlion-muted` for detail and context, 1 px `--merlion-border`, 6 px radius, the diagram's `--merlion-font`, at 13 px regardless of zoom, so it stays readable when labels are too small to read.

### Placement

Positioned against the lit element's bounding rectangle `E` and the visible box `C` (the host, or the dialog in fullscreen), with an 8 px gap:

1. Try below, above, right and left of `E`, in that order; take the first side where the popover fits entirely inside `C`.
2. Otherwise take the side with the most free space, cap the popover's height or width to it and let it scroll, and clamp it inside `C` along the other axis only.
3. If no side of `E` has 48 px free inside `C` (a node zoomed to fill the view), the popover stays hidden; the highlight and the announcement still happen.

The popover never overlaps `E`. Each show writes the content, then reads `E`, `C` and the popover size in one frame and writes the position: one forced layout per show, none per pointer move. A view change (zoom, pan, fullscreen, resize) repositions it in the next frame.

## Keyboard and screen readers

`<merlion-view>` stays the single tab stop. `<g tabindex="0">` is a Tab stop in Chromium (measured), so emitting it on every node would make a 60-node diagram 60 Tab presses to cross; the renderer emits no `tabindex`.

| Key, host focused | Action |
|---|---|
| `ArrowDown`, `ArrowRight` | Next node in declaration order (the outline's order); from no node, the first |
| `ArrowUp`, `ArrowLeft` | Previous node |
| `Home` / `End` | First / last node |
| `Enter`, `Space` | Pin or unpin the current node; `Enter` on a node wrapped in `<a href>` follows the link instead |
| `Shift` + `Enter` | Pin in path mode |
| `Esc` | Clear the pin and the current node; with neither, the base viewer's action |
| `Shift` + arrow keys | Pan by 10% (plain arrow keys pan only when the extension is not loaded) |
| `+`, `-`, `0`, `Tab` | Unchanged: zoom, reset, leave the diagram |

- The current node gets `merlion-active` and the focus ring; the lit set and popover follow it as if it were hovered. A node outside the visible box is panned into view (instantly under reduced motion).
- The extension handles `keydown` on the host in the capture phase and calls `stopPropagation` and `preventDefault` only for keys it consumes, so the base viewer sees every other key, and an `Esc` that clears a pin does not also close the fullscreen dialog.
- **Announcements.** A visually hidden `<div role="status" aria-live="polite">` in the shadow root receives, on every keyboard move and every pin, the element's text:
  - Node: the line the outline prints for it (cluster path prefix, name, outgoing edges with labels, [svg-output.md](svg-output.md#text-alternative)), built by the same rule even for nodes the outline omits, then `Incoming: {source name} [label], …` when it has incoming edges, then `({i} of {V})`.
  - Edge: `{from name} {glyph} {to name}` and the label in brackets.
- Descendant ids are not referenced from the host. Children of `role="img"` are presentational in WAI-ARIA, so `aria-activedescendant` into the SVG is not dependable (Chromium 153 still exposes 104 unignored text descendants of the fixture's image, measured), and ids do not resolve across the shadow boundary for `aria-describedby`.
- The host gets `aria-description="Arrow keys move between nodes; Enter pins; Escape clears."`, and keeps the SVG's `<title>` as its name.

## Touch

- A tap is a touch pointer released within 500 ms and 10 px of where it went down. Tapping a node or edge pins it; tapping the pinned element clears it, or follows its link if it has one; tapping the diagram background or anywhere outside the host clears (one capture-phase `pointerdown` listener on the document while pinned).
- Touch pointers never produce the hover state. One-finger drag when zoomed in still pans, pinch still zooms; a pin survives both. A double-tap still resets the zoom and leaves the pin from its first tap.
- Controls stay visible on touch devices, as today ([viewer.md](viewer.md#behaviour)).

## Theming

| Token | Default | Used for |
|---|---|---|
| `--merlion-highlight` | `var(--merlion-accent, #0969da)` | Border of the hovered or pinned node when it is untoned; the keyboard focus ring |
| `--merlion-highlight-stroke` | `2.5px` | Stroke width of lit elements (viewer layer); the CSS layer uses 2 × `--merlion-stroke` on the hovered element only |
| `--merlion-dim-opacity` | `0.3` | Opacity of elements outside the lit set, both layers |

- No token is a `color-mix()`, so none needs the `@supports` guard of mixed roles ([svg-output.md](svg-output.md#theming)); in a browser without `color-mix`, the use-site fallback `var(--merlion-node-border, …)` outside `@supports` reads the highlight directly.
- The embedded style reads `--merlion-dim-opacity` and declares nothing. The viewer's adopted sheet declares `--merlion-node-border` on the primary node and `--merlion-stroke` on lit groups only; that sheet is page CSS, which may declare tokens, and it reads every value from the host's own tokens.
- Tones, built-in roles, stylesheet roles, `classDef`, `style` and `linkStyle` colours win over the highlight colour on their element, because the highlight replaces only the default layer of each fallback chain ([Precedence](svg-output.md#precedence) is unchanged).
- `merlion-themes.css` sets `--merlion-dim-opacity: 0.6` under `@media (prefers-contrast: more)`, so dimmed text stays legible.
- A compiled stylesheet ([svg-output.md](svg-output.md#stylesheet)) may set `--merlion-highlight` (a colour) and `--merlion-dim-opacity` (a number from 0 to 1) in theme blocks; role selectors may not. A baked palette carries them as the fallback literals of the embedded rules that read them.

## Semantic zoom

- While `merlion-zoomed-out` is set, the lit set keeps its labels: the adopted sheet adds `merlion-view.merlion-zoomed-out .merlion-cluster .merlion-lit, merlion-view[data-merlion-rank-limit] .merlion-lit[data-merlion-rank] text {visibility: visible}`. Both selectors outrank the semantic-zoom selectors in `merlion-themes.css` ((0,3,1) against (0,3,0) and (0,2,1)), and the adopted sheet cascades after them in any case.
- Hidden nodes are not hit targets (`visibility: hidden` elements take no pointer events), so a node inside a collapsed cluster is reached by keyboard: moving to it reveals it and its neighbours.
- The popover shows the full text at 13 px, whatever the zoom.

## Security

- **The SVG still executes nothing.** The additions are `id` attributes, `data-merlion-join`, and rules whose selectors hold only the names listed in [Rules](#rules) and decimal integers generated by the core. The allowed rule prefixes ([svg-output.md](svg-output.md#embedded-style), [security.md](security.md#output)) become four: `#{id} `, `:where(#{id} `, `#{id}:not(:is([data-theme="light"] *)) ` and `#{id}:has(`. The `render` fuzz target additionally checks that every `:has(` argument is `#{id}-n{k}:hover`, `#{id}-e{k}:hover` or `:is(.merlion-node,.merlion-edge):hover`, and that every id named in a rule is defined in the same SVG.
- **No markup from diagram text.** The extension creates elements with `createElement` and fills them with `textContent`. `innerHTML` holds constant strings only (the existing controls). Attribute values from the SVG are used as `Map` keys and compared as strings; they are never interpolated into a selector, a style or markup, so no `CSS.escape` is involved.
- **No new capabilities.** The extension reads `data-merlion-*`, `class`, `id`, `marker-*` and text; it makes no request, reads no URL, and writes nothing outside the host, its shadow root and its own adopted sheet. Links are followed only through native `<a>` activation, whose URLs the renderer has already filtered ([svg-output.md](svg-output.md#links)).
- Popover size is bounded: labels are capped at 4,096 bytes by the core, and lists at 8 rows.
- Role classes stay presentation only; the popover shows no role names.
- The `merlion-select` event ([Events](#events)) carries source ids as strings; a listener that writes them into markup must escape them like any other untrusted text.

## Events

The host dispatches `merlion-select` (bubbling, composed) on every pin change, with `detail` `{ kind: "node" | "edge", id, from, to }` (`id` is `data-merlion-id` for a node; `from` and `to` are set for an edge) or `null` when cleared. Hover dispatches nothing.

## Performance and budgets

| Item | Budget |
|---|---|
| `@fractalboxdev/merlion-view/interact`, minified + gzip | ≤ 3 KB, enforced in CI by `scripts/size.mjs` beside the base budget |
| Base `<merlion-view>` with the extension hook | ≤ 6 KB (3.36 KB today) |
| Adjacency and text | Built once per adopted SVG, O(V + E); text is rebuilt per popover show from the lit element only |
| Edge hit index | Built once per adopted SVG, at most 20,000 samples; each query scans 9 cells |
| Per pointer move | One `closest()`; with no target, one grid query; class changes only when the target changes. No layout reads |
| Per hover change | Class toggles on the old and new lit sets (0.025–0.045 ms style recalculation for V + E from 160 to 1,250, measured), then one layout read when the popover shows |
| Path mode | One breadth-first search per direction per change, O(V + E), bounded by the core's 2,000-node, 4,000-edge limits |

The module stays separate because the base element plus an estimated 2.7 KB of interaction code would pass the 6 KB budget, and pages that only zoom should not pay for it. The pure functions (adjacency, path search, text reconstruction, placement) live in `interact-model.js`, which runs without a DOM, like `zoom.js`.

## Determinism

The render is unchanged as a function: the ids and hover rules depend on the source graph only, are emitted in a fixed order, and are byte-identical on native and WASM. Interaction state never reaches the SVG's bytes, the layout hint, the outline or the diagram id: the viewer writes only class tokens it later removes, host attributes and its own shadow DOM. `hover: "none"` omits the hover rules and keeps the ids.

## Options and integrations

| Surface | Option |
|---|---|
| Core | `RenderOptions.hover: Hover` (`Css` default, `None`) |
| CLI | `merlion render --hover css\|none` |
| WASM | `render(source, { hover: "css" \| "none" })`; any other value throws `TypeError` |
| rehype | `interactive: boolean`, default `true` when `viewer` is `true`; `false` writes `interactive="off"` on each `<merlion-view>` |
| Astro | With `interactive`, loads `@fractalboxdev/merlion-view/interact` in place of the base viewer script, on pages with a diagram |

TODO(owner): decide whether rehype passes `hover: "none"` when `interactive` is on, saving the rules' bytes on every page that loads the module at the cost of hover before the module loads and with JavaScript off.

## Testing

**Core (Rust):** every node and edge group has its id, and ids are unique; the number of hover rules is V + E + 2 at V + E ≤ 128 and 0 above it, with `I034`; every rule starts with an allowed prefix; `hover: None` changes only the rules; native and WASM output byte-identical over `compat`.

**Unit (`node --test`, no DOM):** adjacency and path search on hand-built graphs with self-loops, parallel edges, cycles and undirected links; text reconstruction against `plain_label` for joined, wrapped, Markdown and empty lines; placement against rectangles for all four sides, the scroll fallback and the 48 px refusal.

**Acceptance (Playwright, headless Chromium, every `compat` flowchart inlined in `<merlion-view>` with the extension):**

- For every node and every edge, hovering it lights exactly the expected set: `merlion-lit` on exactly those groups, computed opacity 1 on them and `--merlion-dim-opacity` on the rest. The expected set comes from the benchmark's graph extraction ([benchmark.md](benchmark.md)), not from the viewer's own adjacency.
- The CSS layer alone (no script) produces the same sets for every diagram with V + E ≤ 128, read from computed opacity.
- Pointing 4 CSS px off the midpoint of each edge path lights that edge.
- From host focus, pressing `ArrowDown` V times visits the nodes in declaration order; each announcement's first line equals that node's outline line wherever the outline has one.
- `Esc` returns to idle, and `svg.outerHTML` equals its value before the first hover.
- At viewport widths 360, 768 and 1280, every node's popover lies inside the visible box and does not intersect the node's rectangle, or is hidden under the 48 px rule.
- Under `prefers-reduced-motion: reduce`, the computed `transition-duration` of lit groups and the popover is `0s`.
- Under a `style-src 'self'` CSP, the viewer layer still lights the expected sets.
- axe-core reports no violation on the page with the extension that it does not report without it. TODO(owner): record axe-core (MPL-2.0, development dependency) in [licensing.md](licensing.md).
- On a 500-node synthetic flowchart, style recalculation per hover change stays under 1 ms.

## Out of scope

- Hover for diagram types other than flowcharts; each type adds its own lit-set rule when it lands (sequence: a message and its two lifelines; class and ER: a relation and its two entities).
- Mermaid-rendered SVGs in `<merlion-view>`: they carry no `data-merlion-from`/`-to`, and their class conventions change between releases.
- Graph-walking keys (move to a neighbour along an edge). TODO(owner): decide the key binding once the arrow-key traversal has user feedback.
- Hovering a collapsed cluster to highlight the edges crossing its boundary.
- Editing, dragging nodes, or any interaction that changes the source.
