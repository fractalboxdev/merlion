# ADR-0005: Theming through CSS custom properties, labels as `<text>`

- **Status:** Accepted
- **Date:** 2026-09-22

## Context

mermaid writes theme colours into the SVG at render time, so switching between light and dark means re-rendering, or rendering twice and hiding one copy (duplicate text, doubled weight, id collisions). Mermaid's proposal to emit CSS variables (PR #8265, `cssVariableTheme`) is open with unresolved review findings as of 2026-09-22. beautiful-mermaid themes through CSS variables, but only for 6 diagram types. mermaid's default labels are HTML inside `<foreignObject>`, which crawlers and graph extraction handle inconsistently.

## Decision

Every colour, font and stroke in the output is a `var(--merlion-*, fallback)` in the embedded style, over a presentation attribute carrying the default value ([svg-output.md](../svg-output.md#theming)). All labels are SVG `<text>`. Two foundation tokens (`--merlion-bg`, `--merlion-fg`) derive the rest through `color-mix(in oklab, …)`.

## Options considered

| Option | Buys | Costs |
|---|---|---|
| **CSS variables + `<text>`** | Theme switches without re-render; one SVG per diagram; indexable labels; no sanitiser needed | Rich HTML labels are unsupported (only bold, italic and code); every value is written twice (attribute and CSS) |
| Colours fixed at render, one render per theme | Works in any SVG consumer | Duplicate text, double weight, re-render on theme change |
| `<foreignObject>` HTML labels | Arbitrary HTML formatting and wrapping by the browser | Needs a DOM to measure; inconsistent crawling; needs sanitising |

## Criteria

Theme switching cost, crawlability, and identical output with no DOM. All three favour the decision; the cost is rich label formatting.

## Consequences

- Easier: any site themes diagrams with its own tokens; one render serves every theme.
- Harder: SVG renderers outside browsers show the default light theme from the presentation attributes or `var()` fallbacks, never a custom one; browsers without `color-mix` show the literal defaults for derived roles. A standalone SVG has a transparent background unless `background: true`.
- Expensive to reverse: the token names are a public API from the first release.
