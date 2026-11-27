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
| `bar` | A bar from zero on one band and one linear axis; several series sit side by side, or stack with `stack` |
| `line`, `area` | A series through its rows in `x` order; an area fills to zero or to `y2`, or stacks with `stack` |
| `dot` | A point on any axes |
| `interval` | A segment between two values: ranges, dumbbells, scenario bands |
| `rule` | A reference line across the plot, with an optional note |
| `span` | A shaded window across the plot, with an optional note |

## Stacks

`"stack": true` on a `bar` or an `area` stacks its series: you write each part, and Merlion sums them. Series stack in the order of `series.domain`. Values at or above zero stack up from zero, negative values down from it. Each segment's tooltip, `data-merlion-*` attributes and outline line keep the value you wrote, never the running sum.

### A stacked area with a reference line

Monthly costs by category, with a dashed budget line on the same axis and a shaded window:

```json
{
  "chart": 1,
  "title": "Monthly costs stayed under budget",
  "x": { "type": "time", "format": "%b %Y" },
  "y": { "type": "linear", "label": "Cost, $k per month" },
  "data": [
    { "month": "2026-01", "category": "Staff", "cost": "118.0" },
    { "month": "2026-01", "category": "Rent", "cost": "38.0" },
    { "month": "2026-02", "category": "Staff", "cost": "118.9" },
    { "month": "2026-02", "category": "Rent", "cost": "46.5" },
    { "month": "2026-01", "budget": "180.0", "name": "Budget" },
    { "month": "2026-02", "budget": "180.0", "name": "Budget" }
  ],
  "series": { "domain": ["Staff", "Rent"] },
  "marks": [
    { "type": "span", "x": { "value": "2026-01-15" }, "x2": { "value": "2026-02" }, "text": "Office move" },
    { "type": "area", "x": "month", "y": "cost", "series": "category", "stack": true },
    { "type": "line", "x": "month", "y": "budget", "tone": "neutral", "dash": true, "label": "name" }
  ]
}
```

- Every series needs a value at every `x` of the stack. A missing month fails with `E022` naming the series and the month; write `0` where a series has nothing.
- The line is not stacked: it reads its own rows, and its `label` names it at its last point.

### Horizontal stacked bars with totals

Hours per team, split by activity, with each team's total beyond its bar:

```json
{
  "chart": 1,
  "title": "Build work takes the largest share of every team's hours",
  "x": { "type": "linear", "label": "Hours per week" },
  "y": { "type": "band" },
  "data": [
    { "team": "Platform", "activity": "Design", "hours": "12.5" },
    { "team": "Platform", "activity": "Build", "hours": "48.0" },
    { "team": "Platform", "activity": "Test", "hours": "16.25" },
    { "team": "Payments", "activity": "Design", "hours": "9.0" },
    { "team": "Payments", "activity": "Build", "hours": "36.5" },
    { "team": "Payments", "activity": "Test", "hours": "21.0" }
  ],
  "series": { "domain": ["Design", "Build", "Test"] },
  "marks": [
    { "type": "bar", "y": "team", "x": "hours", "series": "activity", "stack": true, "total": true }
  ]
}
```

- `total` prints the exact sum of the written values, `76.75` and `66.50` here: every total takes as many decimals as the most precise value in the mark, and the sum never passes through floating point.
- Segments sit 2 px apart, and only the outer end of each stack is rounded.
- A stacked bar takes no `label`: hover a segment for its value, or read the outline.

## Values that fail loudly

A chart that draws a wrong number is worse than one that does not draw. These fail with an error naming the JSON path:

- `"6,538.76"` on a linear axis (`E022`): write `"6538.76"`, and set `format` for thousands separators on the ticks.
- A field no row has, such as `"y": "chnage"` (`E021`).
- A member the spec does not define, such as `"colour"` (`E021`).
- A value outside an authored `domain` (`E022`): Merlion never clips a mark.
- A ninth series (`E023`): fold the smallest into "Other" or split the chart.
- A stack that reaches past an authored `domain` (`E022`): the message names the stack and the sum it reaches.
- `stack` without `series`, or on an area with `y2` (`E021`).

`merlion check --chart` and `checkChart` report every problem at once, each with its line and column.
