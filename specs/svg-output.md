# SVG output

Every render returns one self-contained SVG string and a plain-text outline. Inlining the SVG in HTML needs no sanitiser, even when the Mermaid source is untrusted ([security.md](security.md)). The output never contains:

- `<script>`, `<foreignObject>`, `<iframe>`, `<image>`, `<use>`, `<animate>`, `<set>` or any other animation element;
- event-handler attributes (`on*`), `style` attributes built from source text, or `xlink:href`;
- an `href` outside the link rules in [Links](#links);
- external references: no `url()` other than `url(#{id}-…)`, no `@import`, no `@font-face` other than the embedded font in `font: "embed"` mode;
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

Every colour, font and stroke decision is set twice: as a presentation attribute carrying the default value (`fill="#f6f7f9"`), and as a CSS rule in the embedded `<style>` reading a custom property with the same fallback (`fill: var(--merlion-node-bg, #f6f7f9)`). CSS rules override presentation attributes, so in a browser the custom properties win. SVG renderers outside browsers support custom properties partially or not at all (librsvg, for example, resolves only the fallback of `var()` in colour properties). They either drop the CSS declaration and draw the presentation attribute or resolve the fallback, and both give the same default colour. A host page themes every diagram by setting the variables on `:root` or any ancestor, including under `prefers-color-scheme` or a `[data-theme]` selector. Switching theme never re-renders.

Two foundation tokens drive the rest. The other roles default to mixes of those two using `color-mix(in oklab, …)`, and each can be overridden on its own. The embedded style defines the mixed roles only inside `@supports (color: color-mix(in oklab, #000, #fff))`. A browser that supports custom properties but not `color-mix` would otherwise treat every use of a mixed role as invalid at computed-value time, which resets the property instead of falling back; outside the `@supports` block, each use site falls back to the literal default (`var(--merlion-node-bg, #f6f7f9)`). The literal default of a mixed role, used in presentation attributes and fallbacks, is the mix of the two foundation defaults, stored as a constant in the core.

| Token | Default |
|---|---|
| `--merlion-bg` | `#ffffff` |
| `--merlion-fg` | `#1f2328` |
| `--merlion-muted` | `color-mix(in oklab, var(--merlion-fg) 55%, var(--merlion-bg))` |
| `--merlion-line` | `color-mix(in oklab, var(--merlion-fg) 45%, var(--merlion-bg))` |
| `--merlion-surface` | `color-mix(in oklab, var(--merlion-fg) 4%, var(--merlion-bg))` |
| `--merlion-border` | `color-mix(in oklab, var(--merlion-fg) 22%, var(--merlion-bg))` |
| `--merlion-accent` | `#0969da` |
| `--merlion-node-bg` / `-node-border` / `-node-text` | surface / border / fg |
| `--merlion-node-detail` | muted (detail lines of title + detail node labels; see [Text](#text)) |
| `--merlion-edge` / `-edge-label-bg` | line / bg |
| `--merlion-cluster-bg` / `-cluster-border` | `color-mix(in oklab, var(--merlion-fg) 2%, var(--merlion-bg))` / border |
| `--merlion-series-1` … `--merlion-series-8` | Categorical palette for charts, pie and gantt sections |
| `--merlion-font` | `Inter, ui-sans-serif, system-ui, sans-serif` |
| `--merlion-font-size` | `14px` (must match the measured size; see [text-measurement.md](text-measurement.md)) |
| `--merlion-stroke` | `1.25px` |

`@fractalboxdev/merlion-themes` ships `merlion-themes.css`: light and dark defaults plus named themes. Every named theme is an original palette or one whose licence is recorded in [licensing.md](licensing.md).

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
- Source-style rules target the shapes and `text` elements inside the class (`#{id} .merlion-c-{name} text { … }`), which is more specific than the text reset below, so a source `font-weight` or `color` applies.
- `style` and `class` on a subgraph id style the cluster box and title only: the cluster carries `merlion-ss-{index}` for `style` and `merlion-cc-{name}` for each class, and their rules use child combinators (`#{id} .merlion-cc-{name} > .merlion-cluster-box`, `… > .merlion-cluster-title`), so member nodes and nested clusters keep their own colours.
- A fixed colour set in the source stays fixed in every theme; the parser emits `I030 FixedColour` so authors know.

### Embedded style

Every rule in the embedded `<style>` starts with the root's id selector (`#{id} .merlion-node > rect { … }`). An inline SVG's `<style>` applies to the whole HTML document, so an unscoped rule would restyle the host page; with the prefix, two diagrams from different Merlion versions on one page don't interfere either. The style never contains `:root`, `html`, `body`, element-only or universal selectors.

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

- `{id}` is the `id_prefix` render option when given; it must match `[a-z][a-z0-9-]{0,31}`. Otherwise it is `m` + the first 8 hex characters of FNV-1a 64 over (source, options, hint). The hash alone cannot separate two renders of identical input, and FNV is not collision-resistant, so any host placing more than one diagram on a page passes `id_prefix`; the rehype plugin does ([integrations.md](integrations.md#fractalboxdevmerlion-rehype)).
- Every internal id (markers, clip paths, gradients) is `{id}-…`.
- Node groups: `<g class="merlion-node" data-merlion-id="{source id}" data-merlion-rank="{0..15}">`. The rank is dominator depth for flowcharts and degree order otherwise, clamped to 15; it drives semantic zoom ([viewer.md](viewer.md)).
- Clusters: `<g class="merlion-cluster" data-merlion-id="…">` with members nested inside.
- Edges: `<g class="merlion-edge" data-merlion-from="…" data-merlion-to="…">`, with `data-merlion-back="true"` on reversed edges.
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
