# Merlion specs

Merlion renders Mermaid diagrams to static, themeable, accessible SVG. It is a Rust core with no runtime dependencies. The same code ships as a native CLI and as a WebAssembly (WASM) module, with a thin rehype plugin for build-time rendering and an optional web component for zooming.

## What it guarantees

| Property | Contract |
|---|---|
| Output | One self-contained SVG per diagram: labels are `<text>`, colours are CSS custom properties over presentation-attribute defaults, with `role="img"`, `<title>` and a generated `<desc>` |
| Safety | The SVG can be inlined without a sanitiser even when the source or the stylesheet is untrusted: no scripts, no source or stylesheet CSS text, no selectors that reach the host page |
| Theming | Light, dark and custom themes switch by CSS alone; there is no re-render. One stylesheet themes roles and `classDef` colours on the page and, baked, in standalone SVG |
| Runtime | Renders without a DOM or a browser; server and browser output are byte-identical for the same source, options and layout hint |
| Dependencies | 0 runtime crates and 0 runtime npm packages |
| Stability | Given the previous render as a layout hint, nodes that keep their layer across an edit keep their order and move at most `stability` (default 2) positions; the hint is dropped when fewer than half the nodes survive |
| Fit | Layout takes the container width as an input and fits it |

## Documents

| Spec | Covers |
|---|---|
| [architecture.md](architecture.md) | Pipeline stages, packages, build targets |
| [parser.md](parser.md) | Supported syntax, error tolerance, diagnostics |
| [text-measurement.md](text-measurement.md) | Font metric tables, label sizing |
| [layout.md](layout.md) | Layered layout, crossing minimisation, container fit, stable layout, edge routing |
| [svg-output.md](svg-output.md) | The SVG contract: CSS variables, roles, the stylesheet subset and baked palettes, accessibility, ids, data attributes, escaping |
| [security.md](security.md) | Threat model, output safety, resource bounds, fuzzing |
| [viewer.md](viewer.md) | `<merlion-view>` pan, zoom, semantic zoom |
| [interaction.md](interaction.md) | Hover and focus highlighting: CSS rules in the SVG, the viewer's `interact` module, popover, keyboard, touch |
| [integrations.md](integrations.md) | CLI, WASM package, rehype plugin, Astro |
| [supply-chain.md](supply-chain.md) | Dependency policy, release provenance |
| [licensing.md](licensing.md) | Project licence; every project, dataset and tool Merlion uses, and its terms |
| [reference.md](reference.md) | Projects cited but not used, and the constraint on each if it is used later |
| [benchmark.md](benchmark.md) | How "better" is measured |
| [roadmap.md](roadmap.md) | Milestones |
| [adr/](adr/) | Decisions and their trade-offs |
| [research/](research/) | Landscape of existing renderers and the academic literature (dated snapshots) |
