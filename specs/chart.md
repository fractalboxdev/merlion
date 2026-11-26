# Charts

`render_chart` draws a data chart from a `merlion-chart/1` JSON spec: rows of values and the marks that draw them. Output follows the shared SVG contract ([svg-output.md](svg-output.md)): one self-contained SVG, labels as `<text>`, colours as CSS custom properties, `role="img"` with `<title>` and `<desc>`, no script, and byte-identical bytes from the native core and WASM. A chart is not Mermaid source; [ADR-0012](adr/0012-chart-spec.md) records why charts take JSON. Mermaid's own `xychart` header stays on the compatibility roadmap ([roadmap.md](roadmap.md)).

## Input

- The input is one UTF-8 JSON document (RFC 8259) of at most `limits.input_bytes` (1 MiB); a larger one fails with `E004`.
- Nesting deeper than `limits.nesting` (64) fails with `E020`.
- Invalid JSON fails with `E020` at the byte where reading stopped. A duplicate member name in one object fails with `E020`.
- A member the spec does not define fails with `E021` naming its JSON Pointer path (`/marks/0/colour`), so a misspelt member never draws a different chart.
- Every spec error carries its line, column and byte span in the input.

## Spec

| Member | Type | Required | Meaning |
|---|---|---|---|
| `chart` | the number `1` | yes | Format version |
| `title` | string, 1–200 characters | yes | The chart's accessible name, written into `<title>`. The chart draws no visible title: the host page draws the figure's frame (number, title, source, notes) as text it can wrap and select |
| `description` | string, at most 2,000 characters | no | Replaces the generated `<desc>` |
| `x`, `y` | axis object | yes | The two axes ([Axes](#axes)) |
| `data` | array of 1–10,000 row objects | yes | Each row maps at most 64 field names to a string, a number or `null` |
| `marks` | array of 1–16 mark objects | yes | Drawn in array order, except that `span` marks draw first ([Marks](#marks)) |
| `series` | `{ "domain": [...] }` | no | Series names in colour order, 1–8 of them |
| `legend` | boolean | no | Default: drawn when two or more series are drawn |
| `height` | number, 120–2,000 | no | Total SVG height in px for a chart whose `y` axis is not `band`; a band `y` axis sets the height from its categories |

## Axes

| Member | Applies to | Meaning |
|---|---|---|
| `type` | all | `"band"` (categories), `"linear"` (numbers) or `"time"` (UTC instants) |
| `label` | all | Axis title, at most 80 characters. The `y` title sits above the plot at the left edge; the `x` title sits below the tick labels at the right edge |
| `domain` | all | Band: the categories in drawing order, 1–500, each once. Linear or time: `[min, max]` with `min` below `max` |
| `zero` | linear | `true` includes zero in the computed domain |
| `format` | linear | `{ "decimals": 0–6, "prefix": ≤ 8 chars, "suffix": ≤ 8 chars, "group": boolean }` for tick labels; `group` (default `true`) separates thousands with `,` |
| `format` | time | A D3 time format string the Gantt axis accepts ([gantt.md](gantt.md#directives-and-configuration)), such as `"%b %Y"` |

- `x` and `y` must not both be `band`, and must not both be `time` (`E021`).
- Without `domain`, a linear axis spans the values drawn on it, widened to whole tick steps; a time axis spans the values drawn on it exactly. A single value widens to ±1, or ±10% of a nonzero value; a single instant widens to ±1 day.
- With `domain`, every value drawn on the axis must lie inside it, or the chart fails with `E022`. A chart never clips a mark.
- A band axis without `domain` lists categories in order of first appearance: marks in array order, rows in array order.

## Values

| Axis | A value is | Rejected with `E022` |
|---|---|---|
| Linear | A JSON number, or a string matching `-?[0-9]+(\.[0-9]+)?` | `"6,538.76"`, `"1e3"`, `"+1"`, `" 1"`, `"n/a"`, `"5%"` |
| Time | A string: `YYYY`, `YYYY-MM`, `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM[:SS]Z`, read as UTC | A JSON number, an impossible date, a time without `Z` |
| Band | A string, or a number's text as written | — |

- Every drawn item keeps the exact text its values were written as. The SVG carries it in `data-merlion-*` attributes and the outline prints it, so `"-2.40"` stays `-2.40` and a reader can check each mark against its row.
- A mark that names a field no row has fails with `E021`.
- A row whose field is missing or `null` for a mark is not drawn by that mark; the render adds one `I041` per mark with the count.

## Marks

| `type` | Members | Axes | Draws |
|---|---|---|---|
| `bar` | `x`, `y`; `series`, `label`, `tone` | One `band`, one `linear` | A bar from zero to the value. The value axis always includes zero; `zero: false` on it fails with `E021`. Bars of several series in one category sit side by side in series order |
| `line` | `x`, `y`; `series`, `label`, `tone`, `dash` | `y` linear | One 2 px polyline per series through its rows in `x` order |
| `area` | `x`, `y`; `y2`, `series`, `tone` | `y` linear | A filled region between `y` and `y2`, or between `y` and zero without `y2` |
| `dot` | `x`, `y`; `series`, `label`, `tone` | Any | A circle of radius 4.5 px with a 1.5 px ring in the background colour |
| `interval` | `x`, `y`, and exactly one of `x2`, `y2`; `series`, `label`, `tone`, `thickness` | The `x2` or `y2` axis linear or time | A segment from the value to the second value, with round caps |
| `rule` | Exactly one of `x`, `y`; `text`, `tone`, `dash`, `thickness` | That axis linear or time | A line across the whole plot at the value |
| `span` | `x` and `x2`, or `y` and `y2`; `text` | That axis linear or time | A shaded band across the whole plot between the values |

- A position member (`x`, `y`, `x2`, `y2`) is a field name, or `{ "value": … }` for a constant. A mark whose position members are all constants draws once; any field draws once per row.
- `series` is a field whose value names the item's series, or `{ "value": "<name>" }` naming one series for the whole mark. A mark with `series` must keep the default `tone`.
- `label` is a field; its value is drawn beside the item exactly as written. A bar labels outward from zero; an interval labels past its far end, to the right or above; a dot labels to the right, except that on a band `y` axis a dot at the low end of its row's dots and intervals labels to the left, so a dumbbell's labels sit outside its segment; a line labels its last point.
- `tone` is `"series"` (default), `"neutral"` (the `line` role, for context marks) or `"accent"` (the `accent` role).
- `text` is the note of a rule or span, at most 80 characters. A rule across `x` writes its note in a row above the plot, right of the line, or left of it when the right side is taken; a third note at the same place sits inside the plot. A rule across `y` writes its note above the line at the plot's right edge. A span writes its note inside its top-left corner.
- `thickness` is the stroke width in px, 1–40. An interval defaults to 2 px when `neutral`, else 45% of the band width up to 12 px; a rule defaults to 1.25 px.
- A member a mark type does not accept fails with `E021`, as does a mark missing a required member.

## Series and colour

- Series take colours `--merlion-series-1` … `-8` in order: `series.domain` when given, else first appearance in marks, then rows. A series name outside `series.domain` fails with `E022`.
- A ninth series fails with `E023`: the categorical palette has eight colours, and a generated colour would repeat or clash. Fold the smallest series into "Other" or split the chart.
- The legend lists every drawn series with a 10 px swatch, in series order, above the plot, wrapping to the chart width.
- Colour never carries meaning alone: with two or more series the legend names each one, and every item has a `<title>` naming its series and values.

## Layout

- The chart is `target_width` px wide (at least 240). The text of ticks, labels, notes, titles and the legend is drawn at 6/7 of `font_size` (12 px at the default 14 px) and measured with the same font tables as diagram labels ([text-measurement.md](text-measurement.md)).
- Margins come from measured text: the widest `y` tick label or category label sets the left margin, and a mark that labels beyond its end gets a gutter of its widest label on that side, so no label crosses the plot edge.
- A linear axis has about one tick per 56 px of height or 90 px of width, between 2 and 10, at the 1-2-5 steps of Heckbert's nice numbers. A tick label is printed from the integer multiple of its step, so it is exact. A time axis uses the Gantt automatic interval for one tick per 90 px, aligned to UTC boundaries.
- A band axis divides its length evenly among categories. A bar fills 72% of its category's step, at most 48 px per series on a vertical chart and 22 px per series on a horizontal one; bars of one category are separated by a 2 px gap. A horizontal chart gives each category at least 28 px. Category labels wrap: below a vertical chart to their step, beside a horizontal chart to 180 px or 32% of the width.
- Without `height`, a chart whose `y` axis is not `band` has a plot of `clamp(round(0.45 × width), 180, 380)` px.
- Grid lines run from each tick of a linear or time axis across the plot. A zero line runs across the value axis of bars and areas, else across a linear `y` axis that spans zero.

## SVG

- The root has `class="merlion merlion-chart"` and `data-merlion-chart="1"`. `<title>` holds `title`; `<desc>` holds `description` or a generated sentence naming the mark types, axes, row count and series count.
- Each mark is a `<g class="merlion-chart-mark merlion-chart-{type}" data-merlion-mark="{index}">`. Each item inside is a `<g class="merlion-chart-item …">` with a `<title>` tooltip and these attributes:

| Attribute | Value |
|---|---|
| `data-merlion-row` | The row index, absent for a constant |
| `data-merlion-x`, `-y`, `-x2`, `-y2` | The value as written in the spec |
| `data-merlion-series` | The series name |
| `data-merlion-points` | On a line or area path: the number of points |

- Grid lines carry `data-merlion-value` with the tick label; the zero line carries `data-merlion-value="0"`. Tick labels are `merlion-chart-tick`, item labels `merlion-chart-label`, notes `merlion-chart-note`, axis titles `merlion-chart-title`.
- Items paint with `currentColor`. The class `merlion-chart-s{k}` sets `color` from `--merlion-series-{k}`; `merlion-chart-neutral` from `--merlion-line`; `merlion-chart-accent` from `--merlion-accent`. Every element also carries its literal colour as a presentation attribute. A `palette` bakes literals and a dark variant exactly as for diagrams.
- The outline is `Chart. {title}. {n} rows.`, the series list, then one line per row with the fields the marks read, band fields first, joined by ` | `, `-` for a missing value.

## Diagnostics

| Code | Severity | Meaning |
|---|---|---|
| `E020` | Error | The input is not valid JSON, nests deeper than the limit, or repeats a member name |
| `E021` | Error | The spec's shape: an unknown or missing member, a wrong type, a mark on the wrong axes, a field no row has |
| `E022` | Error | A value: not a decimal or date, outside an authored domain, a category or series outside its domain |
| `E023` | Error | More than eight series |
| `I041` | Info | Rows a mark skips for a missing or `null` value |

`strict` promotes warnings and repairs to errors as for diagrams; charts emit none of either.

## Limits

| Limit | Value |
|---|---|
| Input / nesting | `limits.input_bytes` / `limits.nesting` |
| Rows / fields per row | 10,000 / 64 |
| Marks | 16 |
| Categories per band axis | 500 |
| Series | 8 |
| Ticks per axis | 64 |
| Fuel | One unit per drawn item plus one per mark, against `fuel` |

## Out of scope

Stacked bars, facets and small multiples, a second value axis, logarithmic scales, and annotations other than `rule` and `span`. A second value axis is excluded by design: two measures with different units get two charts or an index to a common base.

## Validation

- `crates/merlion-render/tests/chart_acceptance.rs` renders every fixture in `tests/fixtures/chart/` strictly, checks byte-identical repeats, source values in `data-merlion-*` attributes and the outline, the zero line, the legend rule, and each error code with its JSON path.
- `packages/merlion-wasm/test/glue.test.mjs` renders a fixture through `renderChart` from a string and from an object, and checks errors through `checkChart`.
- `crates/merlion-cli/tests/cli.rs` checks that `merlion render --chart` writes the same bytes as the core.
