# Quadrant chart baseline — 2026-11-27

Pinned mermaid mermaid@12.0.0 (98a0945418c76238f15df2afaddbba4272656c3b) in headless Chromium; 24 of 24 retained unique sources. See `bench/NOTICES.md` for provenance.

Compatibility compares the title, the four quadrant labels and where each sits in its quadrant, each axis label with its side of the plot and its alignment, and every point: its label, its 0–1 coordinates recovered from the circle's position in the plot, its radius, its stroke width, its explicit fill and stroke colours, and its label below the point. Quadrant fills are compared where the source sets them. Default colours are each renderer's theme and are not compared. This is content and placement compatibility, not pixel equivalence.

| Measure | Result |
|---|---|
| Matched / all retained sources | 95.8% (23/24) |
| Merlion rendered | 24/24 |
| mermaid rendered | 23/24 |
| Merlion fits 720 px | 24/24 |

Invalid and unsupported sources stay in the denominator; two rejections earn no credit. A limited run is exploratory and cannot establish corpus coverage.

| Source | Outcome | Comparison / error |
|---|---|---|
| `demos-quadrantchart-01` | matched |  |
| `demos-quadrantchart-02` | matched |  |
| `docs-quadrantchart-02` | matched |  |
| `docs-quadrantchart-03` | matched |  |
| `e2e-it-should-render-data-points-with-styles` | matched |  |
| `e2e-it-should-render-data-points-with-styles-classes` | matched |  |
| `e2e-should-able-to-render-x-axis-on-bottom` | matched |  |
| `e2e-should-able-to-render-x-axis-on-bottom-and-y-axis-on-right` | matched |  |
| `e2e-should-able-to-render-y-axis-on-right-side` | matched |  |
| `e2e-should-render-a-complete-quadrant-chart` | matched |  |
| `e2e-should-render-both-axes-labels-on-the-left-and-bottom-if-both-axes-have-only-one-label` | matched |  |
| `e2e-should-render-without-points` | matched |  |
| `e2e-should-render-without-title` | matched |  |
| `e2e-should-render-x-axis-labels-in-the-center-if-x-axis-has-two-labels` | matched |  |
| `e2e-should-render-y-axis-labels-in-the-center-if-y-axis-has-two-labels` | matched |  |
| `e2e-should-use-all-the-config` | matched |  |
| `e2e-should-use-all-the-theme-variable` | matched |  |
| `examples-quadrant-chart-01` | matched |  |
| `examples-quadrant-chart-02` | matched |  |
| `unit-quadrant-jison-01` | matched |  |
| `unit-quadrant-jison-02` | matched |  |
| `unit-quadrant-jison-03` | matched |  |
| `unit-quadrant-jison-04` | reference rejected | mermaid: style named fill is not supported. |
| `unit-quadrant-jison-05` | matched |  |
