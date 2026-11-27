# Quadrant charts

`quadrantChart` renders a static SVG of four labelled quadrants, two labelled axes and points placed by coordinates from 0 to 1, through the shared core, CLI, WASM and Markdown adapters. The layout is a fixed geometry of its own; it uses no graph layout.

## Reference and compatibility

The reference is Mermaid `mermaid@12.0.0`, commit `98a0945418c76238f15df2afaddbba4272656c3b`:

- [Quadrant chart documentation](https://github.com/mermaid-js/mermaid/blob/98a0945418c76238f15df2afaddbba4272656c3b/packages/mermaid/src/docs/syntax/quadrantChart.md) describes the syntax, configuration and theme variables.
- [Grammar](https://github.com/mermaid-js/mermaid/blob/98a0945418c76238f15df2afaddbba4272656c3b/packages/mermaid/src/diagrams/quadrant-chart/parser/quadrant.jison) defines the statements and the coordinate tokens.
- [Database](https://github.com/mermaid-js/mermaid/blob/98a0945418c76238f15df2afaddbba4272656c3b/packages/mermaid/src/diagrams/quadrant-chart/quadrantDb.ts) validates point styles and reads the configuration and theme variables.
- [Builder](https://github.com/mermaid-js/mermaid/blob/98a0945418c76238f15df2afaddbba4272656c3b/packages/mermaid/src/diagrams/quadrant-chart/quadrantBuilder.ts) computes the bands, quadrant boxes, label positions and points that the [renderer](https://github.com/mermaid-js/mermaid/blob/98a0945418c76238f15df2afaddbba4272656c3b/packages/mermaid/src/diagrams/quadrant-chart/quadrantRenderer.ts) draws.

`bench/corpus/compat-quadrant` holds 24 unique diagrams: the demos page, the syntax documentation, mermaid's examples package, the template literals of the quadrant parser's and database's unit tests, and every `e2e/diagrams/quadrant-chart/*.mmd` fixture with a statement after its header. mermaid has no quadrant rendering spec. Selection depends on paths and headers, never on Merlion's support, so sources mermaid rejects stay in the corpus. SHA-256 deduplication keeps the first source-path occurrence; the manifest records paths, Git blobs and block indices, and `bench/NOTICES.md` carries the MIT notice. `pnpm --dir bench bench fetch --kind quadrant` refreshes only this corpus.

`pnpm --dir bench bench quadrant` renders every source with Merlion and with pinned mermaid in headless Chromium and compares the two drawings:

- the title, and each quadrant's label, by the quadrant its box occupies;
- where each quadrant label sits: in the top band of its quadrant or at its vertical centre;
- each axis label's text, the side of the plot it sits on, and whether it is centred on its half of the axis or starts at the low end;
- every point, matched by label: its coordinates recovered from the circle's position inside the plot (±0.002), its radius, its stroke width, its fill and stroke where mermaid draws a hex colour, and whether its label sits below it;
- each quadrant fill the source sets through `themeVariables`.

Default colours are each renderer's theme and are not compared. A text compares with whitespace removed, and mermaid's one-sided axis marker `⟶` compares equal to Merlion's `→`. A source passes when both render and the drawings agree; rejected sources stay in the denominator and two rejections earn no credit.

The run on the pinned corpus matches 23 of 24 sources (95.8%), per [`bench/results/2026-11-27-quadrant-baseline.md`](../bench/results/2026-11-27-quadrant-baseline.md). The one difference is `unit-quadrant-jison-04`, whose `classDef constructor fill:#ff0000` mermaid rejects; Merlion drops the `fill` with `W010` and renders. CI runs `pnpm bench quadrant --min-compat 95`: the measured 23 of 24 clears it, and any further difference fails it. `--min-compat` gates the unrounded percentage over the complete corpus and rejects `--limit`.

## Syntax

The header is `quadrantChart`, matched case-sensitively as mermaid's detector matches it. Statements end at a line break or at `;` outside a double-quoted string; `title`, `accTitle` and `accDescr` take the rest of their line. Keywords match in any case, as mermaid's lexer reads them. `%%` starts a comment outside a quoted string.

| Statement | Meaning |
|---|---|
| `title <text>` | The chart title, taken verbatim to the end of the line |
| `x-axis <low> --> <high>` | Left and right axis labels |
| `x-axis <low> -->` | The left label alone, followed by ` →` |
| `x-axis <low>` | The left label alone |
| `y-axis …` | The same three forms for the bottom and top labels |
| `quadrant-1` … `quadrant-4 <text>` | Labels of the top-right, top-left, bottom-left and bottom-right quadrants |
| `<name>[:::<class>]: [x, y] [<styles>]` | A point |
| `classDef <class> <styles>` | Styles of a point class |
| `accTitle: <text>`, `accDescr: <text>`, `accDescr { … }` | The accessible title and description |

The axis delimiter is two or more `-` followed by `>`. A text in double quotes loses its quotes and may contain `:`, `;`, `-->` and brackets; `` "`…`" `` also loses its backticks. Every text then follows the label normalisation of [parser.md](parser.md#labels-and-entity-codes). A later statement of the same kind replaces an earlier one. A statement that is none of these fails with `E002`, which names the statements expected.

### Points

`x` runs from 0 at the left to 1 at the right and `y` from 0 at the bottom to 1 at the top. A coordinate is a plain decimal. A value outside 0–1 fails with `E024` at the value. A value in range that mermaid's lexer cannot read — mermaid reads only `0`, `1` and `0.` followed by digits — is rewritten with `R021` to the form it reads: `1.0` becomes `1`, `.5` and `+0.5` and `00.5` become `0.5`. The repair carries its fix, so `merlion check --fix` makes the source portable; under `strict` it is an error. A point without its name, its `]` or its second coordinate fails with `E002`.

### Point styles

A point takes styles after its coordinates, and a class takes them in `classDef`, as comma-separated `<key>: <value>` pairs:

| Key | Accepted value |
|---|---|
| `radius` | A number from 0 to 100, optionally followed by `px` |
| `color` | The point's fill: the colour grammar of [svg-output.md](svg-output.md#source-styles-classdef-style-linkstyle), or three or six hex digits without `#` |
| `stroke-color` | The point's outline, in the same grammar |
| `stroke-width` | A number from 0 to 20, optionally followed by `px` |

Any other key or value is dropped with `W010`; mermaid rejects the whole chart instead. A colour sets `I030`. A class name outside `[A-Za-z_][A-Za-z0-9_-]{0,63}` is dropped with `W011`. A point's own styles win over its class's, and the class's over the theme, as in mermaid; a `classDef` may follow the points that use it.

A class that no `classDef` declares is a role ([svg-output.md](svg-output.md#roles)). The tone roles `accent`, `ok`, `warn`, `danger`, `muted` and `store` draw the point's fill and outline in their tone, and a stylesheet role tone reaches a point the same way. A `classDef` of a built-in role's name replaces the role, as on a flowchart.

## Configuration

`config.quadrantChart` in front matter or `%%{init}%%` accepts mermaid's keys:

| Key | Accepted | Default |
|---|---|---|
| `chartWidth`, `chartHeight` | 100–4000 | 500 |
| `titlePadding` | 0–200 | 10 |
| `quadrantPadding`, `quadrantTextTopPadding`, `xAxisLabelPadding`, `yAxisLabelPadding`, `pointTextPadding` | 0–200 | 5 |
| `titleFontSize` | 1–100 | 8/7 × the font size, SemiBold |
| `quadrantLabelFontSize`, `xAxisLabelFontSize`, `yAxisLabelFontSize` | 1–100 | The font size |
| `pointLabelFontSize` | 1–100 | 6/7 × the font size |
| `quadrantInternalBorderStrokeWidth`, `quadrantExternalBorderStrokeWidth` | 0–20 | 1, 2 |
| `pointRadius` | 0–100 | 5 |
| `xAxisPosition` | `top`, `bottom` | `top` |
| `yAxisPosition` | `left`, `right` | `left` |
| `useMaxWidth` | `true`; `false` has no effect, because the SVG always scales to its container | `true` |

A number is a JSON number or a YAML scalar in the plain decimal grammar. A value outside its range and an unknown key are ignored with `W016`.

### Theme variables

The fifteen quadrant keys of `themeVariables` — `quadrant1Fill` … `quadrant4Fill`, `quadrant1TextFill` … `quadrant4TextFill`, `quadrantPointFill`, `quadrantPointTextFill`, `quadrantXAxisTextFill`, `quadrantYAxisTextFill`, `quadrantInternalBorderStrokeFill`, `quadrantExternalBorderStrokeFill` and `quadrantTitleFill` — take a colour in the point style grammar and replace the themed colour of their element with a fixed one, reported once with `I030` ([ADR-0013](adr/0013-quadrant-theme-variables.md)). A value outside the grammar is ignored with `W016`. Any other key of `themeVariables` is ignored with `I011`, as for every diagram. A `themeVariables` holding only quadrant keys reports `I011` on a diagram of another type.

## Layout

The layout follows mermaid's builder with every label measured from the font tables ([text-measurement.md](text-measurement.md)) instead of assumed one font size tall:

1. The chart is `chartWidth` × `chartHeight`. A container narrower than the chart, but no narrower than 200 px, narrows it and scales its height by the same factor.
2. The title band is the title's measured height plus `titlePadding` above and below. The x-axis band is the tallest x label plus `xAxisLabelPadding` on each side; the y-axis band is the thickest y label plus `yAxisLabelPadding` on each side. A band exists only when it has text.
3. The x-axis band sits below the plot whenever the chart has points, and otherwise at `xAxisPosition`. The y-axis band sits at `yAxisPosition`.
4. The plot is what the bands and `quadrantPadding` leave, at least 40 px each way; quadrant 1 is its top-right half-by-half box, 2 the top left, 3 the bottom left and 4 the bottom right.
5. A quadrant label wraps to its quadrant's width less `quadrantPadding` on each side. With points, it sits `quadrantTextTopPadding` below its quadrant's top; without, at its quadrant's centre. The plot grows taller when the measured label and its padding exceed a quadrant's height.
6. Two labels on an axis centre on their halves; a lone label starts at the axis's low end: the left of the plot, or its bottom for a y label. y labels are laid out horizontally and drawn rotated −90° about their centres.
7. A point's centre is `left + x × width`, `top + (1 − y) × height`. Its label hangs below the point, its top `pointTextPadding` below the centre or clear of the circle and its outline when they reach further. The label stays inside the chart's width, and the chart grows taller when a label would leave it. Circle and stroke extents contribute to the viewport; the complete drawing translates when a painted extent would start at a negative coordinate, preserving point coordinates within the plot.

Labels wrap to the room they have; mermaid draws each on one line. Fuel covers one unit per label byte, per point, per class declaration, per applied point class and per palette tone in each table. Each linear class lookup reserves its worst-case comparison count before the scan; role selection and painting reserve their class scans before drawing. Exhausting fuel fails with `E004` before anything is drawn.

## SVG

The root has `class="merlion merlion-quadrant"`, the shared accessible name and description, and `data-merlion-layout="v1;QUADRANT;0:…"` listing the encoded point labels. Inside the diagram group, in drawing order:

| Element | Classes and attributes |
|---|---|
| Quadrant | `<g class="merlion-quadrant-region merlion-quadrant-{n}" data-merlion-quadrant="{n}">` holding `rect.merlion-quadrant-box` and its label |
| Borders | `line.merlion-quadrant-border` (outer) and `line.merlion-quadrant-divider` (the two dividers) inside `g.merlion-quadrant-borders` |
| Point | `<g class="merlion-quadrant-point [merlion-c-{class}] [merlion-ps-{i}]" data-merlion-id data-merlion-x data-merlion-y id="{id}-point-{i}">` holding a `<title>`, `circle.merlion-quadrant-dot` and its label |
| Axis label | `<g class="merlion-quadrant-axis" data-merlion-axis="x\|y" data-merlion-end="low\|high">`, with `transform="rotate(-90 …)"` on a y label |
| Title | `text.merlion-quadrant-title` |

Points draw in reverse declaration order, so an earlier point stays on top where two overlap, as in mermaid. `data-merlion-x` and `data-merlion-y` carry the source coordinates as the shortest decimal that reads back exactly. Every label is `text.merlion-label.merlion-quadrant-text` with a kind class: `merlion-quadrant-label`, `merlion-quadrant-axis-x`, `merlion-quadrant-axis-y`, `merlion-quadrant-point-label` or `merlion-quadrant-title`. A label drawn at a size other than the font size carries that size as a literal `font-size` rule and attribute, as the measurement needs; the title also carries `font-weight` 600.

Colours follow [svg-output.md](svg-output.md#theming): a presentation attribute with the literal and an id-scoped rule reading the token with the same literal as fallback.

| Element | Colour |
|---|---|
| Quadrant 1–4 box | `color-mix(in oklab, var(--merlion-accent) {12, 9, 6, 3}%, var(--merlion-bg))`, inside `@supports`; outside it the literal mix |
| Borders and dividers | `--merlion-border` |
| Labels and title | `--merlion-fg` |
| Point fill and outline | `var(--merlion-tone, var(--merlion-accent))` |

A palette replaces every literal and adds the dark block, as for the other types. Role rules (`.merlion-c-{role}>.merlion-quadrant-dot`), then `classDef` rules reading `--merlion-c-{class}-fill` and `-stroke`, then a point's own style rules (`.merlion-ps-{i}>.merlion-quadrant-dot`, literals) follow in that order at equal specificity, so the later wins. Each `themeVariables` colour replaces its element's token with the literal. The per-element token reset covers the point groups, so a tone set on an ancestor never reaches a point.

mermaid 12.0.0's default theme computes its point colour as `hsl(240, 100%, NaN%)`, an invalid paint: its points draw in the inherited text colour, and a point with `stroke-width` and no `stroke-color` draws no outline. Merlion draws both in the accent. mermaid draws a bare-hex point colour such as `ff0000` as an invalid paint as well; Merlion reads it as `#ff0000`.

## Text alternative

`<desc>` holds the outline, which the render also returns:

```
Quadrant chart. 6 points. Reach and engagement of campaigns
x-axis: Low reach → High reach
y-axis: Low engagement → High engagement
Quadrant 1, top right: Expand
…
Spring launch: x 0.3, y 0.6, quadrant 2 (Promote)
```

A point on a dividing line (`x` or `y` exactly 0.5) names no quadrant. `accDescr` replaces the outline in `<desc>` but not in the returned outline. `<title>` is `accTitle`, else the title, else "Quadrant chart".

## Validation

- `crates/merlion-render/tests/parse_quadrant.rs` covers every statement, case-insensitive keywords, `;` and quoted text, point styles and classes, `E024`, `R021` and its fix, `E002` locations, configuration keys and `themeVariables`.
- `crates/merlion-render/tests/svg_quadrant.rs` checks the layout against the builder's rules, the SVG structure and accessible text, escaping, colour precedence through the CSS cascade, `themeVariables` literals, baked palettes against the page CSS in light and dark, deterministic bytes and the fuel bound; `quadrant_acceptance.rs` renders every fixture in `crates/merlion-render/tests/fixtures/quadrant/` strictly.
- `pnpm --dir bench bench determinism --corpus compat-quadrant` and `--corpus quadrant` compare native and WASM bytes in every font mode, in CI.
- `pnpm --dir bench bench quadrant --min-compat 95` gates compatibility in CI.
