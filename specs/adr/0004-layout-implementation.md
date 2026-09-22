# ADR-0004: Source of the layered layout implementation

- **Status:** Proposed
- **Date:** 2026-09-22

## Context

Zero runtime dependencies in a Rust core ([ADR-0001](0001-rust-core.md), [ADR-0002](0002-zero-runtime-dependencies.md)) rule out `elkjs`, which is JavaScript anyway. The layered layout ([layout.md](../layout.md)) is the largest component, and it needs incremental and stable variants no existing engine exposes in the form specified.

## Decision

Port the layered pipeline of MSAGL.js (`@msagl/core`, MIT, © Microsoft Corporation) to Rust as the base, replacing its 4 small dependencies with the standard library. Implement the additions (dominator layering, exact refinement, the local objective, container fit, stable layout, sleeve routing) from the papers cited in [layout.md](../layout.md). Every ported file carries the Microsoft MIT notice ([licensing.md](../licensing.md)).

## Options considered

| Option | Buys | Costs |
|---|---|---|
| **Port MSAGL.js layered layout (MIT)** | A proven Sugiyama and Brandes–Köpf implementation in readable TypeScript; MSAGL also contains sleeve routing and incremental layout; the licence allows porting with a notice | The design follows MSAGL's internal structure; ported code keeps Microsoft's copyright line; MSAGL is not tuned for Mermaid's small, labelled diagrams |
| Implement from the papers alone | Complete control; one copyright holder | Months more work; every known pitfall of Sugiyama implementations (dummy-node handling, compound nodes, Brandes–Köpf corner cases) is rediscovered |
| Port ELK layered (EPL-2.0) | The best open-source layered layout | Each ported file becomes EPL-2.0, which conflicts with Merlion's MIT licence. Rejected on licence. |
| Adopt mmdr's layout (MIT) | Already Rust | Algorithm undocumented; project describes itself as early; still worth benchmarking as a baseline |

## Criteria

Licence compatibility, time to a correct M1, and quality at Mermaid scale. The licence ruled out ELK. Between MSAGL and implementing from scratch, time to M1 decides it, and MSAGL wins by the size of the proven pipeline.

## Consequences

- Easier: M1 layout correctness; the benchmark gets an MSAGL layout-only baseline for free.
- Harder: `THIRD_PARTY_NOTICES.md` and per-file headers must be maintained. Divergence from MSAGL upstream is permanent; fixes don't flow back automatically.
- Expensive to reverse: layout behaviour once users rely on stable layouts. A rewrite changes every committed SVG.
- Uncertainty: MSAGL's quality on Mermaid-sized flowcharts is unmeasured. M0 benchmarks it against ELK and dagre before the port starts, and the decision is revisited if it scores clearly worse.
