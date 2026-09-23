# State baseline — 2026-09-23

`compat-state`: 117 `stateDiagram` and `stateDiagram-v2` sources extracted from mermaid at `mermaid@12.0.0` (see `bench/NOTICES.md`). Compatibility is measured against `mermaid-dagre` mermaid 12.0.0 (dagre): the same state labels, the same number of pseudo-states, the same transition count, the same non-empty transition labels and the same note texts, each compared as a multiset with whitespace removed. A state is named by what a reader sees, because the two renderers generate different ids for the same scope, so `[*]`, a choice diamond and a fork bar are counted rather than named.

| Measure | merlion | mermaid-dagre |
|---|---|---|
| Rendered | 94.9% (111/117) | 100.0% (117/117) |
| Same content as mermaid-dagre | 90.1% (100/111) | – |
| Fits 720 px | 100.0% (111/111) | 89.7% (105/117) |
| States / transitions, mean | 4.5 / 3.8 | 4.4 / 3.8 |
| Crossings, mean / median | 0.08 / 0.00 | 0.05 / 0.00 |
| Crossing-free | 97.3% (108/111) | 97.4% (114/117) |
| Label overlaps, mean / median | 0.00 / 0.00 | 0.00 / 0.00 |
| Overlap-free, notes included | 100.0% (111/111) | 100.0% (117/117) |
| Width, mean / median | 275 / 229 | 365 / 268 |
| Height, mean / median | 245 / 200 | 362 / 293 |
| Area (px²), mean / median | 77,373 / 40,670 | 162,201 / 75,272 |
| Speed p50 / p95 (ms) | 0.1 / 0.4 | 14.9 / 40.9 |
| Mean fuel | 316 | – |

Crossings count pairs of routed transition segments meeting away from a shared endpoint; label overlaps count pairs of drawn boxes intersecting by more than 1 px² over the state boxes and the transition label chips, and the overlap-free row adds the note boxes to that set.

## Where the content differs

| Check | Diagrams failing |
|---|---|
| State labels | 7 |
| Pseudo-state count | 0 |
| Transition count | 2 |
| Transition labels | 7 |
| Note texts | 0 |

11 diagrams differ. The first 20, with the first check each fails:

| Diagram | Check | Detail |
|---|---|---|
| `demos-state-08` | transitions | 2 drawn against 1 |
| `e2e-should-render-a-states-with-descriptions-including-multi-line-descriptions` | state labels | missing ["Thisamultilinedescriptionherecomesthemultipart"], extra ["herecomesthemultipart"] |
| `e2e-should-render-state-descriptions` | state labels | missing ["AnotherLongstatedescriptionNewline"], extra ["Newline"] |
| `e2e-v2-a-compound-state-should-be-able-to-link-to-itself` | transitions | 2 drawn against 1 |
| `e2e-v2-should-render-a-state-diagram-and-set-the-correct-length-of-the-labels` | transition labels | the label multisets differ |
| `e2e-v2-should-render-a-states-with-descriptions-including-multi-line-descriptions` | state labels | missing ["Thisamultilinedescriptionherecomesthemultipart"], extra ["herecomesthemultipart"] |
| `e2e-v2-should-render-edge-labels-correctly` | state labels | missing ["State1_____________","State2_____________","State3_____________"], extra ["State1_","State2_","State3_"] |
| `e2e-v2-should-render-edge-labels-correctly-with-multiple-states` | state labels | missing ["State10_____________","State1_____________","State2_____________"], extra ["State10_","State1_","State2_"] |
| `e2e-v2-should-render-edge-labels-correctly-with-multiple-transitions` | state labels | missing ["State1_____________","State2_____________","State3_____________"], extra ["State1_","State2_","State3_"] |
| `e2e-v2-should-render-state-descriptions` | state labels | missing ["AnotherLongstatedescriptionNewline"], extra ["Newline"] |
| `e2e-v2-states-can-have-a-class-applied` | transition labels | the label multisets differ |

## Failed renders

mermaid rejects 0; Merlion rejects 6.

- `e2e-should-render-a-long-descriptions-with-additional-descriptions`: parse: E002 expected `;` or a newline after the statement, found `T`
- `e2e-should-render-forks-and-joins`: parse: E002 expected `;` or a newline after the statement, found `&`
- `e2e-should-render-forks-in-composit-states`: parse: E002 expected `;` or a newline after the statement, found `&`
- `e2e-v2-should-render-a-long-descriptions-with-additional-descriptions`: parse: E002 expected `;` or a newline after the statement, found `T`
- `e2e-v2-should-render-forks-and-joins`: parse: E002 expected `;` or a newline after the statement, found `&`
- `e2e-v2-should-render-forks-in-composite-states`: parse: E002 expected `;` or a newline after the statement, found `&`
