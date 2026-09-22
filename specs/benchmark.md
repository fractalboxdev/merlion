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
| `compat` | Demo and test diagrams from the mermaid repository, pinned commit | MIT, vendored with the notice |
| `docs` | 200+ real diagrams collected from public MIT/Apache/CC-BY documentation repositories, each with its source and licence recorded | Per diagram |
| `llm` | MermaidSeqBench (132 cases) plus generated diagrams with known syntax errors | Apache-2.0, fetched at run time |
| `edits` | Pairs (diagram, diagram after a one-line edit): add node, add edge, remove edge, rename label | Original |

## Metrics

| Dimension | Metric | Notes |
|---|---|---|
| Compatibility | % of `compat` rendering the same graph as mermaid, per diagram type | Graph extracted from both SVGs (below) |
| Round-trip correctness | Node and path alignment between the source graph and the graph extracted from the SVG | After DiagramEval (Liang and You, EMNLP 2025); catches dropped edges and wrong labels |
| Layout quality | Crossings; maximum crossings on one edge; bends; total edge length; area; label overlaps; **stress** | Stress: people can perceive it, prefer low values, and trace paths faster as it falls (Mooney et al., GD 2025) |
| Fit | % of `docs` diagrams fitting 720 px without zoom; aspect ratio | |
| Stability | Mean and p95 displacement of surviving nodes across `edits` | Relative to the diagram's bounding box |
| Parser tolerance | Parse rate and repair rate on `llm` | Only Merlion repairs; the other renderers score on parse rate |
| Style loss | Share of `docs` diagrams with at least one `W010 StyleRejected`, and the rejected properties by frequency | The cost of the style allow-list ([svg-output.md](svg-output.md#source-styles-classdef-style-linkstyle)); a property used in more than 5% of diagrams is a candidate for the allow-list |
| Speed | p50/p95 render time per diagram type, cold and warm; fuel used per diagram | Native and WASM, measured separately; the fuel-to-time ratio calibrates the `fuel` default ([ADR-0008](adr/0008-deterministic-work-budget.md)) |
| Determinism | % of `compat` and of the core's sequence fixtures byte-identical between native and WASM, in each font mode | `pnpm bench determinism --corpus compat\|sequence --font link\|embed\|system`; the compat corpus holds no sequence diagram, so sequences are checked over the fixtures. Must be 100% |
| Weight | Gzip size per published artifact, including `merlion-themes.css`; runtime dependency count; packages in the install tree | |
| Accessibility / SEO | axe violations on the inlined SVG; share of label text extractable by `curl` + HTML-to-text; `<title>`/`<desc>` present | |
| Theme switch | Cost of a light→dark switch: re-render vs CSS change | |
| Stylesheet parity | Per corpus diagram, theme and element: the plain SVG inlined in Chrome with the compiled CSS linked is the reference; the baked SVG in Chrome with no host CSS, the baked SVG with its `<style>` removed, and the baked SVG through rsvg-convert must each match it | Colours normalised to 8-bit sRGB through a canvas `fillStyle` round trip; ±1 per channel; rsvg-convert compared by pixels sampled inside each shape, on each stroke and inside each marker, against a screenshot of the reference; a sample counts only where the reference shows the element's own opaque paint, and stroke samples stay 1 px inside a dash. Glyph outlines differ between Chromium's and librsvg's font stacks, so text is compared by ink: inside each text run's box, a pixel within ±8 per channel of the run's computed fill must appear in the rsvg-convert raster whenever it appears in the reference screenshot (runs with no fully covered glyph pixel in the reference are counted and skipped). Inputs are the `compat` corpus plus role fixtures; the gate's stylesheet compiles under `--strict` and covers every built-in role and a re-themed `classDef`. Every named theme of `merlion-themes.css` bakes with no warning. Runs as `pnpm bench parity` ([bench/README.md](../bench/README.md#stylesheet-parity)). Must be 100% |

## Reader preference

TODO(owner): approve the panel ([ADR-0007](adr/0007-reader-panel.md), Proposed).

- 20+ readers, each shown blind pairs of renders of the same source from two renderers, answering "which is clearer?"
- A task subset asks readers to answer a reachability question ("can X reach Y?"); the benchmark records time and accuracy.
- The benchmark reports win rates with 95% confidence intervals. A difference whose intervals overlap is reported as no difference.

## Output

Each run writes `bench/results/<date>-<commit>.json` and a Markdown summary, and CI publishes the summary. Graph extraction from SVGs uses `<text>` positions and edge endpoints; labels that are HTML inside `<foreignObject>` (mermaid diagrams whose own config turns `htmlLabels` back on) are read from that element's text, with a space at every `<br>` and block boundary. Compatibility compares labels with whitespace removed, because renderers wrap long labels at different points.
