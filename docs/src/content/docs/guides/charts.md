---
title: Charts
description: Draw bar, line, area, dot, interval, rule and span charts from a JSON spec of rows, with the same fonts, themes and accessibility as Merlion diagrams.
---

A chart is data, so Merlion reads it as data: a `merlion-chart/1` JSON spec of rows, and marks that name the fields to draw. Each value is written once. The bar, its label, its tooltip and the text outline all show the same text, and the SVG carries it in `data-merlion-*` attributes so anyone can check a mark against its row. The full contract is in [specs/chart.md](https://github.com/fractalboxdev/merlion/blob/main/specs/chart.md).

## A dumbbell, step by step

Six teams' scores in January and June. The rows hold both values; three marks draw them:

```json
{
  "chart": 1,
  "title": "Scores fell most where the baseline was highest",
  "x": { "type": "linear", "label": "Score" },
  "y": { "type": "band" },
  "data": [
    { "team": "Atlas", "jan": 82, "jun": 61 },
    { "team": "Birch", "jan": 77, "jun": 70 },
    { "team": "Delta", "jan": 59, "jun": 41 }
  ],
  "series": { "domain": ["January", "June"] },
  "marks": [
    { "type": "interval", "y": "team", "x": "jan", "x2": "jun", "tone": "neutral" },
    { "type": "dot", "y": "team", "x": "jan", "series": { "value": "January" } },
    { "type": "dot", "y": "team", "x": "jun", "series": { "value": "June" }, "label": "jun" }
  ]
}
```

- `y` is a `band` axis, so each team gets a row; `x` is `linear`, so its domain covers every value drawn on it, widened to whole tick steps.
- The `interval` draws a grey segment from `jan` to `jun`. `tone: "neutral"` keeps context marks out of the series colours.
- Each `dot` mark names one series with `{ "value": … }`. Two series draw a legend; the `label` field writes June's value beside its dot, on the outer side of the segment.
- `title` becomes the SVG's `<title>`. The chart draws no visible title: the page around it does, where the text can wrap.

Render it:

```sh
merlion render --chart teams.json -o teams.svg
```

```js
import { initSync, renderChart } from "@fractalboxdev/merlion-wasm";
const { svg, outline, diagnostics } = renderChart(spec, { width: 640 });
```

`renderChart` accepts the spec as an object or as JSON text.

## Marks

| Mark | Draws |
|---|---|
| `bar` | A bar from zero on one band and one linear axis; several series sit side by side |
| `line`, `area` | A series through its rows in `x` order; an area fills to zero or to `y2` |
| `dot` | A point on any axes |
| `interval` | A segment between two values: ranges, dumbbells, scenario bands |
| `rule` | A reference line across the plot, with an optional note |
| `span` | A shaded window across the plot, with an optional note |

## Values that fail loudly

A chart that draws a wrong number is worse than one that does not draw. These fail with an error naming the JSON path:

- `"6,538.76"` on a linear axis (`E022`): write `"6538.76"`, and set `format` for thousands separators on the ticks.
- A field no row has, such as `"y": "chnage"` (`E021`).
- A member the spec does not define, such as `"colour"` (`E021`).
- A value outside an authored `domain` (`E022`): Merlion never clips a mark.
- A ninth series (`E023`): fold the smallest into "Other" or split the chart.

`merlion check --chart` and `checkChart` report every problem at once, each with its line and column.
