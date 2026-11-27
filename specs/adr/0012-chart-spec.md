# ADR-0012: Charts from a JSON spec rendered by the core

- **Status:** Proposed
- **Date:** 2026-11-26

## Context

Reports and research notes need data charts — ranked bars, dumbbells, scenario ranges, time series with reference lines and shaded windows — with the same properties Merlion gives diagrams: byte-identical output on the server and in the browser, theming by CSS custom properties, `<text>` labels measured from committed font tables, and an accessible name, description and text outline. Two further requirements are specific to numbers:

- **Accuracy.** A value is written once and every mark, label, tooltip and outline line drawn from it shows that text. A reader, or a test, can check a mark against its source row.
- **Portability.** A chart source means the same thing wherever it is read.

Mermaid 12.0.0's `xychart` draws `bar` and `line` plots over one band or linear x axis and one y axis, with values as JavaScript numbers ([xychart syntax](https://mermaid.js.org/syntax/xyChart.html); `PlotData` in mermaid 12.0.0's `xychart/chartBuilder/interfaces.d.ts`). It has no ranges, reference lines, shaded spans, dots or per-item labels from data.

## Decision

The core renders a second input format, the `merlion-chart/1` JSON spec ([chart.md](../chart.md)), through `render_chart` and `check_chart`, the WASM exports `render_chart` and `check_chart`, the JS functions `renderChart` and `checkChart`, and `merlion render --chart`. The spec holds rows of values and marks that name fields; linear values are JSON numbers or decimal strings, kept as written. The chart pipeline reuses the core's text measurement, SVG emitter, theme tokens, palette baking and fuel. Mermaid sources and their compatibility contract are unchanged, and `xychart` stays in M3 for compatibility.

## Options considered

| Option | Buys | Costs |
|---|---|---|
| A Mermaid-style header with chart statements in the Merlion parser | One fence for diagrams and charts | A dialect: the same fence is a parse error on GitHub, in Obsidian and in mermaid itself, and a later Mermaid header of the same name would mean something else. Values become source text an author retypes, which is where wrong numbers come from. Each new mark needs new grammar |
| Mermaid `xychart` only | Renders everywhere Mermaid does | Bar and line only; no ranges, reference lines, spans or dots; values as floats, so `-2.40` prints as `-2.4` |
| **JSON spec rendered by the core** | Rows and fields: a value is written once, validated against a schema with JSON Pointer paths, and carried into the SVG as written; the core's determinism, metrics, theming and accessibility apply unchanged | A second input format to maintain; not Mermaid, so no other renderer draws it; GitHub shows the spec as code |
| A third-party grammar: Vega-Lite with vega's headless renderer or vl-convert; Observable Plot with a DOM shim; ECharts' SVG server renderer | Mature grammars and many chart types | Runtime JavaScript (vl-convert embeds a JavaScript engine), so the `no_std` core cannot call them and the zero-dependency rule fails ([ADR-0002](0002-zero-runtime-dependencies.md)). Each brings its own text measurement, so label widths differ from the diagrams on the same page; vega estimates widths from character counts without a canvas. Colours are literals or JavaScript theme objects, not `--merlion-*` tokens |
| A chart renderer inside each host page or application | No change to Merlion | Several renderers with different metrics, themes and output; no shared determinism guarantee |

## Criteria

Accuracy, portability, determinism across native and WASM, the zero-dependency rule, and delivery time. Accuracy and portability ruled out new Mermaid-style syntax. Determinism and the dependency rule ruled out third-party grammars. Between the core and per-host renderers, one renderer with one set of metrics and tokens decided it.

## Consequences

- Easier: any Merlion host — rehype, Astro, the CLI, a page calling `renderChart` — draws charts that match its diagrams' fonts, colours and themes, and an agent writes rows instead of SVG.
- Harder: Merlion's scope is no longer only Mermaid. Chart layout, its tests and its error codes (`E020`–`E023`, `I041`) are maintained alongside the diagram types.
- Expensive to reverse: the spec's member names and value grammar, the `data-merlion-*` attributes and classes in [chart.md](../chart.md#svg), and the JS names `renderChart` and `checkChart`. Breaking changes need `chart: 2`.
- Unmeasured: chart layout has fixture coverage, not a benchmark against another renderer or a reader study.

## References

- P. Heckbert. *Nice Numbers for Graph Labels.* Graphics Gems, 1990 — the 1-2-5 tick steps.
- L. Wilkinson. *The Grammar of Graphics.* Springer, 2005 — marks reading fields through scales.
- A. Satyanarayan, D. Moritz, K. Wongsuphasawat, J. Heer. *Vega-Lite: A Grammar of Interactive Graphics.* IEEE TVCG 23(1), 2017 — a JSON grammar as the interface between authors and a renderer.
