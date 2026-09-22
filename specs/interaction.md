# Diagram interaction

Clicking a node pins a highlight on it: the node, the edges that touch it and the nodes at their other ends stay at full opacity, everything else dims, and a popover shows the node's full text and connections. Clicking an edge pins the edge, its label and both endpoints. Clicking a cluster title collapses the cluster. Hover is only a preview. Label text stays selectable throughout: a drag on text selects it, as in any HTML page. Interaction is presentation only: it never changes layout or re-renders, and interaction state never reaches the SVG's bytes ([Determinism](#determinism)).

Two layers deliver it ([ADR-0010](adr/0010-hover-interaction.md)):

| Layer | Where it lives | What it does |
|---|---|---|
| **Viewer layer** | `@fractalboxdev/merlion-view/interact`, loaded by `<merlion-view>` for every Merlion SVG | Click to pin a highlight (neighbourhood or transitive path), click to collapse clusters and hide nodes, a detail popover, keyboard traversal with screen-reader announcements, a hover preview. Reads only the `data-merlion-*` attributes and text the SVG already carries |
| **CSS layer** (Proposed, core) | Rules in the SVG's embedded `<style>`, for diagrams of at most 128 nodes plus edges | Hover preview with JavaScript off: dims the rest and keeps the hovered element's neighbourhood at full opacity. No pinning, popover or keyboard |

Both layers compute the same highlight set; the acceptance tests hold them to it ([Testing](#testing)). This spec extends [svg-output.md](svg-output.md) (data attributes, embedded rules) and [viewer.md](viewer.md) (gestures, extension hook).

## Highlight set

| Target | Lit set |
|---|---|
| Node `n` | `n`; every edge with `n` as `data-merlion-from` or `data-merlion-to`; every node at the other end of those edges |
| Edge `e` | `e`, including its label and markers; the nodes named by its `data-merlion-from` and `data-merlion-to` |
| Node `n`, path mode | `n`; every node reachable from `n` along edge direction (downstream) and every node that reaches `n` (upstream); every edge traversed by either search |
| Edge `e`, path mode | `e`; the upstream set of its `from` node and the downstream set of its `to` node, with their edges |

- Edge direction is the source direction (`data-merlion-from` → `data-merlion-to`), whatever `data-merlion-back` says about the drawn direction. An edge with neither `marker-end` nor `marker-start` on its path (`---`) is followed both ways in path mode.
- A self-loop is an incident edge of its node; parallel edges between the same pair are all incident.
- Path mode is two breadth-first searches over per-node incidence lists: O(V + E).
- Nodes and edges outside the lit set are dimmed. Clusters are never dimmed: a cluster group contains its members, so group opacity would dim them too, and the box and title are low-contrast already.
- An edge endpoint that names no node group (an edge ending on a cluster) lights nothing at that end.
- Invisible links (`~~~`) are not drawn and take no part.

## SVG additions

Proposed for the core with the CSS layer; the viewer layer needs none of them. Every render, whatever the options:

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
#{id}:has(:is(.merlion-node,.merlion-edge):hover) :is(.merlion-node,.merlion-edge){opacity:var(--merlion-dim-opacity, 0.2);}
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

- `<merlion-view>` imports `./interact.js` the first time it adopts an SVG with class `merlion`, so every Merlion diagram on a page is interactive with no extra script. `<merlion-view interactive="off">` runs no extension and loads nothing. Pages that wrap other renderers' SVGs never fetch the module. `import "@fractalboxdev/merlion-view/interact"` loads it eagerly.
- The module registers through the base element's extension hook, `MerlionView.extend(fn)`: `fn(host, svg)` runs whenever a host adopts an SVG and returns a cleanup function, called when the SVG changes, the host disconnects or `interactive` becomes `"off"`. A `view` method on the returned function runs after every view change.
- The base element provides the viewer chrome that extensions share ([viewer.md](viewer.md#extension-hook)): `tip(el, build)` (the popover), `say(text)` (the live region), `tap(e)` (click qualification) and `MerlionView.style(css)` (rules for the slotted SVG).
- The extension activates only for an SVG with class `merlion` whose `.merlion-edge` groups all carry `data-merlion-from` and `data-merlion-to`.
- On activation it reads, in one pass each, the node groups (`data-merlion-id`, label text, enclosing clusters), the edge groups (`data-merlion-from`/`-to`, markers, label text) and the cluster groups, and builds per-node incidence lists: O(V + E). The core's node and edge ids ([SVG additions](#svg-additions)) are not needed.

### Gestures

A click commands the diagram only when it is a **tap**: it lands on the drawing rather than a viewer control, the pointer moved less than 4 CSS px since it went down, it leaves no text selected, and it is not the second click of a double-click (`detail` > 1). A drag that selects text therefore never pins, and a double-click that selects a word keeps whatever its first click pinned.

| Tap on | Plain | Shift | Alt |
|---|---|---|---|
| A node (shape or label) | Pin it; on the pinned node, clear | Pin in path mode; on a node pinned in path mode, clear | Hide the node |
| An edge (path or label) | Pin it; on the pinned edge, clear | Pin in path mode | Pin it |
| A cluster title or its badge | Collapse the cluster; on a collapsed one, expand | Same | Same |
| The background (anywhere else in the host) | Clear | Clear | Clear |
| A node inside `<a href>` | The link is followed; nothing is pinned | | |

- Tapping another node or edge moves the pin. A plain tap on a target pinned in path mode clears it; Shift+tap on a plainly pinned target widens it to path mode.
- Shift+pointer-down on a node or edge prevents the default action, so Shift+click never extends a text selection.
- **Esc** clears the pin and the keyboard's current node; with neither, it falls through to the base viewer, which closes fullscreen.
- Hover draws a preview only: the pointed node's border thickens to `--merlion-highlight-stroke`. Nothing dims, no popover shows, nothing is announced. Touch pointers never hover; a finger tap is a tap.
- Pointer drags, text selection and double-click follow the base element's rules ([viewer.md](viewer.md#gestures)).

### States

| State | Lit set shown | Popover | Host attribute |
|---|---|---|---|
| Idle | None; nothing dimmed | Hidden | — |
| Pinned | The pinned target's | The pinned target's | `data-merlion-state` |
| Keyboard preview (nothing pinned) | The current node's | The current node's | `data-merlion-state` |

### Rendering the highlight

The extension changes only `class` tokens on SVG elements and attributes on the host, and removes every one of them on clear, so `svg.outerHTML` after clearing equals its value before the first pin:

| Where | Token | Meaning |
|---|---|---|
| Host | `data-merlion-interactive` | The extension is active |
| Host | `data-merlion-state` | Something is lit |
| Host | `data-merlion-hidden` | Something is hidden or collapsed; the base shows **Show all** |
| Node and edge groups in the lit set | class `merlion-lit` | Lit |
| The pinned or previewed element | class `merlion-primary` | The element the popover describes |
| The keyboard's current node | class `merlion-active` | Focus ring |

The rules live in the base element's light-DOM sheet (`MerlionView.style`), a constructed `CSSStyleSheet` adopted once by each host's root (`document` or the enclosing shadow root). Adopted sheets cascade after the document's own sheets and apply under a CSP without `'unsafe-inline'`:

```css
[data-merlion-state][data-merlion-interactive] :is(.merlion-node,.merlion-edge):not(.merlion-lit),
[data-merlion-interactive] .merlion-stub:not(.merlion-lit){opacity:var(--merlion-dim-opacity,.2)}
[data-merlion-interactive] .merlion-node:hover,.merlion-edge.merlion-lit,.merlion-primary{--merlion-stroke:var(--merlion-highlight-stroke,2.5px)}
.merlion-primary{--merlion-node-border:var(--merlion-highlight,var(--merlion-accent,#0969da))}
.merlion-active{outline:2px solid var(--merlion-highlight,var(--merlion-accent,#0969da));outline-offset:3px}
@media (prefers-reduced-motion:no-preference){[data-merlion-interactive] :is(.merlion-node,.merlion-edge){transition:opacity .15s}}
```

- Group opacity dims everything inside the group: the path, its markers, the edge label and its background.
- **Tones survive.** The highlight colour sets `--merlion-node-border`, which only an untoned node's border reads; a tone, a role or a source colour sits earlier in the fallback chain, so a pinned node keeps its role colours and gains only the thicker border. Measured in Chromium: an untoned pinned node's stroke is `rgb(9, 105, 218)` at 2.5 px.
- Edges are emphasised by width and full opacity, never by colour: an arrowhead inherits from its `<marker>` in `<defs>`, not from the edge ([Roles](svg-output.md#roles)), so a recoloured path would end in an arrowhead of the old colour.
- With `prefers-reduced-motion: reduce`, the dim switches instantly (computed `transition-duration` `0s`, measured); otherwise it fades over 150 ms.

### Hide and collapse

| Action | Effect |
|---|---|
| Collapse a cluster (tap its title) | Its member nodes, nested clusters and the edges between its members get `merlion-hidden` (`display: none`). The box and title stay, the box border turns dashed (`merlion-collapsed`), and the title gains a run `<tspan class="merlion-badge"> +N</tspan>`, N being the member nodes hidden under it |
| Hide a node (Alt+tap) | The node and its incident edges get `merlion-hidden`: nothing is left for those edges to point at |
| **Show all** (controls bar, shown while anything is hidden) | Expands every cluster and shows every node |

- An edge from outside a collapsed cluster to one of its members stays drawn, dimmed (`merlion-stub`), and ends at the member's old position inside the box.
- An edge between members of two different collapsed clusters is a stub; an edge whose ends share the outermost collapsed cluster is hidden.
- Collapsing an outer cluster takes the count of a collapsed inner one: the badge sits on the outermost collapsed cluster only.
- Hiding the pinned element, or the keyboard's current node, clears it. Keyboard traversal skips hidden nodes.
- Hide state lives in the viewer. Nothing re-renders or re-lays out, and the SVG's own `<style>` is untouched; the badge run is removed on expand, so **Show all** restores the markup byte for byte (measured).

## Detail popover

### Content

Built with `textContent` from [Text reconstruction](#text-reconstruction), top to bottom:

| Part | Node | Edge |
|---|---|---|
| Heading, bold | Title lines | `{from name} {glyph} {to name}` |
| Body | Detail lines, one per line, muted | The edge label, if any |
| Context, muted | Cluster path, if any | — |
| Outgoing | One row per outgoing edge: `{glyph} {target name}` and the label in brackets | — |
| Incoming | One row per incoming edge: `← {source name}` and the label in brackets | — |

- The popover is the base element's `part="tooltip"` element, in the host's shadow root, or inside the fullscreen `<dialog>` while it is open, since the modal dialog sits in the top layer above the host. `part` lets a page restyle it through `merlion-view::part(tooltip)` outside fullscreen.
- It is `aria-hidden`, because the live region speaks the node's outline line ([Keyboard](#keyboard-and-screen-readers)), and it has `pointer-events: none`, so it never blocks a click or a text selection underneath.
- Maximum width 320 px or 90% of the visible box, whichever is smaller; long words wrap with `overflow-wrap: anywhere`. Its style is inline (set through the CSSOM, so a strict CSP allows it): `--merlion-bg` background, `--merlion-fg` text, a translucent grey border, 6 px radius, 13 px system UI font regardless of zoom.
- It is dismissed with the pin: tapping the target again, tapping the background, or Esc.

### Placement

Positioned against the element's bounding rectangle `E` and the visible box `C` (the host, or the dialog in fullscreen), with an 8 px gap:

1. Try below, above, right and left of `E`, in that order; take the first side where the popover fits entirely inside `C`, centred on `E` and clamped inside `C` along the other axis.
2. Otherwise take the side with the most free space and cap the popover's height or width to it.
3. If no side of `E` has 48 px free inside `C` (a node zoomed to fill the view), the popover stays hidden; the highlight and the announcement still happen.

The popover never overlaps `E`. Every view change (zoom, pan, fullscreen, resize) and the end of a zoom animation place it again.

## Keyboard and screen readers

`<merlion-view>` stays the single tab stop.

| Key, host focused | Action |
|---|---|
| `ArrowDown`, `ArrowRight` | Next visible node in outline order; from no node, the first. Wraps around |
| `ArrowUp`, `ArrowLeft` | Previous visible node |
| `Enter` | Pin or unpin the current node |
| `Esc` | Clear the pin and the current node; with neither, the base viewer's action |
| `Shift` + arrow keys | Pan by 10% (plain arrow keys pan only when the extension is not loaded) |
| `+`, `-`, `0`, `Tab` | Unchanged: zoom, reset, leave the diagram |

- **Outline order** is the order of the nodes' lines in the SVG's `<desc>` outline ([svg-output.md](svg-output.md#text-alternative)): a line matches a node when it equals the node's prefix (`{cluster path}: {name}`) or continues it with a space and an edge glyph. Nodes without a line follow in document order. Document order differs from outline order in 10 of the 43 gallery flowcharts, so the viewer does not use it alone.
- The current node gets `merlion-active` and the focus ring. With nothing pinned, its lit set and popover show as a preview.
- The extension handles `keydown` on the host in the capture phase and calls `preventDefault` on the keys it consumes; the base viewer skips an event whose default is prevented, so a consumed arrow does not also pan and an `Esc` that clears a pin does not also close fullscreen.
- **Announcements.** The base element's visually hidden `<div role="status" aria-live="polite">` receives, on every keyboard move and every pin of a node, that node's outline line: the matching `<desc>` line, or its prefix when the outline has none. Hover never announces.
- Descendant ids are not referenced from the host. Children of `role="img"` are presentational in WAI-ARIA, so `aria-activedescendant` into the SVG is not dependable, and ids do not resolve across the shadow boundary for `aria-describedby`.

## Theming

| Token | Default | Used for |
|---|---|---|
| `--merlion-highlight` | `var(--merlion-accent, #0969da)` | Border of the pinned node when it is untoned; the keyboard focus ring |
| `--merlion-highlight-stroke` | `2.5px` | Stroke width of the pinned node, lit edges and the hovered node |
| `--merlion-dim-opacity` | `0.2` | Opacity of elements outside the lit set, and of stub edges |

- No token is a `color-mix()`, so none needs the `@supports` guard of mixed roles ([svg-output.md](svg-output.md#theming)).
- Tones, built-in roles, stylesheet roles, `classDef`, `style` and `linkStyle` colours win over the highlight colour on their element, because the highlight replaces only the default layer of each fallback chain ([Precedence](svg-output.md#precedence) is unchanged).
- A compiled stylesheet ([svg-output.md](svg-output.md#stylesheet)) may set `--merlion-highlight` (a colour) and `--merlion-dim-opacity` (a number from 0 to 1) in theme blocks; role selectors may not.

## Semantic zoom

- Semantic zoom hides labels, never nodes, so every node stays a tap target at every zoom level. The popover shows the full text at 13 px, whatever the zoom.

## Security

- **The SVG still executes nothing.** The viewer layer adds nothing to the renderer's output.
- **No markup from diagram text.** The extension creates elements with `createElement`/`createElementNS` and fills them with `textContent`. `innerHTML` holds constant strings only (the base's controls). Attribute values from the SVG are used as `Map` keys and compared as strings; they are never interpolated into a selector, a style or markup, so no `CSS.escape` is involved.
- **No new capabilities.** The extension reads `data-merlion-*`, `class`, `marker-*`, the `<desc>` text and label text; it makes no request, reads no URL, and writes nothing outside the host's SVG class tokens, the host's attributes, the base's popover and live region, and the shared light-DOM sheet. Links are followed only through native `<a>` activation, whose URLs the renderer has already filtered ([svg-output.md](svg-output.md#links)).
- Popover size is bounded: labels are capped at 4,096 bytes by the core, and the popover scrolls within the visible box.
- Role classes stay presentation only; the popover shows no role names.

## Performance and budgets

| Item | Budget |
|---|---|
| `@fractalboxdev/merlion-view/interact`, minified + gzip | ≤ 3 KB (3,023 B), enforced by `scripts/size.mjs` |
| Base `<merlion-view>` with the hook and the shared chrome | ≤ 6 KB (4,938 B), enforced by the same script |
| Model and text | Built once per adopted SVG, O(V + E); popover text is rebuilt per pin from the target only |
| Per pin | Class removal on the old lit set, class addition on the new one, one layout read to place the popover |
| Path mode | One breadth-first search per direction, O(V + E), bounded by the core's 2,000-node, 4,000-edge limits |

The size script bundles each entry on its own: the base with `./interact.js` external (it loads on demand), and the interaction module with the base external. The pure logic (adjacency, reachability, focus sets, collapse sets, tap outcome, text reconstruction, outline order) lives in `interact-model.js`; the gesture rules and popover placement live in `zoom.js`. Both run without a DOM and are tested with `node --test`.

## Determinism

The render is unchanged as a function: the ids and hover rules depend on the source graph only, are emitted in a fixed order, and are byte-identical on native and WASM. Interaction state never reaches the SVG's bytes, the layout hint, the outline or the diagram id: the viewer writes only class tokens and badge runs it later removes, host attributes and its own shadow DOM. `hover: "none"` omits the hover rules and keeps the ids.

## Options and integrations

| Surface | Option |
|---|---|
| Core | `RenderOptions.hover: Hover` (`Css` default, `None`) |
| CLI | `merlion render --hover css\|none` |
| WASM | `render(source, { hover: "css" \| "none" })`; any other value throws `TypeError` |
| `<merlion-view>` | Interactive by default for Merlion SVGs; `interactive="off"` opts one element out |
| rehype | `interactive: boolean`, default `true` when `viewer` is `true`; `false` writes `interactive="off"` on each `<merlion-view>` |
| Astro | Loads the base viewer on pages with a diagram, which loads the interaction module itself |

TODO(owner): decide whether rehype passes `hover: "none"` when `interactive` is on, saving the rules' bytes on every page that loads the module at the cost of hover before the module loads and with JavaScript off.

## Testing

**Core (Rust, CSS layer):** every node and edge group has its id, and ids are unique; the number of hover rules is V + E + 2 at V + E ≤ 128 and 0 above it, with `I034`; every rule starts with an allowed prefix; `hover: None` changes only the rules; native and WASM output byte-identical over `compat`.

**Unit (`node --test`, no DOM):** adjacency and path search on hand-built graphs with self-loops, parallel edges, cycles and undirected links, and a 20,000-edge cycle; collapse sets for nested clusters, stub and hidden edges, hidden nodes; tap outcomes; the pan, tap and double-click rules; text reconstruction for joined, wrapped and run-split lines; outline prefix and order, including a name that prefixes another's; placement on all four sides, the capped fallback and the 48 px refusal.

**Acceptance (Playwright, headless Chromium, Merlion SVGs inlined in `<merlion-view>`):**

- A drag across a label selects its text, leaves the transform unchanged and pins nothing; no computed `user-select: none` on the SVG or its text.
- A tap on a node dims exactly the complement of the expected set, computed from the `data-merlion-*` attributes independently of the viewer; a second tap clears; a tap on another node moves the pin; Esc and a background tap clear.
- Shift+tap lights the transitive set; a tap on an edge label lights the edge and its endpoints.
- A tap on a cluster title hides exactly its members, keeps the box, badges it `+N` and shows **Show all**; Alt+tap hides one node; **Show all** restores `svg.outerHTML` byte for byte.
- Zoomed in, a background drag pans and pins nothing, a text drag selects and does not pan, and a double-click selects a word and keeps the zoom.
- `ArrowDown` walks the `<desc>` lines in order and announces each; Enter pins; Esc clears.
- In fullscreen the popover sits in the dialog; the first Esc clears the pin and the second closes the dialog.
- A zoom-button click keeps the pin; `interactive="off"` and a non-Merlion SVG get no interaction.
- Under `prefers-reduced-motion: reduce`, the computed `transition-duration` of node groups is `0s`; otherwise `0.15s`.

TODO(owner): extend acceptance to every `compat` flowchart, the CSS layer's parity with the viewer layer, a `style-src 'self'` CSP page, and axe-core (MPL-2.0, development dependency, to be recorded in [licensing.md](licensing.md)).

## Out of scope

- Hover for diagram types other than flowcharts; each type adds its own lit-set rule when it lands (sequence: a message and its two lifelines; class and ER: a relation and its two entities).
- Mermaid-rendered SVGs in `<merlion-view>`: they carry no `data-merlion-from`/`-to`, and their class conventions change between releases.
- Graph-walking keys (move to a neighbour along an edge). TODO(owner): decide the key binding once the arrow-key traversal has user feedback.
- A `merlion-select` event on pin changes. TODO(owner): decide whether pages need it; it costs about 100 bytes of the interaction module's budget.
- Rerouting stub edges to the collapsed cluster's border, which would need layout.
- A popover button to hide a node; Alt+tap hides one.
- Editing, dragging nodes, or any interaction that changes the source.
