# State round 1 — 2026-09-23

Four parser corrections against [the baseline](2026-09-23-state-baseline.md) of the same day: a description adds a line instead of replacing the one before it, `state <id> : <text>` describes a state, `{` opens a composite body from the next line, text after a state id is dropped with `W024` rather than ending the parse, a run of the same Markdown delimiter flanks as one so `State1_____________` keeps its underscores, and the named entity codes cover the ASCII punctuation a label reserves (`#colon;`, `#lpar;`, `#semi;`, …).

| Measure | Baseline | Round 1 |
|---|---|---|
| Rendered | 94.9% (111/117) | **100.0% (117/117)** |
| Same content as mermaid-dagre | 90.1% (100/111) | **96.6% (113/117)** |
| Diagrams whose content differs | 11 | **4** |
| Native / WASM byte-identical (`compat-state`) | 111/117, 6 failing on both | **117/117**, at `--font link` and `--font embed` |
| Width, mean / median | 275 / 229 | 274 / 229 |
| Height, mean / median | 245 / 200 | 249 / 219 |
| Area (px²), mean / median | 77,373 / 40,670 | 75,558 / 40,492 |
| Mean fuel | 316 | 478 |

The size and fuel rows now average six more diagrams, four of which nest eight or nine composite states, so they are not a like-for-like comparison. Flowchart and sequence output is byte-identical: `compat_bytes` and the sequence fixtures pass unchanged, and no `compat` flowchart carries a delimiter run or one of the added entity names.

Three rows read worse only because the corpus each covers grew. `Fits 720 px` (100.0% → 98.3%), `Crossing-free` (97.3% → 96.6%) and `Overlap-free` (100.0% → 99.1%) each lose exactly the `should-render-edge-labels-correctly` family, whose `{` on its own line previously read as a state named `{` and left the eight composites flat. Drawn honestly they are large, and smaller than mermaid's drawing of the same source: 900 × 614 against 1104 × 1077, and 1290 × 380 against 1360 × 1077.

## What the remaining four differences are

Each is a mermaid artefact that the comparison reads as a Merlion failure.

| Diagram | What mermaid draws | What Merlion draws |
|---|---|---|
| `demos-state-08`, `e2e-v2-a-compound-state-should-be-able-to-link-to-itself` | `Active --> Active` on a composite state as the degenerate path `M80.45,192Z`, which has no length and no arrow | Both transitions, routed |
| `e2e-v2-states-can-have-a-class-applied`, `…-set-the-correct-length-of-the-labels` | `test({ foo&colon; 'far'})` — under `htmlLabels: false` the name reaches `<text>` undecoded | `test({ foo: 'far' })` |

`compat-state`: 117 `stateDiagram` and `stateDiagram-v2` sources extracted from mermaid at `mermaid@12.0.0` (see `bench/NOTICES.md`). Compatibility is measured against `mermaid-dagre` mermaid 12.0.0 (dagre): the same state labels, the same number of pseudo-states, the same transition count, the same non-empty transition labels and the same note texts, each compared as a multiset with whitespace removed. A state is named by what a reader sees, because the two renderers generate different ids for the same scope, so `[*]`, a choice diamond and a fork bar are counted rather than named.

| Measure | merlion | mermaid-dagre |
|---|---|---|
| Rendered | 100.0% (117/117) | 100.0% (117/117) |
| Same content as mermaid-dagre | 96.6% (113/117) | – |
| Fits 720 px | 98.3% (115/117) | 89.7% (105/117) |
| States / transitions, mean | 4.4 / 3.8 | 4.4 / 3.8 |
| Crossings, mean / median | 0.10 / 0.00 | 0.05 / 0.00 |
| Crossing-free | 96.6% (113/117) | 97.4% (114/117) |
| Label overlaps, mean / median | 0.01 / 0.00 | 0.00 / 0.00 |
| Overlap-free, notes included | 99.1% (116/117) | 100.0% (117/117) |
| Width, mean / median | 274 / 229 | 365 / 268 |
| Height, mean / median | 249 / 219 | 362 / 293 |
| Area (px²), mean / median | 75,558 / 40,492 | 162,201 / 75,272 |
| Speed p50 / p95 (ms) | 0.3 / 0.9 | 14.0 / 39.3 |
| Mean fuel | 478 | – |

Crossings count pairs of routed transition segments meeting away from a shared endpoint; label overlaps count pairs of drawn boxes intersecting by more than 1 px² over the state boxes and the transition label chips, and the overlap-free row adds the note boxes to that set.

## Where the content still differs

| Check | Diagrams failing |
|---|---|
| State labels | 0 |
| Pseudo-state count | 0 |
| Transition count | 2 |
| Transition labels | 4 |
| Note texts | 0 |

4 diagrams differ. The first 20, with the first check each fails:

| Diagram | Check | Detail |
|---|---|---|
| `demos-state-08` | transitions | 2 drawn against 1 |
| `e2e-v2-a-compound-state-should-be-able-to-link-to-itself` | transitions | 2 drawn against 1 |
| `e2e-v2-should-render-a-state-diagram-and-set-the-correct-length-of-the-labels` | transition labels | the label multisets differ |
| `e2e-v2-states-can-have-a-class-applied` | transition labels | the label multisets differ |

## Failed renders

mermaid rejects 0; Merlion rejects 0.
