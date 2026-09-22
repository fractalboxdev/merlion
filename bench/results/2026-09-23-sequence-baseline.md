# Sequence baseline — 2026-09-23

`compat-sequence`: 216 `sequenceDiagram` sources extracted from mermaid at `mermaid@12.0.0` (see `bench/NOTICES.md`). Compatibility is measured against `mermaid-dagre` mermaid 12.0.0 (dagre): the same participant labels, the same message count, the same non-empty message labels and the same note texts, each compared as a multiset with whitespace removed.

| Measure | merlion | mermaid-dagre |
|---|---|---|
| Rendered | 99.1% (214/216) | 98.6% (213/216) |
| Same content as mermaid-dagre | 99.1% (211/213) | – |
| Fits 720 px | 89.3% (191/214) | 60.1% (128/213) |
| Width, mean / median | 516 / 438 | 729 / 654 |
| Height, mean / median | 428 / 342 | 547 / 449 |
| Area (px²), mean / median | 264,590 / 141,736 | 443,516 / 336,473 |
| Label overlaps, mean / median | 0.02 / 0.00 | 0.23 / 0.00 |
| Speed p50 / p95 (ms) | 0.1 / 0.4 | 8.1 / 17.4 |
| Mean fuel | 431 | – |

Label overlaps count pairs of drawn boxes intersecting by more than 1 px²: the participant head boxes and note boxes each renderer draws, plus one box per message label estimated at 0.55 em per character in that renderer's own font size, because mermaid draws no background box behind a message label.

## Where the content differs

| Check | Diagrams failing |
|---|---|
| Participant labels | 0 |
| Message count | 0 |
| Message labels | 2 |
| Note texts | 2 |
| Fragment kinds (not part of the pass) | 1 |

2 diagrams differ. The first 20, with the first check each fails:

| Diagram | Check | Detail |
|---|---|---|
| `demos-sequence-07` | message labels | the label multisets differ |
| `demos-sequence-08` | message labels | the label multisets differ |

## Known differences

mermaid typesets `$$…$$` with KaTeX and Merlion draws it as the text it is, so the two never agree on a diagram that carries one. 2 of the 2 differing diagrams do.

## Failed renders

mermaid rejects 3; Merlion rejects 2.

- `demos-sequence-02`: parse: E011 front matter is not closed by a `---` line
- `demos-sequence-05`: parse: E002 expected a message such as `A->>B: text`, a keyword or a comment
