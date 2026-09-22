# Layout

Graph-shaped diagrams (flowchart, state, class, ER) use one layered layout engine. The engine follows the Sugiyama framework with three additions: it fits the container, it keeps nodes stable across edits, and it layers flowcharts by control flow. The origin of the implementation is decided in [ADR-0004](adr/0004-layout-implementation.md). The algorithms are taken from the published literature cited below, never from code under an incompatible licence ([licensing.md](licensing.md)).

## Options

| Option | Default | Meaning |
|---|---|---|
| `target_width` | `720` (px) | Width of the container the SVG will fill at scale 1 |
| `max_aspect` | `1.6` | Maximum height / width that container fit may produce |
| `direction` | From source; `TB` when absent | `auto` lets the engine choose `TB` or `LR` to fit `target_width` |
| `edge_style` | `orthogonal` | `orthogonal`, `polyline`, or `spline` (sleeve routing) |
| `node_spacing` / `rank_spacing` | `24` / `48` (px) | Minimum gaps between nodes and between layers |
| `hint` | none | The previous layout ([Stable layout](#stable-layout)) |
| `stability` | `2` | Largest number of positions a surviving node may move within its layer |
| `fuel` | `20,000,000` | Work limit for all phases ([ADR-0008](adr/0008-deterministic-work-budget.md)) |

## Phases

### 1. Cycle removal

- **Flowcharts.** The engine adds a virtual root with an edge to every entry node and computes the dominator tree from it. Entry nodes are the nodes with no incoming edges; when a strongly connected component of the condensation has no incoming edge from outside it, its first declared node is also an entry. An edge whose target dominates its source is a loop back-edge and is reversed.
- **Remaining cycles.** Irreducible cycles (a jump into the middle of a loop) contain no edge to a dominator, so they survive the step above. So does every cycle in the other graph types. For each strongly connected component that still has a cycle, the engine reverses a feedback arc set chosen by the Eades–Lin–Smyth greedy heuristic.
- Reversed edges are drawn in their original direction and marked as back-edges (`data-merlion-back="true"`).

### 2. Layer assignment

Every forward edge `u → v` gets `layer(v) > layer(u)`.

- **Flowcharts:** longest-path layering from the virtual root over the acyclic graph left by phase 1. A node sits below all of its non-back-edge predecessors, and therefore below its dominators, which keeps the happens-before order readable (VEIL; Schaad, Ben-Nun, Hoefler, 2025). A source (a node with no predecessor) then moves down to one `min_len` above its highest successor, so a second entry point sits next to the node it feeds instead of at the top with a long edge; a source has no predecessor to stay below, so the invariant above still holds. The dominator tree also orders nodes within a layer: phase 3 starts from a depth-first order of the dominator tree, so a node's dominated region stays contiguous.
- **Other graph types:** network simplex (Gansner et al.), minimising total edge length.
- Edges spanning more than one layer get dummy nodes, one per intermediate layer. The layered graph's size and layer count are checked against the limits in [architecture.md](architecture.md#boundaries) before phase 3.

### 3. Crossing minimisation

Three passes. Pass 1 is mandatory; passes 2 and 3 are optional and stop when their fuel runs out.

1. **Layer sweeps.** Alternate downward and upward sweeps ordering each layer by the median of its neighbours' positions, with the barycenter as the tie-breaker, and stop after 24 sweeps or after 4 sweeps without improvement. Transpose adjacent nodes greedily after each sweep.
2. **Exact refinement.** For each adjacent pair of layers with at most `k = 8` crossings between them, solve one-sided crossing minimisation exactly (the other layer fixed) by branch and bound over the pairwise crossing matrix, and keep the result if it lowers the total. One-sided crossing minimisation is fixed-parameter tractable in the number of crossings k, in O(φ^k · n²) time with φ the golden ratio (Dujmović and Whitesides, 2004); at k ≤ 8 the search stays small, and it draws fuel like every other pass. The engine refines one pair at a time and never solves more than two layers jointly: exact multi-layer minimisation has no subexponential algorithm for 5 or more layers unless the Exponential Time Hypothesis fails (Fomin et al., SODA 2026). The `k` threshold is a default to be tuned by the benchmark.
3. **Local objective.** Reorder to lower the maximum number of crossings on any single edge without raising the total, using the median heuristic with the tie-breaking rule of Giannopoulos et al. (2025), which is a 3-approximation for the one-sided local objective.

### 4. Coordinate assignment

Brandes–Köpf horizontal alignment (four alignments, balanced), followed by a compaction step that respects `node_spacing`. Vertical coordinates come from layer heights (the tallest node in each layer) plus `rank_spacing`. The description uses `TB`; the other directions transpose or mirror the result.

### 5. Container fit

A diagram with more than one weakly connected component is laid out one component at a time, and the drawings are packed. A cluster joins the component of its nodes, so a cluster whose nodes lie in several components merges them; a diagram with a cluster that holds no node is laid out as one graph. Each component goes through phases 1–7 and the steps below on its own; the drawings are then packed in declaration order (by each component's first node) along the order axis: `TB`/`BT` components stand in rows `node_spacing` apart, aligned on their layer-0 side, and a component that would pass `target_width` starts a new row `rank_spacing` below; `LR`/`RL` components stack in one column `node_spacing` apart, aligned on their layer-0 side. An edit then moves only its own component and the components after it in the same row. With `direction: auto`, both directions are packed and step 1 chooses between them.

After coordinates are assigned, if the drawing's width exceeds `target_width`, the engine applies these steps in order and stops at the first that fits:

1. With `direction: auto`, lay out in the other direction and keep whichever fits; if both fit, keep the smaller area; if neither fits, continue the steps below with the narrower one.
2. **`LR` / `RL` — wrap the layer sequence.** Layers are columns, so width grows with the number of layers. Split the sequence at the point closest to the midpoint where the fewest edges cross the split, and place the second part below the first. Repeat on the widest part while the result is still too wide and height / width stays at or below `max_aspect`; a wrap that would exceed `max_aspect` is not applied, and the step succeeds only when a wrap reaches `target_width`. Edges that cross a wrap are routed around the outside and marked `data-merlion-wrap="true"`.
3. **`TB` / `BT` — split wide layers.** Layers are rows, so width grows with the widest layer. Split each layer wider than `target_width` into two sub-rows at the position that separates the fewest edges, inserting a pseudo-layer so that edges stay downward. Repeat under the same `max_aspect` rule as step 2.
4. Reduce the label wrap width in steps of 20 px, down to 120 px, and lay out again.
5. When nothing fits, no wrap, split or narrower label wrap is kept: the result is the step-1 drawing, wider than the container, and `<merlion-view>` handles it by zooming ([viewer.md](viewer.md)). A partial wrap or split that stays too wide routes edges around every wrap and splits layers into staircases, and costs more crossings, bends and edge length than zooming the whole drawing.

Aspect ratio is a soft constraint, following ARCOL (Alsuwaykit et al., 2026).

### 6. Edge routing

- `orthogonal`: horizontal and vertical segments through dummy-node positions, with rounded corners (radius 6 px), and ports spread evenly along the node side.
- `polyline`: straight segments through dummy-node positions.
- `spline`: sleeve routing (Nachmanson and Chen, 2026). Triangulate the free space with a constrained Delaunay triangulation, find a sequence of triangles (a "sleeve") from source to target in the dual graph, then take the shortest path through it with the funnel algorithm and smooth it into cubic Béziers. Used for dense diagrams and for edges that cross cluster boundaries. Routing consumes fuel; when it runs out, the remaining edges fall back to `polyline`. Until sleeve routing lands ([roadmap.md](roadmap.md) M5), every `spline` edge is routed and drawn exactly as `polyline`; smoothing the routed polyline alone would overshoot its vertices, leave the viewBox and cross clusters.
- Edge labels sit at the midpoint of the longest segment, on an opaque label chip (`--merlion-edge-label-bg`), moved along the edge until they overlap no node, no cluster title, no other label and neither of the edge's own end markers (the last 10 px of the path, 10 px wide). Placement and the space reserved for a label use the chip's outer size, padding included; a gap crossed by a labelled edge also reserves 10 px per end marker. Labels of edges between neighbouring layers whose chips, centred between the edge's ends, would overlap are stacked: the gap between the layers grows to hold every chip of the group plus the markers at both ends, and each chip takes an equal slot of the part of the gap the markers leave free, on its own edge.

### 7. Clusters (subgraphs)

Clusters are laid out as compound nodes: each cluster's members occupy a contiguous span in every layer they touch, and the cluster box encloses them with 12 px padding plus the title height. An edge that crosses a cluster boundary enters or leaves through a port on the boundary. The title sits in the band at the top of the box, centred unless an edge route crosses its text or a fixed label chip covers it; it then moves along the band, within the padding, to the nearest position that leaves every crossing route room for its label chip, else to the nearest position clear of the routes themselves, else it stays centred. Edge-label chips treat titles as obstacles, like nodes.

## Stable layout

Formulated as the Constrained Incremental Graph Drawing Problem (C-IGDP; Charytitsch and Nascimento, 2026).

- **Hint.** The previous layout's node order per layer, keyed by source node id ([svg-output.md](svg-output.md#layout-hint)). The CLI and rehype plugin read it from the previous SVG; the core takes it as an argument. The hint is untrusted input: a malformed hint, one with an unknown version, or one larger than the node limits is discarded with `I022 LayoutHintInvalid`, never an error.
- **Direction.** With `direction: auto`, the hint's direction is kept unless it no longer fits `target_width` and the other direction does.
- **Surviving nodes.** Nodes present in both the hint and the new source whose layer is unchanged. A node whose layer changed (for example because its dominators changed) is treated as new.
- **Constraint.** Surviving nodes keep their relative order within each layer, and each moves at most `stability` positions. New nodes are inserted wherever crossings are lowest, subject to that constraint.
- **Procedure.** Phase 3 runs with the survivors' order fixed and only the new nodes free, then lets survivors move within the `stability` limit if that lowers crossings.
- **Unchanged source.** When every node survives and the fresh phase 3 already places each at its hinted position, the fresh layout is kept whole. The hint carries real-node order only, so re-ordering under it could still move dummy nodes; keeping the fresh layout makes a render hinted with its own previous SVG byte-identical to that SVG.
- **Fallback.** When fewer than 50% of the nodes survive, the hint is discarded (per component when components are packed, with `I020`/`I021` reporting the totals once) and the result is a fresh layout plus `I020 LayoutHintDiscarded`. When some but not all nodes survive, the render adds `I021 LayoutHintPartial` with the count of nodes treated as new.
- **Measure.** The benchmark reports the mean displacement of surviving nodes after a one-line edit ([benchmark.md](benchmark.md)).

## Fixed-geometry diagram types

Sequence, gantt, timeline, pie, XY, packet and kanban do no graph layout. Each has its own module that computes geometry from the model: for example, sequence participants are columns sized to their widest label and messages are rows ([sequence.md](sequence.md#layout)). These modules share the text measurement, `target_width` fitting (columns shrink to a minimum, then the SVG widens), the fuel counter and the SVG contract.

## References

- K. Sugiyama, S. Tagawa, M. Toda. *Methods for Visual Understanding of Hierarchical System Structures.* IEEE Trans. SMC, 1981.
- U. Brandes, B. Köpf. *Fast and Simple Horizontal Coordinate Assignment.* GD 2001.
- E. R. Gansner, E. Koutsofios, S. C. North, K.-P. Vo. *A Technique for Drawing Directed Graphs.* IEEE TSE, 1993.
- P. Eades, X. Lin, W. F. Smyth. *A Fast and Effective Heuristic for the Feedback Arc Set Problem.* IPL, 1993.
- V. Dujmović, S. Whitesides. *An Efficient Fixed Parameter Tractable Algorithm for 1-Sided Crossing Minimization.* Algorithmica 40:15–31, 2004. [doi:10.1007/s00453-004-1093-2](https://link.springer.com/article/10.1007/s00453-004-1093-2)
- F. V. Fomin et al. *Tight Parameterized (In)tractability of Layered Crossing Minimization: Subexponential Algorithms and Kernelization.* SODA 2026. [arXiv:2510.13335](https://arxiv.org/abs/2510.13335)
- P. Giannopoulos et al. *One-Sided Local Crossing Minimization.* [arXiv:2510.00331](https://arxiv.org/abs/2510.00331)
- P. Schaad, T. Ben-Nun, T. Hoefler. *VEIL: Reading Control Flow Graphs Like Code.* [arXiv:2511.05066](https://arxiv.org/abs/2511.05066)
- Z. Alsuwaykit et al. *ARCOL: Aspect Ratio Constrained Orthogonal Layout.* [arXiv:2603.29618](https://arxiv.org/abs/2603.29618)
- L. Nachmanson, X. Chen. *Browsing Large Graphs with Tile Pyramids and Sleeve Routing in the Browser.* [arXiv:2605.17498](https://arxiv.org/abs/2605.17498)
- B. C. B. Charytitsch, M. C. V. Nascimento. *… the Constrained Incremental Graph Drawing Problem.* EJOR 330(2), 2026. [arXiv:2508.15949](https://arxiv.org/abs/2508.15949)
