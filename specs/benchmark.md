# Benchmark

Merlion claims to beat an existing renderer only where this benchmark shows it, and publishes every run. No single number decides "better": quality metrics alone can rate a poor drawing as good (van Wageningen, Mchedlidze, Telea, GD 2025), so metrics are always reported beside what readers prefer.

## Baselines

| Renderer | Version | How it runs |
|---|---|---|
| mermaid, ELK layout | 12.0.0 | Headless Chromium through Playwright (development dependency) |
| mermaid, dagre layout | 12.0.0 | Same, with `layout: dagre` |
| beautiful-mermaid | 1.1.3 | Node |
| mermaid-rs-renderer (mmdr) | pinned commit | Native binary |
| merman | 0.8.0-alpha.6, default features (no ELK) | Native, through its Rust API |
| MSAGL.js layered layout | `@msagl/core` 1.1.24 | Node, for layout-only comparison |
| librsvg (`rsvg-convert`) | pinned version | Native binary; parity target for baked SVGs, not a compared renderer |

Baselines are run, never modified or vendored ([licensing.md](licensing.md)).

## Corpora

| Corpus | Contents | Licence |
|---|---|---|
| `compat` | Flowcharts from the mermaid repository's demos, syntax documentation and end-to-end tests, pinned commit | MIT, vendored with the notice |
| `compat-sequence` | The same sources' sequence diagrams | MIT, vendored with the notice |
| `compat-state` | The same sources' `stateDiagram` and `stateDiagram-v2` diagrams | MIT, vendored with the notice |
| `docs` | 200+ real diagrams collected from public MIT/Apache/CC-BY documentation repositories, each with its source and licence recorded | Per diagram |
| `llm` | MermaidSeqBench (132 cases) plus generated diagrams with known syntax errors | Apache-2.0, fetched at run time |
| `edits` | Pairs (diagram, diagram after a one-line edit): add node, add edge, remove edge, rename label | Original |
| `authoring` | 18 diagrams stated in prose beside the graph they mean, and the answers models gave when asked for each one twice — once as Mermaid, once as SVG | Original; the answers are recorded, not fetched at run time |

A diagram is extracted as the browser would read it: an end-to-end `.mmd` fixture is inserted into an HTML page by mermaid's own harness, so its entities are decoded before the parser sees them. `state f &lt;&lt;fork&gt;&gt;` left encoded reaches the grammar as text and is dropped, which is how four fork and join fixtures came to sit in `compat-state` exercising no bar while still counting towards the pass rate.

## Metrics

| Dimension | Metric | Notes |
|---|---|---|
| Compatibility | % of `compat` rendering the same graph as mermaid, per diagram type | Graph extracted from both SVGs (below) |
| Round-trip correctness | Node and path alignment between the source graph and the graph extracted from the SVG | After DiagramEval (Liang and You, EMNLP 2025); catches dropped edges and wrong labels |
| Layout quality | Crossings; maximum crossings on one edge; bends; total edge length; area; label overlaps; edges through another edge's label chip; **stress** | Stress: people can perceive it, prefer low values, and trace paths faster as it falls (Mooney et al., GD 2025). An edge painted across a chip leaves the text under it unreadable while the chips never touch, so box-against-box overlap alone reports a drawing as clean that a reader cannot follow |
| Fit | % of `docs` diagrams fitting 720 px without zoom; aspect ratio | |
| Stability | Mean and p95 displacement of surviving nodes across `edits` | Relative to the diagram's bounding box |
| Parser tolerance | Parse rate and repair rate on `llm` | Only Merlion repairs; the other renderers score on parse rate |
| Authoring format | Cost, validity, fidelity and legibility of a model's Mermaid answer beside its SVG answer for the same diagram | The `authoring` corpus, and the reader ceiling every SVG answer is read against ([below](#authoring)) |
| Style loss | Share of `docs` diagrams with at least one `W010 StyleRejected`, and the rejected properties by frequency | The cost of the style allow-list ([svg-output.md](svg-output.md#source-styles-classdef-style-linkstyle)); a property used in more than 5% of diagrams is a candidate for the allow-list |
| Speed | p50/p95 render time per diagram type, cold and warm; fuel used per diagram | Native and WASM, measured separately; the fuel-to-time ratio calibrates the `fuel` default ([ADR-0008](adr/0008-deterministic-work-budget.md)) |
| Determinism | % of `compat` and of the core's sequence and state fixtures byte-identical between native and WASM, in each font mode | `pnpm bench determinism --corpus compat\|compat-sequence\|compat-state\|sequence\|state --font link\|embed\|system`; the compat corpus holds no sequence and no state diagram, so those are checked over the fixtures. Must be 100% |
| Weight | Gzip size per published artifact, including `merlion-themes.css`; runtime dependency count; packages in the install tree | |
| Accessibility / SEO | axe violations on the inlined SVG; share of label text extractable by `curl` + HTML-to-text; `<title>`/`<desc>` present | |
| Theme switch | Cost of a light→dark switch: re-render vs CSS change | |
| Stylesheet parity | Per corpus diagram, theme and element: the plain SVG inlined in Chrome with the compiled CSS linked is the reference; the baked SVG in Chrome with no host CSS, the baked SVG with its `<style>` removed, and the baked SVG through rsvg-convert must each match it | Colours normalised to 8-bit sRGB through a canvas `fillStyle` round trip; ±1 per channel; rsvg-convert compared by pixels sampled inside each shape, on each stroke and inside each marker, against a screenshot of the reference; a sample counts only where the reference shows the element's own opaque paint, and stroke samples stay 1 px inside a dash. Glyph outlines differ between Chromium's and librsvg's font stacks, so text is compared by ink: inside each text run's box, a pixel within ±8 per channel of the run's computed fill must appear in the rsvg-convert raster whenever it appears in the reference screenshot (runs with no fully covered glyph pixel in the reference are counted and skipped). Inputs are the `compat` corpus plus role fixtures; the gate's stylesheet compiles under `--strict` and covers every built-in role and a re-themed `classDef`. Every named theme of `merlion-themes.css` bakes with no warning. Runs as `pnpm bench parity` ([bench/README.md](../bench/README.md#stylesheet-parity)). Must be 100% |

## Authoring

Merlion exists because a model writing a diagram writes Mermaid, not SVG. The `authoring` corpus is where that claim is measured rather than asserted: which output format serves a model better when it is asked for the same diagram, under the same requirement, in one format and then the other.

The corpus holds 18 tasks, from 5 nodes to 19. Each states a diagram in prose and declares the same diagram as data — nodes, edges and edge labels. The prose quotes every node label exactly as the reference spells it, so a model that follows the prose scores full node fidelity in either format and the comparison measures the format, not the model's invention. Edge labels are scored apart from topology, because the prose names a condition without fixing its wording.

Both prompts carry the same requirement: *every label sits inside the shape it names, no two shapes overlap, and every connection is drawn as an arrow between the right two shapes*. Asking only the SVG side for a readable drawing would be a loaded comparison. Asking both is the point — the requirement is what a layout engine satisfies for the Mermaid answer and what the model has to satisfy itself for the SVG answer.

Generation and scoring are separate. `pnpm bench authoring generate` records a model's answers in `bench/corpus/authoring/outputs/<model>.json` and `pnpm bench authoring score` reads those files, so the same answers are re-scored whenever a metric or the extractor changes and the numbers move only when Merlion does. A Mermaid answer becomes a drawing through Merlion; an SVG answer is the drawing. Both are then read the same way.

The corpus takes models across the capability range, because the cost of writing SVG is not the same at both ends of it. Placing geometry has a floor a model is either above or below, while stating a graph only gets longer as the diagram grows; a result from one frontier model would say nothing about where that floor sits. Providers cover the range without a common account: `claude-cli` uses the local CLI, `anthropic` the Messages API, and `openai-compatible` any local LM Studio, Ollama or vLLM server. Where a model reasons before answering, the reasoning tokens are recorded beside the total and reported apart from it, because a reasoning model bills its thinking as output and a comparison against a model that does not think would otherwise be measuring the wrong thing.

### What is measured

| Dimension | Metric | Notes |
|---|---|---|
| Cost | Output tokens the provider reports, and source bytes, per diagram and per node plus edge | The provider's own count; the `claude-cli` provider's input count covers the CLI's system prompt as well as the task, so only its output count is comparable |
| Validity | Share of answers holding a diagram in the requested format that draws at all | A Mermaid answer also records whether mermaid 12.0.0 itself parses it, which separates Merlion's tolerance from validity |
| Fidelity | Node F1 and edge F1 against the task's declared graph, plus the share of labelled edges drawn with the right label | Matched on labels, never ids: a model picks its own ids in Mermaid and has none in SVG |
| Legibility | Share of labels whose laid-out ink leaves the shape they sit in; pairs of painted shapes overlapping; ink outside the `viewBox`; labels touching no shape at all | Measured in Chromium, because text advance depends on the font the browser resolves and nothing in the SVG source records it. A label's shape is the smallest one holding its centre, or, when none does, the smallest one it still touches — a label that has slid off its box is the worst overflow there is, not an absent one. Text touching nothing is a title or a chipless edge label and is counted apart rather than scored as a fault. A drawing the probe cannot measure is excluded and counted, never scored as clean |
| Reader ceiling | The same fidelity and legibility over Merlion's own drawing of each declared graph, read back through the same geometry recovery | Below |

### Verifiability, and the reader ceiling

The two formats are not equally checkable, and that asymmetry is the finding rather than a flaw in the method. A Mermaid source *states* its graph: parsing it yields the nodes and edges the author declared, and comparing that against the requested graph is exact. An SVG states coordinates. Its graph has to be *recovered* — a painted closed shape holding a label is a node, a stroke between two shapes is an edge, a text sitting on a stroke is that edge's label — which is the method DiagramEval uses to score generated diagrams (Liang and You, EMNLP 2025), and which is lossy however carefully it is written.

So every run scores a ceiling: Merlion's drawing of each task's declared graph, read back through that same geometry recovery (`bench/src/svg/generic.ts`). The graph is known to be right, so whatever the ceiling loses is the reader's error and not the drawing's, and no SVG answer is credited or blamed for it. A drawing that scores at the ceiling is as good as the reader can tell; the gap below it is the drawing's.

Recovery is calibrated on that ceiling, which is calibration against Merlion's own output and is stated rather than hidden. The direction is the safe one: the thresholds are set until a drawing known to be correct reads back correctly, so the reader errs towards accepting a drawing, never towards failing one it should have read. Over the 18 reference drawings it recovers every node and every edge, and 66 of 68 edge labels.

The ceiling is also the honest limit on the whole exercise. A hand-written SVG can only ever be checked as well as a reader can read it back, while a Mermaid source can be checked exactly, by a parser, before anything is drawn. That is the difference the corpus is here to price.

## Reader preference

TODO(owner): approve the panel ([ADR-0007](adr/0007-reader-panel.md), Proposed).

- 20+ readers, each shown blind pairs of renders of the same source from two renderers, answering "which is clearer?"
- A task subset asks readers to answer a reachability question ("can X reach Y?"); the benchmark records time and accuracy.
- The benchmark reports win rates with 95% confidence intervals. A difference whose intervals overlap is reported as no difference.

## Output

Each run writes `bench/results/<date>-<commit>.json` and a Markdown summary, and CI publishes the summary. Graph extraction from SVGs uses `<text>` positions and edge endpoints; labels that are HTML inside `<foreignObject>` (mermaid diagrams whose own config turns `htmlLabels` back on) are read from that element's text, with a space at every `<br>` and block boundary. Compatibility compares labels with whitespace removed, because renderers wrap long labels at different points.
