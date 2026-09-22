# Architecture decision records

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-rust-core.md) | Rust core, shipped as a CLI and as WASM | Accepted |
| [0002](0002-zero-runtime-dependencies.md) | Zero runtime dependencies | Accepted |
| [0003](0003-prerender-first.md) | Pre-rendering is the primary target | Accepted |
| [0004](0004-layout-implementation.md) | Port MSAGL.js layered layout (MIT) as the base | Proposed |
| [0005](0005-css-variable-theming.md) | CSS custom properties for theming, `<text>` labels | Accepted |
| [0006](0006-stable-layout.md) | Stable layout from a hint in the previous SVG, in M2 | Proposed |
| [0007](0007-reader-panel.md) | Reader-preference panel in the benchmark | Proposed |
| [0008](0008-deterministic-work-budget.md) | Layout work bounded by fuel, not by time | Proposed |
| [0009](0009-stylesheet.md) | One compiled stylesheet for web and standalone diagrams | Proposed |

A superseding decision is a new ADR; the old one keeps its text and changes status to `Superseded by <link>`.
