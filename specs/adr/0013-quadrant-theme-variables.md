# ADR-0013: Quadrant chart theme variables are fixed source colours

- **Status:** Proposed
- **Date:** 2026-11-27

## Context

Merlion ignores `themeVariables` with `I011`, because its themes are CSS custom properties ([ADR-0005](0005-css-variable-theming.md)). A mermaid quadrant chart, unlike the other supported types, has no `classDef` or `style` for its quadrants, axes or title: the fifteen quadrant keys of `themeVariables` (`quadrant1Fill`, `quadrantPointFill`, …) are the only way its source sets those colours, and mermaid's own documentation and test fixtures use them. Ignoring them would drop authored colours that no other statement can express.

## Decision

A quadrant chart reads the quadrant keys of `themeVariables` as source colours: each replaces its element's token with a fixed literal and reports `I030 FixedColour`, as a `style` colour does. Every other `themeVariables` key keeps `I011`. A diagram of another type reads none of them, so a `themeVariables` holding only quadrant keys reports `I011` once the header is known ([quadrant.md](../quadrant.md#theme-variables)).

## Options considered

| Option | Buys | Costs |
|---|---|---|
| **Quadrant keys as fixed source colours** | Authored colours render as in mermaid; the diagnostic matches other source colours | A fixed colour ignores light and dark themes, as a `style` colour does |
| Ignore them with `I011` | One rule for every type | Drops the only colour statements a quadrant chart has |
| Map them onto new `--merlion-quadrant-*` tokens | Themeable | A source value would become a theme default, and the token names a public API |

## Criteria

Fidelity to authored mermaid sources without widening the token API. The decision keeps the token list unchanged and treats the values like every other colour written in a source.

## Consequences

- Easier: mermaid quadrant charts with theme colours render with those colours.
- Harder: such a chart keeps its colours in a dark theme; `I030` reports it.
- Expensive to reverse: little; dropping the keys again is a diagnostic change.
