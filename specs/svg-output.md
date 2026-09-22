# SVG output

Every render returns one self-contained SVG string and a plain-text outline. Inlining the SVG in HTML needs no sanitiser, even when the Mermaid source is untrusted ([security.md](security.md)). The output never contains:

- `<script>`, `<foreignObject>`, `<iframe>`, `<image>`, `<use>`, `<animate>`, `<set>` or any other animation element;
- event-handler attributes (`on*`), `style` attributes built from source text, or `xlink:href`;
- an `href` outside the link rules in [Links](#links);
- external references: no `url()` other than `url(#{id}-…)`, no `@import`, no `@font-face` other than the embedded font in `font: "embed"` mode, and no `<`, `&` or `\` anywhere in the embedded `<style>`;
- a CSS selector that can match outside the SVG ([Embedded style](#embedded-style)).

## Root element

```html
<svg xmlns="http://www.w3.org/2000/svg"
     id="{id}"
     viewBox="0 0 {w} {h}" width="100%" style="max-width:{w}px"
     role="img" aria-labelledby="{id}-title {id}-desc"
     class="merlion merlion-{type}"
     data-merlion-version="{semver}"
     data-merlion-layout="{hint}">
  <title id="{id}-title">…</title>
  <desc id="{id}-desc">…</desc>
  <style>…</style>
  …
</svg>
```

- `viewBox` is always present; there is no fixed `height`, so the aspect ratio holds at any width and the diagram causes no layout shift.
- There is no background rectangle unless `background: true`, so the host page's background shows through.
- Coordinates are rounded and printed as specified in [architecture.md](architecture.md#determinism).

## Theming

Every colour, font and stroke decision is set twice: as a presentation attribute carrying the default value (`fill="#f6f7f9"`), and as a CSS rule in the embedded `<style>` reading a custom property with the same fallback (`fill: var(--merlion-node-bg, #f6f7f9)`). CSS rules override presentation attributes, so in a browser the custom properties win. SVG renderers outside browsers support custom properties partially or not at all. librsvg draws the fallback of a single-level `var()` and drops a declaration whose fallback is itself a `var()`, drawing the presentation attribute instead. Every attribute and every innermost fallback carry the same literal, so both paths draw the same colour: the built-in default, or the palette value when the render has one ([Palette](#palette)). A host page themes every diagram by setting the variables on `:root` or any ancestor, including under `prefers-color-scheme` or a `[data-theme]` selector. Switching theme never re-renders.

Two foundation tokens drive the rest. The other roles default to mixes of those two using `color-mix(in oklab, …)`, and each can be overridden on its own. The embedded style defines the mixed roles only inside `@supports (color: color-mix(in oklab, #000, #fff))`. A browser that supports custom properties but not `color-mix` would otherwise treat every use of a mixed role as invalid at computed-value time, which resets the property instead of falling back; outside the `@supports` block, each use site falls back to the literal default (`var(--merlion-node-bg, #f6f7f9)`). The literal default of a mixed role, used in presentation attributes and fallbacks, is the mix of the two foundation defaults, stored as a constant in the core; a palette replaces it with the mix of its own foundations ([Palette](#palette)).

| Token | Default |
|---|---|
| `--merlion-bg` | `#ffffff` |
| `--merlion-fg` | `#1f2328` |
| `--merlion-muted` | `color-mix(in oklab, var(--merlion-fg) 55%, var(--merlion-bg))` |
| `--merlion-line` | `color-mix(in oklab, var(--merlion-fg) 45%, var(--merlion-bg))` |
| `--merlion-surface` | `color-mix(in oklab, var(--merlion-fg) 4%, var(--merlion-bg))` |
| `--merlion-border` | `color-mix(in oklab, var(--merlion-fg) 22%, var(--merlion-bg))` |
| `--merlion-accent` | `#0969da` |
| `--merlion-ok` / `-warn` / `-danger` | `#1a7f37` / `#9a6700` / `#cf222e` (tones of the built-in roles; see [Roles](#roles)) |
| `--merlion-node-bg` / `-node-border` / `-node-text` | surface / border / fg |
| `--merlion-node-detail` | muted (detail lines of title + detail node labels; see [Text](#text)) |
| `--merlion-edge` / `-edge-label-bg` | line / bg |
| `--merlion-cluster-bg` / `-cluster-border` | `color-mix(in oklab, var(--merlion-fg) 2%, var(--merlion-bg))` / border |
| `--merlion-series-1` … `--merlion-series-8` | Categorical palette for charts, pie and gantt sections |
| `--merlion-font` | `Inter, ui-sans-serif, system-ui, sans-serif` |
| `--merlion-font-size` | `14px` (must match the measured size; see [text-measurement.md](text-measurement.md)) |
| `--merlion-stroke` | `1.25px` |
| `--merlion-tone` | Unset. Per-element: set on a node, edge or cluster role ([Roles](#roles)) |
| `--merlion-dash` | Unset. Per-element dash pattern, same grammar as `stroke-dasharray` |
| `--merlion-c-{name}-fill` / `-stroke` / `-color` | Unset. Replaces the literal of `classDef {name}` for that property ([Source styles](#source-styles-classdef-style-linkstyle)) |

`@fractalboxdev/merlion-themes` ships `merlion-themes.css`: light and dark defaults plus named themes, each defining the foundations, the accent, the three role tones (`--merlion-ok`, `--merlion-warn`, `--merlion-danger`) and the series palette. Every named theme is an original palette or one whose licence is recorded in [licensing.md](licensing.md).

### Source styles: `classDef`, `style`, `linkStyle`

Mermaid style statements are free-form CSS in the source. The parser reads each `property:value` pair into a typed value and the renderer re-serialises it from that value; source text never reaches the output. Accepted pairs:

| Property | Accepted values |
|---|---|
| `fill`, `stroke`, `color` | `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`; `rgb()`, `rgba()`, `hsl()`, `hsla()` with numeric or percentage arguments; the CSS named colours; `transparent`; `none` (not for `color`) |
| `stroke-width` | A number from 0 to 20, optionally followed by `px` |
| `stroke-dasharray` | Up to 8 numbers from 0 to 100, separated by spaces or commas |
| `opacity`, `fill-opacity`, `stroke-opacity` | A number from 0 to 1 |
| `font-weight` | `normal` or `400`; `bold`, `600` or `700`, all drawn and measured as 600 (the measured SemiBold) |
| `font-style` | `normal`, `italic` |

- Any other property or value is dropped with `W010 StyleRejected` (an `Error` under `strict`).
- A `classDef` name must match `[A-Za-z_][A-Za-z0-9_-]{0,63}`; otherwise the statement is dropped with `W011 ClassNameRejected`. It is emitted as the class `merlion-c-{name}`, so it can't collide with the host page's classes.
- Source-style rules target the shapes and `text` elements inside the class (`#{id} .merlion-c-{name} text { … }`), which is more specific than the text reset below, so a source `font-weight` or `color` applies. On an edge, the stroke properties apply to the path and `color` to the label.
- A `classDef` colour reads a per-class token with the source literal as fallback: `fill: var(--merlion-c-{name}-fill, <literal>)`, and likewise `--merlion-c-{name}-stroke` for `stroke` and `--merlion-c-{name}-color` for `color`. The presentation attribute carries the literal. A stylesheet ([Stylesheet](#stylesheet)) or the host page re-themes the class by setting the token on `:root`, a `[data-theme]` block or the role; unset, the literal shows. The embedded style reads these tokens and never declares them. Other `classDef` properties, and every `style` and `linkStyle` value, are literals.
- `style` and `class` on a subgraph id style the cluster box and title only: the cluster carries `merlion-ss-{index}` for `style` and `merlion-cc-{name}` for each class, with or without a `classDef`, and their rules use child combinators (`#{id} .merlion-cc-{name} > .merlion-cluster-box`, `… > .merlion-cluster-title`), so member nodes and nested clusters keep their own colours.
- A colour set in the source ignores the theme, so the parser emits `I030 FixedColour`: a `classDef` colour stays fixed unless a stylesheet or page sets its token; a `style` or `linkStyle` colour stays fixed.

### Roles

A role is a class name given with `class` or `:::` whether or not a `classDef` defines it. Nodes and edges carry `merlion-c-{name}`, clusters `merlion-cc-{name}`. An edge takes a role through its id: `a e1@--> b` then `class e1 failure`. Role names follow the `classDef` grammar; others are dropped with `W011`. Role classes carry presentation only, never provenance or trust: any diagram can put any role on any element.

Two per-element tokens restyle a role: `--merlion-tone` and `--merlion-dash`. The embedded style consumes them in every render:

| Element | Property | Value |
|---|---|---|
| Node shape | `fill` | `color-mix(in oklab, var(--merlion-tone, <node-bg>) 14%, <node-bg>)` |
| Node shape | `stroke` | `var(--merlion-tone, <node-border>)` |
| Node label | `fill` | `color-mix(in oklab, var(--merlion-tone, <node-text>) 75%, <node-text>)` |
| Edge path, marker | `stroke`, marker `fill` | `var(--merlion-tone, <edge>)` |
| Edge label | `fill` | `color-mix(in oklab, var(--merlion-tone, <fg>) 75%, <fg>)` |
| Cluster box | `fill`, `stroke` | `color-mix(in oklab, var(--merlion-tone, <cluster-bg>) 8%, <cluster-bg>)`, `var(--merlion-tone, <cluster-border>)` |
| Cluster title | `fill` | `color-mix(in oklab, var(--merlion-tone, <fg>) 75%, <fg>)` |
| Node shape, edge path, cluster box | `stroke-dasharray` | `var(--merlion-dash, <the element's default>)` |

`<node-bg>` stands for the role's full fallback chain (`var(--merlion-node-bg, var(--merlion-surface, #f5f5f5))`). With the tone unset, each mix combines a colour with itself, so an untoned element draws exactly its default. The mixes sit inside the `@supports` block; outside it, the element draws its untoned default.

Every node, edge and cluster group resets both tokens at zero specificity:

    :where(#{id} .merlion-node, #{id} .merlion-edge, #{id} .merlion-cluster, #{id} marker) { --merlion-tone: initial; --merlion-dash: initial; }

A tone set on an ancestor, on `:root` or on a cluster therefore never reaches member elements, nor the arrow markers, which inherit from `<defs>`; a role rule on the group itself (specificity 0,1,0 or more) wins over the reset and reaches the group's shape and text by inheritance. These two tokens are the only custom properties the embedded style declares.

Each edge role in use gets its own marker in `<defs>` (`{id}-arrow-c-{name}`, likewise `-circle-` and `-cross-`) carrying the role class, because a marker inherits from `<defs>`, not from the edge that references it. An edge with several roles uses one marker per distinct role set, `{id}-arrow-r{k}` with `k` the set's first-use index, carrying every class of the set. `classDef` and `linkStyle` colours reach the edge path and label, not the marker.

#### Built-in roles

Eight roles are styled with no stylesheet ([ADR-0009](adr/0009-stylesheet.md)). Their rules are emitted only for roles the diagram uses, before `classDef` rules:

| Role | Applies to | Tone | Dash |
|---|---|---|---|
| `accent` | Nodes | `--merlion-accent` | — |
| `ok` | Nodes | `--merlion-ok` | — |
| `warn` | Nodes | `--merlion-warn` | — |
| `danger` | Nodes | `--merlion-danger` | — |
| `muted` | Nodes | `--merlion-muted` | — |
| `group` | Clusters (`merlion-cc-group`) | — | `6 4` on the box |
| `failure` | Edges | `--merlion-danger` | `6 4` |
| `async` | Edges | — | `6 4` |

A built-in role's use-site rules put the role's tone token in place of the unset tone, `var(--merlion-tone, var(--merlion-danger, #cf222e))`, and its dash in place of the default, `var(--merlion-dash, 6 4)`. The presentation attributes and the fallbacks outside `@supports` carry the mixed literals of the default tone, so the role shows in every renderer, and a page theme that sets `--merlion-danger` retunes every `danger` and `failure` element. A stylesheet or page rule on the role (`.merlion-c-danger { --merlion-tone: … }`) wins over the reset and replaces the built-in tone. A role used on an element kind the table does not list for it, or a name the table does not list, has no built-in style. A `classDef` of the same name replaces the built-in role for the whole diagram: `classDef ok fill:#e6ffed` gives `ok` nodes exactly the `classDef` properties, with no built-in tint on the label, stroke, dash or marker, so a diagram written for mermaid draws as it did before built-in roles existed.

#### Precedence

Per property and per element, highest first:

1. Node `style` and edge `linkStyle` literals.
2. `classDef` colours, through their `--merlion-c-{name}-*` tokens; other `classDef` properties as literals.
3. Stylesheet role rules, by specificity, then source order.
4. Built-in roles.
5. The theme, then the built-in default.

A source literal that masks a stylesheet tone on the same element (a `style` colour, or a `classDef` colour whose token the stylesheet leaves unset) emits `I033 ToneMasked`. Built-in roles never emit `I033`.

### Palette

`RenderOptions.palette` ([ADR-0009](adr/0009-stylesheet.md)) carries literals resolved from a stylesheet ([Stylesheet](#stylesheet)) for one theme. It replaces the built-in literal table: the presentation attribute, the innermost `var()` fallback and the fallback outside `@supports` all carry the palette value. Mixed roles the stylesheet leaves unset are recomputed from the resolved `--merlion-bg` and `--merlion-fg` with the core's `oklab_mix`, correct to ±1 per 8-bit channel against Chrome. Per-role tone and dash literals become the fallbacks of `var(--merlion-tone, …)` and `var(--merlion-dash, …)` in rules scoped to `#{id} .merlion-c-{name}` / `.merlion-cc-{name}`, emitted only for roles the diagram uses and before `classDef` rules. A `--merlion-c-{name}-*` value replaces that `classDef` literal in the attribute and the fallback.

A palette never declares a custom property, so a host that sets tokens on an ancestor still themes an inlined baked SVG.

With a dark table, the embedded style adds one block:

    @media (prefers-color-scheme: dark) {
      #{id}:not(:is([data-theme="light"] *)) .merlion-node > .merlion-shape { fill: var(--merlion-node-bg, var(--merlion-surface, <dark literal>)); }
      …
    }

In Chrome, the query inside an `<img>`-embedded SVG follows the embedding page's used `color-scheme`. The block serves `<img>` and file embeds; inline hosts render without a palette.

### Stylesheet

A stylesheet is a CSS subset that sets tokens. `stylesheet::compile` parses it into a typed model; nothing from it is passed through as text.

Selectors, ASCII only, alone or in lists of up to 8:

| Selector | Meaning |
|---|---|
| `:root` | Base theme |
| `[data-theme="<t>"]`, `<t>` matching `[a-z][a-z0-9-]{0,31}` | Named theme |
| `:root:not([data-theme])` inside `@media (prefers-color-scheme: dark)` | Automatic dark theme |
| `.merlion-c-<name>`, `.merlion-cc-<name>` | Node and edge role, cluster role |
| `[data-theme="<t>"] .merlion-c-<name>`, `[data-theme="<t>"] .merlion-cc-<name>` | Role under a named theme |
| `:root:not([data-theme]) .merlion-c-<name>`, `:root:not([data-theme]) .merlion-cc-<name>` inside `@media (prefers-color-scheme: dark)` | Role under the automatic dark theme |

Declarations: the colour tokens of the [Theming](#theming) table, `--merlion-c-<name>-fill` / `-stroke` / `-color`, `--merlion-stroke`, `--merlion-tone`, `--merlion-dash`, and private `--<ident>` colours in theme blocks. A private token is usable only through `var()` and is never emitted. Colours follow the [source-style grammar](#source-styles-classdef-style-linkstyle) plus `oklab()` and `oklch()` inside sRGB. A value is a literal or `var(--<name>[, <literal>])` naming a token declared in the same file; references resolve at compile time to a depth of 8.

Theme selectors (`:root`, `[data-theme]`, the automatic dark theme) set the colour, `classDef`, stroke and private tokens; role selectors set `--merlion-tone` and `--merlion-dash` only. A reference resolves in the theme of its rule: the theme's own block over `:root`; a role rule inside the dark media block resolves in the automatic dark theme. A bare role selector inside the media block is `W017`. Palettes (`--theme`, `--auto-dark`) read named blocks only, so automatic-dark role rules reach the page CSS, not baked SVGs.

Rejected, with the rule or declaration dropped: `--merlion-font`, `--merlion-font-size`, every property not listed, and a token the rule's selector may not set (`W018`); `color-mix()`, `calc()`, `env()`, `url()`, quoted strings, `!important`, CSS escapes, a `var()` fallback that is not a literal (`W018`); an undefined name without a fallback, cycles and more than 8 `var()` hops (`W019`); nesting and every at-rule other than `@media (prefers-color-scheme: dark)` (`W017`); a selector outside the table in a rule that declares a `--merlion-*` token (`W017`; the rule's other selectors are kept). Rules that declare no `--merlion-*` token, other than theme rules declaring private tokens, are dropped with a single `I032` count. Limits: 64 KiB, 512 rules, 32 declarations per rule, 8 selectors per list in a rule that declares a token, 16 theme names, 256 role selectors, block depth 2, and 64 KiB of compiled output (`E013`); 100 diagnostics, then one `Info` summary count carrying the code of the first one left out.

The compiled page CSS uses literal values only and fixed selector shapes: `:root`, `[data-theme="<t>"]`, `:root:not([data-theme])` inside the media query, and role selectors prefixed with `.merlion ` (`.merlion .merlion-c-<name>`, `[data-theme="<t>"] .merlion .merlion-c-<name>`, and `:root:not([data-theme]) .merlion .merlion-c-<name>` inside the media query, consecutive ones sharing one block). Role rules keep their source order, and the prefix adds the same (0,1,0) to each shape, so unthemed, named-theme and automatic-dark role rules rank as they do in the source. Compiling compiled output yields the same bytes. A page links only compiled output or the shipped `merlion-themes.css`.

### Embedded style

Every rule in the embedded `<style>` starts with one of three prefixes: `#{id} `, `:where(#{id} ` (the per-element token reset in [Roles](#roles)), or `#{id}:not(:is([data-theme="light"] *)) ` (inside the palette's dark block). An inline SVG's `<style>` applies to the whole HTML document, so an unscoped rule would restyle the host page; with the prefix, two diagrams from different Merlion versions on one page don't interfere either. The style never contains `:root`, `html`, `body`, or a selector made only of an element name or `*`, and never declares a theme token. Only the token reset uses `:where()`: librsvg 2.62 drops every rule that contains it, and the reset is the one rule a CSS-less renderer does not need. Role label rules reach the text through child combinators instead, at specificity (1,1,1): `#{id} .merlion-c-{name}>text` on nodes, `#{id} .merlion-c-{name}>*>text` on edges, `#{id} .merlion-cc-{name}>text` on clusters. Each beats the base label rule (1,1,0) and ties the later `classDef` and `style` text rules, which therefore win.

The embedded style is parsed by HTML as foreign content, where `<` starts markup. `style::build` drops any rule whose text contains `<`, `&` or `\`, or `@` outside `@supports`, `@media (prefers-color-scheme: dark)` and the embedded `@font-face`.

The root rule resets the text properties that inline SVG would otherwise inherit from the host page and that the text measurement assumes ([text-measurement.md](text-measurement.md#measuring)):

```css
#{id} text {
  font-family: var(--merlion-font, Inter, ui-sans-serif, system-ui, sans-serif);
  font-size: var(--merlion-font-size, 14px);
  font-weight: 400; font-style: normal; font-stretch: normal;
  font-kerning: normal; font-variant-ligatures: none;
  font-feature-settings: "calt" 0, "liga" 0;
  letter-spacing: 0; word-spacing: 0; text-transform: none;
}
```

## Text

- Every label is `<text>` / `<tspan>` with explicit `x`/`y`; wrapped lines are one `<tspan>` each.
- Text is escaped: `&`, `<`, `>`, `"`, `'` become entities in text and in attribute values.
- Dropped characters: control characters other than tab and newline, and the non-characters U+FFFE and U+FFFF, so the SVG is well-formed XML 1.0.
- Bidirectional formatting characters (U+202A–U+202E, U+2066–U+2069) are stripped with `W014 BidiControlStripped`. They can make a label display in a different order from its source text; right-to-left scripts render correctly without them through the Unicode bidirectional algorithm.
- Markdown in labels (`**bold**`, `*italic*`, `` `code` ``) becomes `<tspan>` with `font-weight`, `font-style` or `font-family` set. Bold and italic runs are measured with the matching weight table. Code runs are drawn in `ui-monospace, SFMono-Regular, Menlo, Consolas, monospace` and measured at a fixed 0.6 em per glyph (1 em for CJK, Hangul and fullwidth glyphs, 0 for combining marks), with no kerning: those fonts draw Latin at 0.55–0.602 em, so the estimate is within 0.01 em per glyph and errs wide except against 0.602 em fonts. Other HTML in labels is rendered as literal text.
- **Title + detail node labels.** A node label whose first line (up to the first `<br>`) is one `**bold**` span, ignoring surrounding spaces, and which has at least one later line holding text renders in two tiers. Example: `q["**q-observe**<br/>250 push slots<br/>separate invocations"]`.
  - The first line is the title: SemiBold at the font size, drawn like any other line.
  - Every later line is a detail line, including wrapped continuations and empty lines between detail lines. A detail line is measured and drawn at 0.8 × the font size (11.2 px at 14 px) and wraps at the same maximum width; Markdown inside it keeps working at that size.
  - Each detail line is a `<tspan class="merlion-detail">` with explicit `x`/`y`, a `font-size` presentation attribute equal to the measured size and `fill` set to the muted default. The embedded style adds `#{id} .merlion-detail { fill: var(--merlion-node-detail, var(--merlion-muted, #7b7d81)); font-size: 11.2px; }` (the `color-mix` fallback inside `@supports`, like every mixed role). The size is a literal, not a token, because it must match the measurement, as with `--merlion-font-size`. The rule appears only in diagrams that have detail lines.
  - A source `color` (`classDef`, `style`) sets the fill of the `text` element, so it colours the title; detail lines keep `--merlion-node-detail`, which a host page or theme overrides to restyle them.
  - Labels that do not match render exactly as other labels. Edge labels and cluster titles never split into tiers. The outline and `<desc>` carry the plain text of every line.

## Links

`click <node> href "<url>" [_self|_blank]` wraps the node's group in `<a href="…">`. The URL is accepted only if, after removing ASCII tab, newline and carriage return (as browsers do before parsing), it is relative or its scheme, compared case-insensitively, is `https`, `http` or `mailto`. `_blank` adds `target="_blank" rel="noopener noreferrer"`. Any other URL is dropped with `W013 LinkRejected`. `click <node> <callback>` and `click <node> call <fn>()` are ignored with `I031 ClickCallbackIgnored`; the output never calls page JavaScript.

## Caller hooks

Hooks for maths, extended label formatting and icons ([ADR-0002](adr/0002-zero-runtime-dependencies.md)) return geometry, not markup: a width, a height, and a list of paths, each an SVG path `d` string plus a theme token or a colour from the accepted colour grammar. The core parses each `d` into commands and numbers and re-serialises it, so hook output is held to the same guarantees as the rest of the SVG.

## Ids and data attributes

- `{id}` is the `id_prefix` render option when given; it must match `[a-z][a-z0-9-]{0,31}`. Otherwise it is `m` + the first 8 hex characters of FNV-1a 64 over (source, options, palette digest when a palette is given, hint). The palette digest hashes its canonical serialisation, so a formatting-only change to the stylesheet leaves the id unchanged. The hash alone cannot separate two renders of identical input, and FNV is not collision-resistant, so any host placing more than one diagram on a page passes `id_prefix`; the rehype plugin does ([integrations.md](integrations.md#fractalboxdevmerlion-rehype)).
- Every internal id (markers, clip paths, gradients) is `{id}-…`.
- Node groups: `<g class="merlion-node" data-merlion-id="{source id}" data-merlion-rank="{0..15}">`. The rank is dominator depth for flowcharts and degree order otherwise, clamped to 15; it drives semantic zoom ([viewer.md](viewer.md)).
- Clusters: `<g class="merlion-cluster" data-merlion-id="…">` with members nested inside.
- Edges: `<g class="merlion-edge" data-merlion-from="…" data-merlion-to="…">`, with `data-merlion-back="true"` on reversed edges and `merlion-c-{name}` for each role given through the edge's id.
- Source ids in `data-merlion-*` values are escaped the same way as text. Where a source id becomes part of an XML id or of the layout hint, it is encoded into `[A-Za-z0-9-_]`: ASCII letters, digits and `-` stay, `_` becomes `__`, and every other byte of its UTF-8 form becomes `_` plus two lowercase hex digits. The encoding is injective, so distinct source ids never collide.

## Layout hint

`data-merlion-layout` records the layout for the next render ([layout.md](layout.md#stable-layout)):

```
v1;TB;0:a,b;1:c,d,e;2:f
```

That is: format version, direction, then `layer:comma-separated node ids in order`, with ids encoded as above, so `;`, `:` and `,` never appear inside an id. Layers are numbered as assigned in phase 2, before container fit inserts pseudo-layers, so a change in fit doesn't change the hint. The hint carries order only, never coordinates, so it survives changes to fonts and spacing.

## Text alternative

`<desc>` holds a deterministic outline of the graph, and the same outline is returned as plain text for `<details>` fallbacks and `llms-full.txt`:

```
Flowchart, left to right. 5 nodes, 5 edges.
Build: Source .md → Has mermaid?
Has mermaid? → Render SVG [yes]; → Pass through [no]
Render SVG → Cache
```

Clusters are listed as headings, with edges grouped by source node and edge labels in brackets. `accDescr` in the source replaces the outline in `<desc>` but not in the plain-text return. `<title>` is `accTitle`, else the front-matter `title`, else "{Type} diagram".
