# ADR-0002: Zero runtime dependencies

- **Status:** Accepted
- **Date:** 2026-09-22

## Context

Installing mermaid 12.0.0 adds 23 direct and 117 transitive npm packages (d3, cytoscape plus two layout plugins, katex, marked, dompurify, chevrotain, roughjs, dayjs, uuid, and more). Most of them serve runtime jobs, such as sanitising HTML labels or rendering maths and Markdown in the browser, that build-time SVG doesn't need. beautiful-mermaid 1.1.3 installs 3 packages, one of which (`elkjs`) is a 1.6 MB GWT-compiled bundle that is impractical to audit. Measured with `npm ls --all` on 2026-09-22.

## Decision

Every published artifact has zero runtime dependencies ([supply-chain.md](../supply-chain.md)). Layout, parsing, argument handling, the WASM bridge and the viewer are all written in-repo. Maths, Markdown-in-labels beyond bold/italic/code, and icons are opt-in hooks the caller provides; hooks return geometry that the core validates, not markup ([svg-output.md](../svg-output.md#caller-hooks)).

## Options considered

| Option | Buys | Costs |
|---|---|---|
| **Zero runtime dependencies** | Nothing third-party ships to users; the whole runtime can be audited in a day; no transitive advisories | Layout engine, WASM glue (~50 lines) and CLI parsing (~100 lines) written by hand; no `elkjs` shortcut for M1 |
| Small audited set (`lexopt`, `wasm-bindgen`, `elkjs`) | Faster M1 | `wasm-bindgen` alone brings ~10 crates, including `syn` and `proc-macro2`; `elkjs` brings EPL-2.0 code and 467 KB gzip |
| No policy | Fastest start | Recreates mermaid's dependency surface |

## Criteria

Supply-chain exposure for users, licence compatibility, and delivery time. Exposure and licence decided it; the cost is delivery time, mostly the layout engine ([ADR-0004](0004-layout-implementation.md)).

## Consequences

- Easier: security review, licence review and reproducible builds.
- Harder: every capability is built and maintained here. M1 takes 8–12 weeks instead of 4–6.
- Expensive to reverse: nothing. Adding a dependency later needs only an ADR.
