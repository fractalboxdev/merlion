# ADR-0003: Pre-rendering is the primary target

- **Status:** Accepted
- **Date:** 2026-09-22

## Context

Most Mermaid lives in documentation and content sites whose source is known at build time. Rendering in the browser ships a renderer (mermaid's `mermaid.min.js` is 5.58 MB raw, 1.59 MB gzip) and leaves crawlers that don't run JavaScript seeing only raw Mermaid source. rehype-mermaid's dark-mode option works only with `<img>` output, whose labels are not page text.

## Decision

Build-time rendering to inline SVG is the primary path: the CLI and `@fractalboxdev/merlion-rehype`. The browser WASM build serves only content that doesn't exist at build time: live editors, previews, and diagrams written at runtime.

## Options considered

| Option | Buys | Costs |
|---|---|---|
| **Pre-render first, WASM secondary** | Pages ship no renderer; labels are indexable text; no layout shift | A build step is required; live content still needs WASM |
| Browser-first | No build step | A renderer download on every page; worse SEO; flash of unrendered source |
| Pre-render only | Smallest scope | Rules out editors and LLM chat, where Mermaid is increasingly written |

## Criteria

Page weight, crawlability, and coverage of live-authoring use cases. Page weight and crawlability decided it; keeping WASM as a secondary target keeps live-authoring coverage.

## Consequences

- Easier: SEO and Core Web Vitals; theming stays pure CSS ([ADR-0005](0005-css-variable-theming.md)).
- Harder: the served font must match the measured one ([text-measurement.md](../text-measurement.md)).
- Expensive to reverse: nothing; the WASM target keeps the browser path open.
