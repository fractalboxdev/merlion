# Roadmap

Effort estimates are for one engineer working full time and are rough (±50%).

| Milestone | Scope | Exit criteria | Effort |
|---|---|---|---|
| M0 | Benchmark harness, corpora, baseline scores for mermaid (ELK and dagre), beautiful-mermaid, mmdr, MSAGL.js | Baseline report published; [ADR-0004](adr/0004-layout-implementation.md) confirmed or revised by MSAGL's scores; the p99 of every limit in [architecture.md](architecture.md#boundaries) over `docs` recorded, and each default at least 10× that p99; `W010 StyleRejected` count over `docs` recorded | 1–2 weeks |
| M1 | Flowchart: parser with repairs, Inter metric tables, layered layout (dominator layering, three-pass crossing minimisation, Brandes–Köpf, container fit, orthogonal routing, clusters), SVG contract, outline, CLI | 0 runtime dependencies in CI; `compat` pass rate for flowcharts ≥ 90%; layout quality at least equal to mermaid-ELK on crossings and stress; native and WASM output byte-identical on `compat`; every fuzz target in [security.md](security.md#fuzzing) in CI | 8–12 weeks |
| M2 | Stable layout ([ADR-0006](adr/0006-stable-layout.md)); `@fractalboxdev/merlion-wasm`; `@fractalboxdev/merlion-rehype`; `@fractalboxdev/merlion-astro` | Mean displacement across `edits` below mermaid-ELK's; WASM size recorded; fuel default calibrated ([ADR-0008](adr/0008-deterministic-work-budget.md)) | 3–4 weeks |
| M3 | `<merlion-view>`, semantic zoom; types with fixed geometry: gantt, timeline, pie, XY, packet, kanban | Viewer ≤ 6 KB gzip; `compat` ≥ 90% for each new type | 4–6 weeks |
| M4 | Sequence, state, class, ER | `compat` ≥ 90% for each | 4–6 weeks |
| M5 | Sleeve routing (`edge_style: spline`); first reader panel ([ADR-0007](adr/0007-reader-panel.md)) | Panel results published | 4–6 weeks |
| M5b | `merlion lsp`; `merlion-vscode` (VS Code Marketplace and Open VSX, serving Cursor); Zed extension for the language server | Extension renders every `compat` flowchart in the preview; theme follows the editor | 3–4 weeks |
| M6 | 1.0: stable public API, token names, hint format `v1`, WASM ABI | Reproducible release verified by a second build on a different runner image; `SECURITY.md` published | 2 weeks |

Mindmap, gitGraph, C4, architecture, block, sankey, quadrant, requirement, journey, radar and treemap are outside 1.0. TODO(owner): rank them after M4 by frequency in the `docs` corpus.
