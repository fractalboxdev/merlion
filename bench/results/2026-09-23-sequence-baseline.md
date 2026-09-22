# Sequence baseline — 2026-09-23

`compat-sequence`: 216 `sequenceDiagram` sources extracted from mermaid at `mermaid@12.0.0` (see `bench/NOTICES.md`). Compatibility is measured against `mermaid-dagre` mermaid 12.0.0 (dagre): the same participant labels, the same message count, the same non-empty message labels and the same note texts, each compared as a multiset with whitespace removed.

| Measure | merlion | mermaid-dagre |
|---|---|---|
| Rendered | 97.2% (210/216) | 98.6% (213/216) |
| Same content as mermaid-dagre | 84.7% (177/209) | – |
| Fits 720 px | 90.0% (189/210) | 60.1% (128/213) |
| Width, mean / median | 517 / 451 | 729 / 654 |
| Height, mean / median | 431 / 344 | 547 / 449 |
| Area (px²), mean / median | 266,866 / 144,644 | 443,516 / 336,473 |
| Label overlaps, mean / median | 0.02 / 0.00 | 0.33 / 0.00 |
| Speed p50 / p95 (ms) | 0.3 / 1.4 | 7.9 / 18.3 |
| Mean fuel | 429 | – |

Label overlaps count pairs of drawn boxes intersecting by more than 1 px²: the participant head boxes and note boxes each renderer draws, plus one box per message label estimated at 0.55 em per character in that renderer's own font size, because mermaid draws no background box behind a message label.

## Where the content differs

| Check | Diagrams failing |
|---|---|
| Participant labels | 3 |
| Message count | 0 |
| Message labels | 25 |
| Note texts | 13 |
| Fragment kinds (not part of the pass) | 1 |

32 diagrams differ. The first 20, with the first check each fails:

| Diagram | Check | Detail |
|---|---|---|
| `demos-sequence-01` | message labels | the label multisets differ |
| `demos-sequence-03` | participants | missing ["multilineusing<br/>","multilineusing<br/>","multilineusing<br/>"], extra ["multilineusing","multilineusing","multilineusing"] |
| `demos-sequence-04` | message labels | the label multisets differ |
| `demos-sequence-07` | message labels | the label multisets differ |
| `demos-sequence-08` | message labels | the label multisets differ |
| `docs-sequencediagram-24` | message labels | the label multisets differ |
| `docs-sequencediagram-25` | message labels | the label multisets differ |
| `e2e-sequence-05` | message labels | the label multisets differ |
| `e2e-sequence-v2-02` | message labels | the label multisets differ |
| `e2e-should-handle-different-line-breaks` | participants | missing ["multilineusing<br/>","multilineusing<br/>","multilineusing<br/>"], extra ["multilineusing","multilineusing","multilineusing"] |
| `e2e-should-handle-empty-lines` | message labels | the label multisets differ |
| `e2e-should-handle-line-breaks-and-wrap-annotations` | message labels | the label multisets differ |
| `e2e-should-render-a-sequence-diagram-with-actor-creation-and-destruction-coupled-with-backgrounds-loops-and-notes` | message labels | the label multisets differ |
| `e2e-should-render-a-sequence-diagram-with-par-over` | message labels | the label multisets differ |
| `e2e-should-render-autonumber-with-different-line-breaks` | message labels | the label multisets differ |
| `e2e-should-render-different-participant-types-with-wrapping-text` | message labels | the label multisets differ |
| `e2e-should-render-long-messages-wrapped-inline-from-an-actor-to-the-left-to-one-to-the-right` | message labels | the label multisets differ |
| `e2e-should-render-long-messages-wrapped-inline-from-an-actor-to-the-right-to-one-to-the-left` | message labels | the label multisets differ |
| `e2e-should-render-long-notes-wrapped-inline-left-of-actor` | notes | the note texts differ |
| `e2e-should-render-long-notes-wrapped-inline-over-actor` | notes | the note texts differ |

## Known differences

An escaped line break — `#lt;br#gt;`, the source's way of writing a literal `<br>` — decodes to `<br>` and is then read as a line break, so the literal text is dropped where mermaid draws it. 3 of the 32 differing diagrams carry one. Entity decoding is shared with flowcharts, whose output the `compat` digests pin, so the order of decoding and `<br>` splitting is one change for both.

## Failed renders

mermaid rejects 3; Merlion rejects 6.

- `demos-sequence-02`: parse: E011 front matter is not closed by a `---` line
- `demos-sequence-05`: parse: E002 expected a message such as `A->>B: text`, a keyword or a comment
- `e2e-should-render-actor-and-database-aligned-on-neo`: parse: E002 `rect` expects an `rgb()`, `rgba()`, `hsl()` or `hsla()` colour; a hex colour is unavailable because `#` opens a comment
- `e2e-should-render-sequence-rect-with-theme-aware-default-background`: parse: E002 `rect` expects an `rgb()`, `rgba()`, `hsl()` or `hsla()` colour; a hex colour is unavailable because `#` opens a comment
- `e2e-should-render-sequence-rect-with-theme-aware-default-background-base-theme`: parse: E002 `rect` expects an `rgb()`, `rgba()`, `hsl()` or `hsla()` colour; a hex colour is unavailable because `#` opens a comment
- `e2e-should-render-sequence-rect-with-theme-aware-default-background-dark-theme`: parse: E002 `rect` expects an `rgb()`, `rgba()`, `hsl()` or `hsla()` colour; a hex colour is unavailable because `#` opens a comment
