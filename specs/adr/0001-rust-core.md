# ADR-0001: Rust core, shipped as a CLI and as WASM

- **Status:** Accepted
- **Date:** 2026-09-22

## Context

The renderer must run at build time without a browser, in the browser for live editing, and produce identical output in both places. mermaid-cli launches headless Chromium for every diagram, which adds 2–3 s of startup; mermaid-rs-renderer (mmdr) reports 1.5–2.5 ms per graph diagram as native Rust.

## Decision

The parser, text measurement, layout and drawing are one `no_std` + `alloc` Rust library. It is published as a native CLI and as a `.wasm` module with hand-written JS glue. DOM work (the viewer) and framework glue (rehype, Astro) are TypeScript.

## Options considered

| Option | Buys | Costs |
|---|---|---|
| **Rust core → native + WASM** | One codebase for all targets; native speed; a single static binary for CI; zero runtime dependencies is achievable | Smaller contributor pool than TypeScript; WASM needs an async `init()` in the browser; three release pipelines (crates.io, npm, binaries) |
| TypeScript core | Largest contributor pool; runs natively in Node and the browser | Build-time rendering needs Node; slower layout; a zero-dependency policy is harder to hold in the npm ecosystem |
| Go core | Simple single binary | WASM output is large (the Go runtime ships with it); weak `no_std` story |

## Criteria

Output identical across targets, build-time rendering without Node or Chromium, and zero runtime dependencies. Identical output decided it: only a single compiled core gives the same bytes on the server and in the browser without maintaining two implementations.

## Consequences

- Easier: CI and static-site generators that don't use Node (Hugo, Zola) call one binary.
- Harder: layout debugging needs its own visual tooling (a debug SVG overlay drawn by the core). WASM file size is unmeasured and must be tracked from M1.
- Expensive to reverse: the core's public API and the WASM ABI, once `@fractalboxdev/merlion-wasm` 1.0 ships.
