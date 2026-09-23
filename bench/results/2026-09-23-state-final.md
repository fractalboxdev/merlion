# State final — 2026-09-23

Eleven corrections against [round 1](2026-09-23-state-round1.md) of the same day, over the boundaries a hostile source crosses, the geometry of concurrency regions and labels, the text alternative, the note's colour and the corpus itself. `compat-state` is unchanged in size and in what it compares: 117 sources at `mermaid@12.0.0` against `mermaid-dagre` mermaid 12.0.0.

## What a hostile source costs

Three inputs inside the 1 MiB `input_bytes` limit spent far more than the limit implies. All three are release-build wall time through the CLI, with peak RSS where it is the point.

| 1 MiB input | Round 1 | Final |
|---|---|---|
| `stateDiagram-v2\n` + `a;` × 500,000 | 34.85 s | **0.04 s** |
| `stateDiagram-v2\n` + `a}` × 500,000 | 40.85 s | **0.11 s** |
| `stateDiagram-v2\n` + `}` × 1,048,500 | 22.96 s, 165 MB | **0.12 s, 4.4 MB** |
| `stateDiagram-v2\n` + `}\n` × 500,000 | 2.13 s, 86 MB | **0.10 s, 10 MB** |
| `stateDiagram-v2\n` + `--\n` × 300,000 | 1.27 s, 54 MB | **0.08 s, 8.4 MB** |
| `stateDiagram-v2\n` + `hide empty description\n` × 45,000 | 0.20 s, 14 MB | **0.03 s, 5.0 MB** |
| `stateDiagram-v2\na\n` + `note left of a : x\n` × 55,187 | 0.10 s, 25,568,072-byte SVG, 0 diagnostics | **`TooLarge`** |
| `stateDiagram-v2\n` + `a\n` × 500,000 (the control) | 0.07 s | 0.07 s |

The `}` shape scales linearly now: 100k / 200k / 400k take 0.42 / 0.82 / 1.67 s, against 0.60 / 1.57 / 4.52 s.

Three boundaries moved. `Limits::notes` (2,000) bounds a state diagram's notes, which neither `nodes` nor `edges` counted. `Limits::diagnostics` (4,000, matching `edges`) bounds the diagnostic list, whose last slot then carries `I034` with the count of what it leaves out; a dropped `Error` still fails the render. The layout follows 128 cluster levels rather than 64, because a composite carrying concurrency regions lowers to a cluster of clusters and so costs two levels per model level.

## Drawing

| Measure | merlion (round 1) | merlion (final) | mermaid-dagre |
|---|---|---|---|
| Rendered | 100.0% (117/117) | 100.0% (117/117) | 100.0% (117/117) |
| Same content as mermaid-dagre | 96.6% (113/117) | 96.6% (113/117) | – |
| Fits 720 px | 98.3% (115/117) | 98.3% (115/117) | 89.7% (105/117) |
| Crossings, mean / median | 0.10 / 0.00 | 0.10 / 0.00 | 0.05 / 0.00 |
| Crossing-free | 96.6% (113/117) | 96.6% (113/117) | 97.4% (114/117) |
| Label overlaps, mean / median | 0.01 / 0.00 | 0.01 / 0.00 | 0.00 / 0.00 |
| Overlap-free, notes included | 99.1% (116/117) | 99.1% (116/117) | 100.0% (117/117) |
| Transitions drawn across another's label | 118 in 20 diagrams | 118 in 20 diagrams | 2 in 2 diagrams |
| Diagrams drawing a fork or join bar | 2 (4 bars) | **6 (12 bars)** | 6 |
| Width, mean / median | 274 / 229 | 274 / 229 | 365 / 268 |
| Height, mean / median | 249 / 219 | 247 / 219 | 361 / 293 |
| Area (px²), mean / median | 75,558 / 40,492 | 75,153 / 40,492 | 161,632 / 75,272 |
| Speed p50 / p95 (ms) | 0.4 / 0.9 | 0.4 / 1.0 | 14.0 / 39.1 |
| Mean fuel | 478 | 479 | – |

The four content differences are the same four as in round 1, each a mermaid artefact the comparison reads as a Merlion failure: two self-transitions on a composite state that mermaid draws as the degenerate path `M80.45,192Z`, and two labels mermaid leaves entity-encoded under `htmlLabels: false`.

The bar row is the corpus correction. mermaid's end-to-end `.mmd` fixtures are inserted into an HTML page by its own harness, so the browser decodes their entities before the parser reads them; the four fork and join fixtures spell their markers `&lt;&lt;fork&gt;&gt;`, and left encoded they reached the grammar as text and drew labelled rectangles. Extraction now decodes them, so the pass rate covers the bars it always claimed to.

Three defects the drawing no longer has, none of which these aggregates see:

- **Region dividers.** A region is an ordinary sibling cluster of its composite, and the layered engine places sibling clusters beside one another on the order axis, which runs across the direction. The divider took its axis from the direction instead, so in a `TB` machine it was a horizontal line through the middle of the state boxes, and three regions produced two identical lines — both dividers of `e2e-v2-should-align-dividers-correctly` were `M20 67.41L325.01 67.41`. Each divider now crosses the gap its region box leaves with its predecessor's, on whichever axis the two are apart on: `M124 20L124 97.88` and `M240.4 20L240.4 97.88`.
- **Label chips on cluster titles.** A chip placed from a label dummy's slot or from a self-loop is positioned before the cluster boxes exist, so it could land on a cluster title: `State4_____________` and `State8_____________` both vanished under a transition label. Chip-over-title in `compat-state` falls from 4 in 2 diagrams to 2 in 1. The two that remain sit on edges that offer no clear position at all.
- **Notes reading as states.** A note box and a plain state box carried the same `fill="#f5f5f5" stroke="#c8c9cb"` under the default theme and resolved the same way under the shipped themes. `--merlion-note-bg` and `--merlion-note-border` are the note's own tokens, falling back to the page background and to `--merlion-warn`.

The text alternative now describes the drawing rather than the source: a transition between a composite state and one of its own members has no two endpoints in the lowered graph and is not drawn, so the outline neither lists nor counts it and a closing line names how many the drawing leaves out. `llm-unclosed-composite` read "8 states, 7 transitions" over an SVG holding six; it now reads six and says one is not drawn.

## Determinism and byte stability

Native and WASM output is byte-identical over every corpus in every font mode — `compat` 384/390, `compat-sequence` 214/216, `compat-state` 117/117, the state fixtures 20/20 and the sequence fixtures 22/22, with 0 differing at `--font link`, `--font embed` and `--font system`. The failures on both targets are the same pre-existing ones.

Flowchart and sequence output is unchanged to the byte. `compat_bytes` passes against its recorded digests, and rendering `bench/corpus/compat`, `bench/corpus/compat-sequence`, `tests/fixtures/flowcharts` and `tests/fixtures/sequence` with the renderer before and after these changes gives identical files in all four directories.

## What `compat-state` measures

117 `stateDiagram` and `stateDiagram-v2` sources extracted from mermaid at `mermaid@12.0.0` (see `bench/NOTICES.md`). Compatibility is the same state labels, the same number of pseudo-states, the same transition count, the same non-empty transition labels and the same note texts, each compared as a multiset with whitespace removed. A state is named by what a reader sees, because the two renderers generate different ids for the same scope, so `[*]`, a choice diamond and a fork bar are counted rather than named.

Crossings count pairs of routed transition segments meeting away from a shared endpoint. Label overlaps count pairs of drawn boxes intersecting by more than 1 px² over the state boxes and the transition label chips; the overlap-free row adds the note boxes. The bar-against-bar measures miss a transition painted across another transition's chip, which leaves the text under it unreadable while the two chips never touch, so `edgesThroughLabels` counts that separately.
