# Benchmark: compat corpus

Run 2026-09-22 at commit `8224422`. Corpus: 390 flowcharts from mermaid at `98a0945418c7` (tag `mermaid@12.0.0`, MIT; see `bench/NOTICES.md`), 106 edit pairs. Metric definitions: `bench/README.md`; method: `specs/benchmark.md`.

## Renderers

| Renderer | Version | Rendered | Same graph as mermaid-dagre | Fits 720 px | p50 ms | p95 ms | Mean fuel |
|---|---|---|---|---|---|---|---|
| mermaid-dagre | mermaid 12.0.0 (dagre) | 99.7% (389/390) | reference | 81.0% (315/389) | 9.8 | 30.4 | – |
| mermaid-elk | mermaid 12.0.0 (elk) | 99.7% (389/390) | 99.5% (387/389) | 82.3% (320/389) | 11.9 | 30.5 | – |

## Layout quality

Mean / median over the diagrams each renderer drew. Lower is better except aspect ratio.

| Metric | mermaid-dagre | mermaid-elk |
|---|---|---|
| Crossings | 0.20 / 0.00 | 0.05 / 0.00 |
| Max crossings on one edge | 0.08 / 0.00 | 0.03 / 0.00 |
| Bends | 3.7 / 0.0 | 4.2 / 0.0 |
| Total edge length (px) | 562 / 167 | 631 / 146 |
| Area (px²) | 197155 / 83512 | 182508 / 80196 |
| Label overlaps | 0.01 / 0.00 | 0.01 / 0.00 |
| Stress | 0.068 / 0.010 | 0.076 / 0.020 |
| Aspect ratio (w/h) | 2.68 / 1.92 | 2.67 / 1.76 |

## Per diagram against mermaid-elk

Wins / ties / losses on diagrams both renderers drew; a win is a strictly lower value.

| Metric | mermaid-dagre |
|---|---|
| Crossings | 1 / 373 / 15 |
| Max crossings on one edge | 1 / 373 / 15 |
| Bends | 69 / 303 / 17 |
| Total edge length (px) | 136 / 139 / 114 |
| Area (px²) | 92 / 130 / 167 |
| Label overlaps | 0 / 389 / 0 |
| Stress | 110 / 151 / 38 |

## Stability

Displacement of nodes surviving a one-line edit, as a fraction of the diagram's diagonal before the edit. Merlion receives the previous SVG as its layout hint.

| Renderer | Pairs rendered | Mean | p95 |
|---|---|---|---|
| mermaid-dagre | 102/106 | 0.059 | 0.311 |
| mermaid-elk | 102/106 | 0.060 | 0.258 |

## Determinism

Native vs WASM: skipped (merlion was not among the renderers).

## Failures

**mermaid-dagre**

- 1 × Parse error on line 4:

**mermaid-elk**

- 1 × Parse error on line 4:
