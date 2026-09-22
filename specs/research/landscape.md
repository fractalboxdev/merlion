# Landscape of Mermaid renderers

*Snapshot as of 2026-09-22. Figures marked "measured" come from `npm view`, `npm ls --all` and local renders on that date; the rest cite the sources at the end.*

No existing renderer covers all Mermaid types, looks good, switches themes without re-rendering, runs without a browser, has a small dependency tree, and produces accessible, crawlable output. Each gets two or three. merman comes closest on coverage without a browser. It follows mermaid's own theming and SVG structure, so it inherits mermaid's fixed colours and has no stable layout.

## merman

[merman](https://github.com/Latias94/merman) (crate `merman` 0.8.0-alpha.6, `MIT OR Apache-2.0`, 561 GitHub stars) is a headless Rust implementation of Mermaid aiming at parity with `mermaid@11.17.2`: parser, layout, configuration, theming, sanitisation and SVG structure are checked against pinned mermaid source and fixtures.
- Covers all 35 built-in diagram families.
- Cytoscape-style layout by default; ELK layout is an optional feature under EPL-2.0.
- Deterministic built-in text measurer, or a host-supplied measurer.
- WASM and Node bindings.
- Workspace split into `merman-core`, `merman-render`, `merman-ascii`, `merman-export` and others.
- **Zed's built-in Markdown preview renders Mermaid with it** (`crates/mermaid_render` depends on `merman = "=0.8.0-alpha.5"` with `layout-cytoscape` and `svg`).

`merman-core` 0.8.0-alpha.6 alone has 16 required runtime dependencies (including `lol_html`, `lalrpop-util`, `json5`, `url`, `serde`, `serde_json`, `indexmap`, `euclid`) plus 4 optional ones; the transitive count hasn't been measured. Its accessibility output hasn't been checked.

| | mermaid 12.0.0 | beautiful-mermaid 1.1.3 | mermaid-rs-renderer (mmdr) | D2 + TALA |
|---|---|---|---|---|
| Diagram types | ~25 | 6: flowchart, state, sequence, class, ER, XY (measured: gantt, mindmap, pie, timeline and gitGraph fail with `Invalid mermaid header`) | 23 claimed | Own language |
| Layout | ELK by default since 12.0.0 (2026-09-10); dagre optional | ELK (`elkjs` ^0.11) | Not documented | TALA (proprietary); dagre/ELK free |
| Theming | Colours fixed at render; `base` + `themeVariables`; `neo` look and `redux-color` themes are the 12.0.0 default | CSS variables; switch without re-render; 15 themes | `themeVariables`; no CSS variables documented | Built-in themes, fixed at render |
| Needs a DOM | Yes; server rendering needs Playwright/Chromium | No; synchronous | No; native | No; Go binary |
| Speed | mermaid-cli: 2–3 s Chromium startup per diagram | Measured: flowchart with subgraph 72 ms (first call, ELK warm-up); sequence 0.7 ms | 1.5–2.5 ms per graph diagram (author's benchmark, 2026-02-02) | — |
| Size | Measured: `mermaid.min.js` 5.58 MB raw / 1.59 MB gzip; ESM build loads per diagram type | Measured: `elk.bundled.js` 1.61 MB / 467 KB gzip | Native | Native |
| Dependencies (measured) | 23 direct, 117 transitive | 2 direct (`elkjs`, `entities`), 3 transitive | Not checked | Not checked |
| Labels | `<foreignObject>` HTML by default | `<text>` (measured) | Not checked | `<text>` |
| Accessibility | Only via author-written `accTitle`/`accDescr` | No `role`, no `<title>` (measured); has `viewBox` | Not checked | Not checked |
| Browser/runtime floor | ES2024, Safari 17.4+, Node ≥ 22.12 | — | — | — |
| Licence | MIT | MIT | MIT | MPL-2.0; TALA proprietary |

Other components:

- **MSAGL.js** (`@msagl/core` 1.1.24, MIT, last published 2026-04-24): layered Sugiyama layout, incremental layout, sleeve routing, semantic zoom. `@msagl/core` has 4 direct dependencies; with `@msagl/renderer-svg` the install is 19 packages (measured).
- **rehype-mermaid** (MIT): strategies `img-png`, `img-svg`, `inline-svg` (default), `pre-mermaid`. Its `dark` option emits `<picture>` and works only with the `img-*` strategies. Renders through Playwright outside the browser.
- **Pan and zoom:** `@panzoom/panzoom` 4.6.2 (MIT, 0 dependencies, published 2026-04-02); `svg-pan-zoom` 3.6.2 (BSD-2-Clause, last published 2024-10).
- **Mermaid's CSS-variable proposal:** PR #8008 closed unmerged, superseded by PR #8265 (`cssVariableTheme`, `webCompatibility`), which is open. Review flagged unsanitised `prefix`/`preserveAspectRatio` values, colours rewritten inside text nodes, and `viewBox` derived wrongly from percentage dimensions.
- **unified pipeline:** `unified` + `remark-parse` + `remark-rehype` + `rehype-stringify` install 63 packages (measured). Sites on Astro or MDX already have them.

## Gaps nobody fills

1. Theme switching without re-rendering, across all diagram types.
2. Full type coverage and polished output at once.
3. Full coverage without a DOM *and* with a small dependency tree. merman renders all 35 types without a DOM but needs 16+ runtime crates; beautiful-mermaid is small but covers 6 types.
4. Layout that stays stable across edits.
5. Accessibility metadata and a text alternative by default.
6. A small dependency tree.
7. Zoom beyond a bolt-on: semantic zoom, keyboard access.
8. Tolerance for the small syntax errors LLMs make.

## Sources

- [mermaid 12.0.0 upgrade notes (docz-site#30)](https://github.com/donaldgifford/docz-site/issues/30)
- [mermaid theming](https://mermaid.ai/open-source/config/theming.html) · [PR #8008](https://github.com/mermaid-js/mermaid/pull/8008) · [PR #8265](https://github.com/mermaid-js/mermaid/pull/8265)
- [beautiful-mermaid README](https://github.com/lukilabs/beautiful-mermaid/blob/main/README.md)
- [mermaid-rs-renderer README](https://github.com/1jehuang/mermaid-rs-renderer/blob/master/README.md)
- [rehype-mermaid](https://github.com/remcohaszing/rehype-mermaid)
- [D2 layouts](https://d2lang.com/tour/layouts/)
- [merman](https://github.com/Latias94/merman) and crates.io API for `merman`, `merman-core`
- Zed `crates/mermaid_render/Cargo.toml` (main branch)
- npm registry and crates.io API, measured 2026-09-22
