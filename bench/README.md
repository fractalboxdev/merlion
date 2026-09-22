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
pnpm test && pnpm typecheck
```

`run` writes `results/<date>-<commit>.json`; `report` writes the Markdown summary next to the newest results file. `--out-svgs` writes every drawing to `results/svgs/<renderer>/<name>.svg`. Only `results/baseline-*.md`, `results/<date>-baseline.md` and `results/<date>-round<n>.md` are committed.

## Corpora

| Corpus | Source | Count |
|---|---|---|
| `compat` | mermaid at tag `mermaid@12.0.0` (commit `98a0945418c7`): `<pre class="mermaid">` blocks in `demos/*.html`, `mermaid` / `mermaid-example` fences in `packages/mermaid/src/docs/syntax/flowchart.md`, static template literals in `e2e/rendering/flowchart/*.spec.*`, and `e2e/diagrams/flowchart/**/*.mmd` except `handdrawn/`. Flowchart and graph diagrams only, de-duplicated by content | 390 |
| `edits` | The first 30 `compat` diagrams (by name) with at least three simple edge lines, each edited four ways: add an isolated node, add an edge between the farthest-apart unconnected pair, remove the last simple edge (re-declaring endpoints it declared), rename the first `id[Label]` | 106 pairs |

`corpus/compat/manifest.json` pins every diagram by source path, source blob and sha256. The mermaid MIT notice is in [NOTICES.md](NOTICES.md).

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
