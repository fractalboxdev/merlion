# ADR-0006: Stable layout from a hint stored in the previous SVG, in M2

- **Status:** Proposed
- **Date:** 2026-09-22

## Context

Every existing Mermaid renderer lays out each version from scratch. A one-edge change can reorder whole layers, which makes committed diagram diffs unreadable and disorients readers during live editing or while an LLM iterates on a diagram. The Constrained Incremental Graph Drawing Problem (Charytitsch and Nascimento, EJOR 2026) formalises adding to a layered drawing while limiting how far existing nodes move.

## Decision

Stable layout, as specified in [layout.md](../layout.md#stable-layout), ships in M2 instead of last. The previous layout travels inside the previous SVG (`data-merlion-layout`), so stability needs no extra file or state and works directly on a git history of committed SVGs.

## Options considered

| Option | Buys | Costs |
|---|---|---|
| **Hint inside the SVG, M2** | The one capability no competitor has, shipped early; no sidecar files | Longer early milestones; the hint format is a public format from M2 |
| Sidecar layout file (`.merlion.json`) | Keeps the SVG smaller | An extra file per diagram that can drift from the SVG |
| Stable layout last (M5) | Faster first release | The main differentiator arrives last |
| Seeded GRASP heuristic from the paper | Better stability and crossings together | Heavier; unnecessary until a plain constrained sweep fails the benchmark |

## Criteria

Differentiation, operational simplicity (no extra state), and delivery time. Differentiation and simplicity favour the decision; delivery time is the cost.

## Consequences

- Easier: re-rendering in place is stable by default (`--hint` defaults to the existing output).
- Harder: output depends on history. Two renders of the same source can differ if their hints differ, so tests must pin the hint, and the hint is untrusted input that the core must parse defensively ([security.md](../security.md)).
- Expensive to reverse: the `v1` hint format once published.
