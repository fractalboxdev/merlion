# Merlion benchmark harness

Implements [specs/benchmark.md](../specs/benchmark.md): corpora, renderer adapters, graph extraction from SVG, metrics and the Markdown report. Private; not published.

```sh
cd bench
pnpm install --ignore-workspace
pnpm bench fetch                     # compat + edits corpora (committed)
cargo build --release -p merlion-cli # from the repository root, for the merlion adapter
pnpm bench run [--renderers merlion,mermaid-dagre,mermaid-elk] [--corpus compat] [--limit N] [--out-svgs] [--no-edits]
pnpm bench report [--input results/<file>.json] [--out <file>.md]
pnpm bench parity [--limit N] [--require-rsvg]   # stylesheet parity, after the release build
pnpm bench sequence [--limit N] [--out-svgs]    # compat-sequence against mermaid, writes results/<date>-sequence-baseline.md
pnpm bench state [--limit N] [--out-svgs]       # compat-state against mermaid, writes results/<date>-state-baseline.md
pnpm bench authoring generate --provider claude-cli --model <m> [--label <name>] [--limit N]
pnpm bench authoring score [--out <dir>]        # writes results/<date>-authoring.md
pnpm test && pnpm typecheck
```

`run` writes `results/<date>-<commit>.json`; `report` writes the Markdown summary next to the newest results file. `--out-svgs` writes every drawing to `results/svgs/<renderer>/<name>.svg`. Only `results/baseline-*.md`, `results/<date>-baseline.md`, `results/<date>-round<n>.md`, `results/<date>-sequence-baseline.md`, `results/<date>-sequence-round<n>.md`, `results/<date>-state-baseline.md` and `results/<date>-state-round<n>.md` are committed; the JSON and the SVG dumps are not.

## Corpora

| Corpus | Source | Count |
|---|---|---|
| `compat` | mermaid at tag `mermaid@12.0.0` (commit `98a0945418c7`): `<pre class="mermaid">` blocks in `demos/*.html`, `mermaid` / `mermaid-example` fences in `packages/mermaid/src/docs/syntax/flowchart.md`, static template literals in `e2e/rendering/flowchart/*.spec.*`, and `e2e/diagrams/flowchart/**/*.mmd` except `handdrawn/`. Flowchart and graph diagrams only, de-duplicated by content | 390 |
| `compat-sequence` | mermaid at the same commit: `<pre class="mermaid">` blocks in `demos/*.html`, `mermaid` / `mermaid-example` fences in `packages/mermaid/src/docs/syntax/sequenceDiagram.md`, static template literals in `e2e/rendering/sequence/*.spec.*`, and `e2e/diagrams/sequence/*.mmd`. `sequenceDiagram` sources only, de-duplicated by content | 216 |
| `compat-state` | mermaid at the same commit: `<pre class="mermaid">` blocks in `demos/*.html`, `mermaid` / `mermaid-example` fences in `packages/mermaid/src/docs/syntax/stateDiagram.md`, static template literals in `e2e/rendering/state/*.spec.*`, and `e2e/diagrams/state-diagram/**/*.mmd` and `e2e/diagrams/state-diagram-v2/**/*.mmd`. `stateDiagram` and `stateDiagram-v2` sources only, de-duplicated by content | 117 |
| `edits` | The first 30 `compat` diagrams (by name) with at least three simple edge lines, each edited four ways: add an isolated node, add an edge between the farthest-apart unconnected pair, remove the last simple edge (re-declaring endpoints it declared), rename the first `id[Label]` | 106 pairs |
| `authoring` | 18 diagrams (5 to 19 nodes) stated in prose in `corpus/authoring/tasks.json` beside the graph each one means, and the recorded model answers in `corpus/authoring/outputs/` | 18 tasks |

Every corpus but `authoring` pins each diagram in its `manifest.json` by source path, source blob and sha256; `authoring` is original, and its task file is its own ground truth. The mermaid MIT notice is in [NOTICES.md](NOTICES.md).

## Renderers

| Name | Runs | Notes |
|---|---|---|
| `merlion` | `target/release/merlion render --batch in -o out --json-summary`, one process per corpus; edit pairs run `render --json --hint <prev.svg>` per diagram with the source on stdin | Corpus time is the CLI's in-process timing of the core call (`micros`); edit-pair time includes process start-up. Fuel comes from `fuel_used` |
| `mermaid-dagre` | mermaid 12.0.0 `dist/mermaid.min.js` in headless Chromium (Playwright 1.63.0), `layout: "dagre"` | Time is `mermaid.render` measured in the page |
| `mermaid-elk` | Same bundle, `layout: "elk"` | mermaid 12 bundles ELK; `@mermaid-js/layout-elk` is not needed |

Both mermaid adapters render with `htmlLabels: false`, so labels are `<text>` and every SVG is read by the same parser. A diagram's own front matter or `%%{init}%%` can still override the layout and `htmlLabels`. A render that exceeds 30 s is recorded as a failure. Every failure is recorded per diagram; none aborts the run. Compatibility is measured against `mermaid-dagre`.

## Metrics

All metrics are computed from the SVG alone (`src/svg/extract.ts`, `src/metrics/metrics.ts`), in viewBox units.

- **Extraction.** Merlion: `g.merlion-node[data-merlion-id]`, `g.merlion-edge[data-merlion-from][data-merlion-to]`, `g.merlion-cluster`. Mermaid: node groups (`g.node`, `g.rough-node`, or any group with a `…flowchart-<id>-<n>` id, which gives the node id), edge paths (`.flowchart-link`, endpoints from `data-id="L_<from>_<to>_<n>"` resolved against node and subgraph ids, else the nearest node box within 30 px; multi-stroke hand-drawn paths use `data-points`), `g.edgeLabel`. Node box: union of the node's shapes, transforms applied. Label: `<text>` content with line breaks as spaces, or, when a mermaid label has no `<text>` (the diagram's config sets `htmlLabels: true`), `<foreignObject>` content with a space at every `<br>` and block boundary; `fa:fa-*` icon tokens removed, whitespace collapsed. Curves are sampled at 12 segments.
- **Crossings.** Proper intersections between the polylines of two distinct edges. For edges sharing an endpoint node, intersections within 8 px of that node's box are not counted. Also reported: the maximum over edges of crossings on one edge.
- **Bends.** Per edge: Douglas–Peucker simplification at 3 px, then interior vertices grouped when closer than 12 px; a group whose summed signed turn exceeds 10° is one bend. A rounded orthogonal corner and a spline turn both count once.
- **Total edge length.** Sum of polyline lengths.
- **Area, aspect ratio, fit.** viewBox width × height; width / height; fits when viewBox width ≤ 720.
- **Label overlaps.** Pairs among node boxes and edge-label boxes (the background box when drawn, else an estimate at 0.55 em per character) intersecting by more than 1 px².
- **Stress.** Normalised stress with optimal scaling: `(1/P) Σ ((s‖x_i − x_j‖ − d_ij)/d_ij)²` over the P connected pairs, d = undirected shortest-path length, x = node box centre, `s = Σ(‖x‖/d) / Σ(‖x‖²/d²)` minimising the sum. Scale-invariant; 0 reproduces graph distance exactly.
- **Compatibility.** Same node-label multiset, same edge count and same multiset of non-empty edge labels as the `mermaid-dagre` drawing of the same source, labels compared with whitespace removed because renderers wrap at different points. Pass rate is over diagrams `mermaid-dagre` rendered.
- **Stability.** For each edit pair: nodes present before and after (matched by id), box-centre displacement relative to each drawing's viewBox origin, divided by the viewBox diagonal before the edit. Reported as the mean and p95 over all surviving nodes. Merlion renders the edited source with the previous SVG as `--hint`.
- **Speed.** p50 / p95 render milliseconds over rendered diagrams; mean fuel for Merlion.
- **Determinism.** Native vs WASM byte identity, run when `packages/merlion-wasm/merlion.wasm` exists (loaded through `packages/merlion-wasm/index.js` `initSync`); otherwise reported as skipped.
- **Per diagram against mermaid-elk.** For each metric where lower is better, counts of diagrams where a renderer is strictly lower (win), equal within 1e-6 relative (tie) or higher (loss), over diagrams both drew.

## Sequence diagrams

`pnpm bench sequence` measures the `compat-sequence` corpus (`src/sequence-run.ts`, `src/svg/sequence.ts`, `src/metrics/sequence.ts`). A sequence diagram has no routed graph — participants are columns in source order and messages are rows — so crossings, bends, edge length and stress do not apply and are not reported. What is measured:

- **Content compatibility** against the `mermaid-dagre` drawing of the same source: the same participant labels, the same message count, the same non-empty message labels and the same note texts, each compared as a multiset with whitespace removed because the two wrap at different points. Fragment kinds are reported beside the pass, not inside it: `rect` tints rows in mermaid and draws no kind tab, so mermaid's drawing carries no fragment to read.
- **Extraction.** Merlion: `g.merlion-participant[data-merlion-id]`, `g.merlion-message`, `g.merlion-note`, `g.merlion-fragment[data-merlion-kind]`. mermaid: `text.actor-box` / `text.actor-man` for participants — one `<text>` per drawn line at a shared anchor, and the whole label once at the head and once at the foot, so texts are grouped by anchor and kept once per column, and the hidden `g.actorPopupMenu` a `link` statement adds is skipped; `.messageLine0` / `.messageLine1` for messages with `.messageText` for their labels; `rect.note` with the `.noteText` lines it holds; `.labelText` and `.loopText` for fragments.
- **Render rate, fit, size and speed**, as for flowcharts.
- **Label overlaps**: pairs of drawn boxes intersecting by more than 1 px² — participant head boxes and note boxes, plus one box per message label estimated at 0.55 em per character in that renderer's own font size, since mermaid draws no background box behind a message label.

## State diagrams

`pnpm bench state` measures the `compat-state` corpus (`src/state-run.ts`, `src/svg/state.ts`, `src/metrics/state.ts`). A state machine lowers to a routed graph, so the flowchart measures — crossings, label overlaps, size, fit and speed — apply unchanged and are reported beside the content comparison. What is state-specific:

- **Content compatibility** against the `mermaid-dagre` drawing of the same source: the same state labels, the same number of pseudo-states, the same transition count, the same non-empty transition labels and the same note texts, each a multiset with whitespace removed. A state is named by what a reader sees, because the two renderers generate different ids for one scope, so `[*]`, a choice diamond and a fork bar are counted rather than named.
- **Extraction.** Merlion: `g.merlion-node[data-merlion-id]`, `g.merlion-edge[data-merlion-from][data-merlion-to]`, `g.merlion-cluster` — a concurrency region is `g.merlion-region` and is not one — and `g.merlion-note` with its `.merlion-note-box`. mermaid: `g.node` for the states, `g.statediagram-cluster` for the composites minus the `note-cluster` it wraps a noted state in, `path.transition` minus the `note-edge` that links a note to its state, and `g.statediagram-note` for the notes; the two node ids ending `----note` and `----parent` belong to that scaffolding and are dropped.
- **Label overlaps** are reported twice: over the state boxes and transition-label chips, as for flowcharts, and again with the note boxes counted in.

## Stylesheet parity

`pnpm bench parity` implements the stylesheet parity row of [specs/benchmark.md](../specs/benchmark.md) (`src/parity/`). Inputs: every `compat` diagram plus `fixtures/roles/*.mmd` (every built-in role on nodes, clusters and edges; stylesheet roles; re-themed `classDef`s next to `style`/`linkStyle` literals), and `fixtures/parity.css`, which must compile under `merlion css --strict`. Each theme of the compiled CSS (`:root` and `dark`) is checked against three targets:

| Target | How |
|---|---|
| Reference | Plain SVG (`render --no-hint`) inlined in Chromium with the compiled CSS linked and `data-theme` on `<html>` |
| Baked, no host CSS | `render --css fixtures/parity.css [--theme t]` inlined with no page CSS |
| Baked, `<style>` removed | The same SVG without its `<style>`: presentation attributes only |
| rsvg-convert | The baked SVG at `-z 2`, sampled at points chosen on the reference |

Compared per element (every path, rect, circle, ellipse, line, polygon, text and tspan outside `<defs>`, plus marker contents): computed `fill` and `stroke` through a canvas `fillStyle` round trip to 8-bit sRGB (±1 per channel, paint opacity folded into alpha), `stroke-dasharray`, and the product of `opacity` up the tree. Pixel samples: up to three interior fill points per shape that are the topmost element there and clear of every label's box (widened for font differences), up to three points on each stroke kept 1 px inside a dash, and one interior point per marker instance. A sample counts only where the reference screenshot shows the element's own opaque paint, so occluded and antialiased points are dropped and reported as a count. Text is compared by ink, because glyph outlines differ between Chromium's and librsvg's font stacks: one box per text element (per tspan when tspans carry their own fill), and inside it a pixel within ±8 per channel of the computed fill must appear in the rsvg-convert raster whenever the reference screenshot shows one. The run also bakes every named theme of `merlion-themes.css` and fails on a stylesheet warning.

Output: a summary on stdout and every mismatch in `target/parity/report.json`. Without rsvg-convert on `PATH` the pixel target is reported as skipped; CI passes `--require-rsvg`.

## Authoring: Mermaid or SVG

`pnpm bench authoring` measures which output format serves a model better when it is asked for the same diagram twice, implementing [specs/benchmark.md](../specs/benchmark.md#authoring) (`src/authoring/`). Generation and scoring are separate steps so the numbers are reproducible without an account:

```sh
pnpm bench authoring generate --provider claude-cli --model sonnet     # records corpus/authoring/outputs/sonnet.json
pnpm bench authoring generate --provider anthropic --model claude-opus-5  # needs ANTHROPIC_API_KEY
pnpm bench authoring score                                             # scores every recorded file
```

`generate` asks one model for each of the 18 tasks in both formats and writes the answers, the extracted diagram and the provider's token counts to one file per model. `score` never calls a model: it reads those files, draws each Mermaid answer through Merlion, takes each SVG answer as the drawing, and writes `results/<date>-authoring.{json,md}`.

- **The two prompts differ only in the output format** (`src/authoring/prompt.ts`). Both carry the same readability requirement, which the layout engine satisfies for one side and the model has to satisfy itself for the other.
- **Fidelity** (`src/authoring/compare.ts`) compares labels, never ids. A Mermaid answer's graph is read exactly from Merlion's data attributes; an SVG answer's graph is recovered from geometry (`src/svg/generic.ts`): a painted closed shape holding a label is a node, a stroke is an edge, a text sitting on a stroke within 12 units of its middle is that edge's label and the outline around it is a chip rather than a node.
- **Legibility** (`src/authoring/probe.js`) is measured in Chromium, because text advance depends on the font the browser resolves. Per drawing: labels whose laid-out ink leaves their shape, pairs of node shapes overlapping, and ink outside the `viewBox`.
- **The reader ceiling** is scored on every run: Merlion's own drawing of each task's declared graph, read back through the same geometry recovery. The graph is known to be right, so the ceiling is the reader's error rather than the drawing's, and it bounds what any hand-written SVG can score.

Only `results/<date>-authoring.md` is committed; the JSON is not.
