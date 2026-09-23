# Authoring: Mermaid or SVG

18 tasks (corpus version 1), each asked of every model twice — once as a Mermaid flowchart, once as a standalone SVG — with the same description and the same readability requirement. Scored 2026-09-23. Method: [specs/benchmark.md](../../specs/benchmark.md#authoring).

## Reader ceiling

Merlion's own drawing of each task's declared graph, read back through the same geometry recovery every hand-written SVG is read through. The graph is known to be right, so what this loses is the reader's error, not the drawing's.

| Node F1 | Edge F1 | Labels overflowing their shape | Shape overlaps | Clipped |
|---|---|---|---|---|
| 1.000 | 1.000 | 0.0% | 0 | 0 |

No recorded answers. `pnpm bench authoring generate --provider <p> --model <m>` writes one file per model.

