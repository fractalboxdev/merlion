# ADR-0007: A reader-preference panel in the benchmark

- **Status:** Proposed
- **Date:** 2026-09-22

## Context

Van Wageningen, Mchedlidze and Telea (GD 2025) show that a drawing can be morphed into an arbitrary shape while one or more quality metrics stay almost unchanged, so metrics alone can rate a poor drawing as good. Mooney et al. (GD 2025) show that people perceive and prefer lower stress, which makes stress one metric with evidence from people behind it.

## Decision

The benchmark includes a blind pairwise panel of 20+ readers plus a reachability task, reported with 95% confidence intervals ([benchmark.md](../benchmark.md#reader-preference)). A public "better than X" claim needs both a metric improvement and a panel win whose interval excludes zero.

## Options considered

| Option | Buys | Costs |
|---|---|---|
| **Metrics + panel** | A defensible claim; catches metric-gaming | Recruiting and running 20+ readers per release that changes layout |
| Metrics only | Automatic, runs in CI | Can't support a readability claim |
| LLM-as-judge | Cheap, repeatable | No evidence it tracks what human readers find clear for diagrams; useful only as a pre-filter |

## Criteria

Whether a result can support a public claim, against cost per run. Defensibility decided it.

## Consequences

- Easier: marketing and documentation claims rest on evidence.
- Harder: each layout-changing release needs a panel run. TODO(owner): who recruits readers and how often the panel runs.
- Expensive to reverse: nothing.
