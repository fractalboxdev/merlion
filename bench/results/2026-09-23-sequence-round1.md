# Sequence round 1 — 2026-09-23

First measured round on sequence diagrams, over the 22 fixtures in
`crates/merlion-render/tests/fixtures/sequence/`, against mermaid 12.0.0 rendered in
headless Chromium with `htmlLabels: false`. The `compat` corpus carries no sequence
diagram — all 391 entries are flowcharts — so there is no sequence pass rate yet.

Before is the three-way merge of the parser, layout and draw workstreams; after is that
merge plus the three fixes below.

## Result

| Measure | Before | After |
|---|---|---|
| Message labels wrapped over more than one line | 41 | 19 |
| Lifeline segments painted over by a fragment box | 75, in 16 of 22 fixtures | 0 |
| Participants created inside a fragment, drawn | 0 of 2 | 2 of 2 |
| Total fixture area (px²) | 8,643,674 | 8,574,491 (−0.8%) |
| Fixtures whose geometry changes | — | 7 |

Flowchart output is byte-identical: `compat_bytes` passes over all 384 rendered `compat`
diagrams, and the merlion compat run reproduces the committed `2026-09-22-final.md` row
for row — crossings 0.10 / 0.00, bends 4.9 / 0.0, area 99,886 / 41,091, label overlaps
0.00 / 0.00.

## Fixes

**Columns paint over the fragments they sit inside.** A fragment box carries an opaque
`--merlion-cluster-bg` fill and used to be drawn after the columns, so every lifeline
crossing a fragment was erased — 75 segments across 16 fixtures — and a participant
`create`d inside one vanished completely. `payment-3ds` emitted the `3-D Secure page`
head box and its destroy cross and then painted the `critical` box over both, leaving
`Redirect the shopper` and `Enter the one-time code` pointing at nothing. The draw order
is now boxes, fragments, columns, activations, notes, messages; ordinary head and foot
boxes sit above and below every fragment, so nothing else moves.

**Container fit stops wrapping for a fit the diagram never reaches.** Fit narrows the
label wrap width in 20 px steps to a 120 px floor, and used to keep the narrowest result
even when no wrap width brings the drawing to the 720 px target. Seven fixtures are in
that case. Each is now wider and shorter with single-line labels; the fifteen that do
reach the target are untouched.

| Fixture | Before | After |
|---|---|---|
| payment-3ds | 939.55 × 872.55 | 1037.85 × 737.03 |
| db-replication | 885.55 × 594.91 | 1028.59 × 510.21 |
| llm-tool-calls | 761.25 × 706.67 | 979.85 × 588.09 |
| async-queue-fanout | 855.61 × 651.85 | 967.83 × 567.15 |
| auth-device-flow | 778.87 × 1034.06 | 901.50 × 864.67 |
| ci-release-train | 765.85 × 583.15 | 882.70 × 515.52 |
| websocket-session | 758.05 × 559.03 | 786.06 × 508.21 |

**A creating message ends at the head box it opens.** That box sits on the creating
message's own row, so an arrow run to the lifeline ended underneath it and buried its own
arrowhead, and its label landed on the box's text.

## Open

- The `compat` corpus has no sequence diagram. mermaid's `demos/sequence.html`,
  `packages/mermaid/src/docs/syntax/sequenceDiagram.md` and
  `e2e/rendering/sequencediagram*.spec.js` are the sources a `compat` sequence corpus
  draws from, and `src/svg/extract.ts` reads flowchart node and edge groups only, so
  compatibility, crossings, bends and stress need sequence-shaped definitions before a
  pass rate means anything.
- The remaining 19 wrapped labels are in diagrams that do reach 720 px. Whether a
  sequence should fit a flowchart's `target_width` at all is an open call: the viewer
  zooms either way, and mermaid never fits.
- A note or a section label drawn across a lifeline still hides it — the same opaque-fill
  question as fragments, one layer further in.
- `viewer-links.mmd` renders in Merlion and fails in mermaid, so the six `llm-*` repair
  fixtures and this one have no mermaid drawing to compare against. That is the intent:
  they exercise `R009`–`R013` and `W021`.
