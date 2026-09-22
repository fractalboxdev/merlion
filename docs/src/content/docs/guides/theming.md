---
title: Theming
description: Merlion colours are CSS custom properties. Light, dark and custom themes switch through CSS alone, on the page or on any container, with no re-render.
---

Every colour, font and stroke in a Merlion SVG is set twice: as a presentation attribute carrying the default (`fill="#f6f7f9"`) and as a rule in the embedded `<style>` that reads a `--merlion-*` custom property with the same fallback. A browser applies the custom property; a renderer without CSS draws the attribute. A page themes every diagram by setting variables on `:root` or any ancestor, and switching theme never re-renders.

The diagrams on this site follow the theme switch in the header: Starlight sets `data-theme="light"` or `"dark"` on `<html>`, and `merlion-themes.css` keys on exactly that attribute.

```mermaid
flowchart LR
  accTitle: How a theme reaches a diagram
  page["**page CSS**<br/>:root, [data-theme]"] --> vars(["--merlion-* variables"])
  vars --> style["**embedded style**<br/>var(--merlion-node-bg, #f6f7f9)"]
  attr["**presentation attribute**<br/>fill #f6f7f9"] e1@-.->|no CSS| shape[Node shape]
  style --> shape
  class page style
  class vars accent
  class attr muted
  class shape output
  class e1 async
```

## Tokens

Two foundations drive the rest. The other roles default to `color-mix(in oklab, …)` of those two, and each can be set on its own.

| Token | Default | Drives |
|---|---|---|
| `--merlion-bg` | `#ffffff` | Foundation: background, edge-label chips |
| `--merlion-fg` | `#1f2328` | Foundation: text |
| `--merlion-muted` | fg 55% into bg | `muted` role, detail lines |
| `--merlion-line` | fg 45% into bg | Edges |
| `--merlion-surface` | fg 4% into bg | Node fill |
| `--merlion-border` | fg 22% into bg | Node and cluster borders |
| `--merlion-accent` | `#0969da` | `accent` role |
| `--merlion-ok` / `-warn` / `-danger` | `#1a7f37` / `#9a6700` / `#cf222e` | `ok`, `warn`, `danger` and `failure` roles |
| `--merlion-node-bg` / `-node-border` / `-node-text` | surface / border / fg | Nodes |
| `--merlion-node-detail` | muted | Detail lines of [title + detail labels](/guides/labels/) |
| `--merlion-edge` / `-edge-label-bg` | line / bg | Edges and their label chips |
| `--merlion-cluster-bg` / `-cluster-border` | fg 2% into bg / border | Subgraphs |
| `--merlion-series-1` … `-8` | categorical palette | Charts, pie and gantt sections |
| `--merlion-font` | `Inter, ui-sans-serif, system-ui, sans-serif` | Label font family |
| `--merlion-font-size` | `14px` | Must match the measured size |
| `--merlion-stroke` | `1.25px` | Stroke width |
| `--merlion-tone` / `--merlion-dash` | unset | Per-element tone and dash of a [role](/guides/roles-and-stylesheets/) |

The mixed roles live inside `@supports (color: color-mix(in oklab, #000, #fff))`; a browser without `color-mix` draws their literal defaults. The full contract is in [svg-output.md](/reference/specs/svg-output/#theming).

## The shipped themes

`@fractalboxdev/merlion-themes` ships `merlion-themes.css`. It sets the foundations, the accent, the three role tones and the series palette for each theme:

| Selector | Theme |
|---|---|
| `:root`, `[data-theme="light"]` | Light, the default |
| `@media (prefers-color-scheme: dark)` on `:root:not([data-theme])` | Dark, when the page sets no `data-theme` |
| `[data-theme="dark"]` | Dark, forced |
| `[data-theme="harbour"]` | Warm paper with navy ink and a brick accent |
| `[data-theme="lantern"]` | Plum night with parchment text and an amber accent |

Link it once per page, before any stylesheet of your own:

```html
<link rel="stylesheet" href="/node_modules/@fractalboxdev/merlion-themes/merlion-themes.css" />
```

The Astro integration imports it on every page ([Getting started](/getting-started/)). A `data-theme` attribute on `<html>` themes the page; the same attribute on any ancestor of a diagram themes that subtree.

## Per-container themes

Custom properties inherit, so a wrapper element themes the diagrams inside it and nothing else:

```html
<div data-theme="lantern">…diagram…</div>

<div style="--merlion-bg: #0b1d2a; --merlion-fg: #d8e6f0; --merlion-accent: #f2b134">
  …diagram…
</div>
```

Setting `--merlion-bg` and `--merlion-fg` alone is enough for a coherent theme: every mixed role follows. Override a single role (`--merlion-edge`, `--merlion-node-bg`) to break one away from the mix.

## Fonts

Text is measured at build time against committed Inter metrics, so the page must draw the font the layout was measured with. The `font` option picks how:

| Mode | The SVG carries | Use it for |
|---|---|---|
| `link` (default) | Nothing inline; `font-family: var(--merlion-font, Inter, …)` | Web pages that load `merlion-font.css` (Inter subsets under the family `Merlion Inter`) or already serve Inter |
| `embed` | The Inter WOFF2 subset as a `data:` URI in its `<style>` | Standalone files, image embeds |
| `system` | Metrics for `system-ui` with extra padding for up to ±6% width drift | Pages that must not load a web font |

```sh
target/release/merlion render diagram.mmd --font embed -o diagram.svg
```

The Astro integration loads `merlion-font.css` by default. The rehype plugin warns once per build in `link` mode unless its `fontCss` option says the page loads the font.

## Standalone SVG

Inline SVG follows the page's CSS. A standalone file (an `<img>`, a GitHub embed, librsvg) sees no page CSS, so it draws its presentation attributes. Bake a theme into those attributes with a stylesheet: `render --css site.css --theme dark` writes the dark literals, and `--auto-dark dark` adds a `prefers-color-scheme: dark` block for `<img>` embeds. See [Roles and stylesheets](/guides/roles-and-stylesheets/#standalone-svg).
