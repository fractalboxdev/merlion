---
title: Architecture
description: The packages Merlion ships, how they depend on each other, and the four-stage pipeline every render runs on every target.
---

Merlion is one Rust library, `merlion-render`, wrapped for each place a diagram gets rendered. The library is `#![no_std]` with `alloc` and has zero dependencies; everything else is a thin surface over it.

## Packages

```mermaid
flowchart RL
  accTitle: Merlion packages and their dependencies
  core["**merlion-render**<br/>no_std core, 0 deps"]
  subgraph rust["crates.io"]
    cli["**merlion-cli**<br/>the merlion command"]
    wasmcrate["**merlion-wasm**<br/>extern C exports, not published"]
  end
  subgraph npm["npm @fractalboxdev"]
    wasm["**merlion-wasm**<br/>.wasm + hand-written glue"]
    rehype["**merlion-rehype**<br/>rehype and Sätteri plugin"]
    astro["**merlion-astro**<br/>Astro integration"]
    view["**merlion-view**<br/>pan and zoom element"]
    themes["**merlion-themes**<br/>theme tokens, Inter subset"]
  end
  cli --> core
  wasmcrate --> core
  wasm --> wasmcrate
  rehype --> wasm
  astro --> rehype
  astro e1@-.-> view
  astro e2@-.-> themes
  class core accent
  class cli,wasm output
  class wasmcrate muted
  class themes style
  class rust,npm group
  class e1,e2 async
```

Arrows point from a package to what it builds on. Solid arrows are code dependencies; the dashed ones are stylesheets and a script the Astro integration adds to pages. `merlion-view` and `merlion-themes` do not depend on the renderer: the viewer wraps any inline SVG, and the themes are plain CSS.

| Package | Ships | Runtime dependencies |
|---|---|---|
| `merlion-render` | Parse, measure, layout, draw; the stylesheet compiler. No I/O | 0 |
| `merlion-cli` (`merlion`) | Argument parsing, file and stdin I/O, layout hints read from existing output | `merlion-render` only |
| `merlion-wasm` (crate) | Raw `extern "C"` exports; ships only inside the npm package | `merlion-render` only |
| `@fractalboxdev/merlion-wasm` | The `.wasm` file, JavaScript glue, `index.d.ts` (the JavaScript contract) | 0 |
| `@fractalboxdev/merlion-rehype` | Build-time rendering of ```` ```mermaid ```` blocks, for unified and Sätteri | 0 (the WASM package is a peer) |
| `@fractalboxdev/merlion-astro` | Registers the plugin with Astro's Markdown processor, adds theme CSS, the compiled stylesheet and the viewer | 0 (peers) |
| `@fractalboxdev/merlion-view` | `<merlion-view>`, ≤ 6 KB gzipped | 0 |
| `@fractalboxdev/merlion-themes` | `merlion-themes.css`, `merlion-font.css` and the Inter subset | 0 |

CI fails any `Cargo.toml` or `package.json` that adds a runtime dependency ([ADR-0002](/reference/specs/adr/0002-zero-runtime-dependencies/)).

## Pipeline

Every render runs four pure stages. None reads the clock, the environment or the filesystem, so the same source, options and hint give the same bytes.

```mermaid
flowchart LR
  accTitle: The render pipeline
  src[/"**Mermaid source**<br/>UTF-8, ≤ 1 MiB"/] --> parse["**Parse**<br/>model + diagnostics"]
  parse --> measure["**Measure**<br/>label boxes from font tables"]
  measure --> layout["**Layout**<br/>phases 1–7, container fit"]
  hint[("**previous SVG**<br/>data-merlion-layout")] e1@-.-> layout
  layout --> draw["**Draw**<br/>SVG + outline"]
  palette[("**Palette**<br/>from a stylesheet")] e2@-.-> draw
  draw --> out[/"**SVG + outline**<br/>+ diagnostics, fuel used"/]
  class src input
  class parse,layout accent
  class hint store
  class palette style
  class out output
  class e1,e2 async
```

| Stage | Input | Output | Code |
|---|---|---|---|
| Parse | Source, `strict`, limits | `Diagram` model and diagnostics | `crates/merlion-render/src/parse/` |
| Measure | Model, font metric tables | Size of every label and node | `text/`, `layout/measure.rs` |
| Layout | Sized model, options, optional hint | Geometry: node boxes, edge paths, cluster boxes | `layout/` |
| Draw | Geometry, options, optional palette | SVG string and plain-text outline | `svg/` |

The two optional inputs enter at different stages and never cross: the layout hint changes only node order, and the palette changes only colour literals in Draw. A palette never affects measurement, layout or the hint, so a themed standalone SVG has exactly the geometry of the plain one. The stylesheet compiler sits outside the pipeline and runs once per invocation ([Stylesheets](/how-it-works/stylesheet/)).

The full sequence with its limits and fuel is on [Render pipeline](/how-it-works/render-pipeline/); the layout phases each have a page under Layout, starting with [cycle removal](/how-it-works/layout/cycle-removal/).

## Targets

- **Build time** is the primary target: the CLI or the rehype plugin renders during the site build, and pages ship SVG with no renderer ([ADR-0003](/reference/specs/adr/0003-prerender-first/)).
- **Browser** is the secondary target: `@fractalboxdev/merlion-wasm` serves live editors, previews and diagrams written at runtime, such as a model's answer in a chat. The [playground](/playground/) runs it.

## Determinism

Native and WASM output are byte-identical; CI renders the 390-diagram `compat` corpus both ways and fails on any difference. The rules behind that:

- **No clock.** Work is counted in fuel units, never milliseconds ([ADR-0008](/reference/specs/adr/0008-deterministic-work-budget/)).
- **Arithmetic.** Geometry is `f64` with `+ − × ÷` only from hardware; `sqrt` (correctly rounded), `atan2`, `sin`, `cos` and `hypot` are implemented in software, because the `core` versions are unstable and `libm` would be a dependency.
- **Output numbers.** Coordinates round to 1/100 px and print through the core's own formatter: no exponent, no trailing zeros, no `-0`.
- **Collections.** `Vec`, `BTreeMap` and `BTreeSet` only, so no iteration order depends on a hash seed.
- **Ids.** Without `idPrefix`, the SVG id is `m` plus 8 hex digits of FNV-1a 64 over the source, the options, the palette digest and the drawn layout.

Details: [specs/architecture.md](/reference/specs/architecture/#determinism).
