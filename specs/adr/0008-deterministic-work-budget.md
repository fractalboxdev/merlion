# ADR-0008: Layout work is bounded by fuel, not by time

- **Status:** Proposed
- **Date:** 2026-09-22

## Context

Layout has optional work (exact crossing refinement, the local objective, sleeve routing) whose cost grows quickly with graph size, and mandatory phases (network simplex, crossing sweeps, coordinate assignment) whose cost is polynomial but large on adversarial input. Output must be byte-identical on every target for the same source, options and hint ([architecture.md](../architecture.md#determinism)). The core is `#![no_std]` and has no clock. The browser build renders untrusted source from LLMs and chat users, so the worst-case render time is a denial-of-service boundary.

## Decision

Every phase draws from a single per-diagram fuel counter. One unit of fuel is one step of the phase's inner loop: one crossing comparison in a sweep, one pivot in network simplex, one search node expanded in exact refinement, one triangle visited in sleeve routing. The `fuel` render option sets the limit, and the default is 20,000,000 units. TODO(owner): calibrate the default so that the M0 reference machine spends about 50 ms on the heaviest `docs` diagram, and record the machine and the measurement in [benchmark.md](../benchmark.md).

- When fuel runs out during an optional pass, the pass stops and the best result found so far is kept; sleeve routing falls back to `polyline` for the remaining edges ([layout.md](../layout.md#6-edge-routing)).
- When fuel runs out during a mandatory phase, the render returns `TooLarge`.
- Each optional pass may use only the fuel left after the mandatory phases' reserve, which the core computes from the layered graph's size before phase 3 starts, so an optional pass never starves a mandatory one.

## Options considered

| Option | Buys | Costs |
|---|---|---|
| **Fuel counter** | Deterministic output; works in `no_std`; the worst-case time is a function of input and options only | Fuel-to-milliseconds varies by machine, so the default only approximates a time target; every inner loop has to decrement the counter |
| Wall-clock budget (50 ms) | Direct control of latency | Output depends on machine load, which breaks byte-identical output and committed-SVG diffs; needs a clock injected into a `no_std` core |
| No budget, only size limits | Simplest | Worst-case time on adversarial input within the size limits is unbounded in practice; exact refinement can't be cut off |

## Criteria

Determinism, bounded worst-case time for untrusted input, and `no_std` compatibility. Determinism decided it: a wall-clock budget makes the same commit render different SVGs on a loaded CI runner and a laptop.

## Consequences

- Easier: tests pin output without pinning hardware; a denial-of-service bound can be stated per option set.
- Harder: fuel accounting must be added to each inner loop and kept honest. Changing what a unit costs changes output for diagrams that hit the limit, so the fuel model is part of the layout's compatibility contract.
- Expensive to reverse: the `fuel` option name and its default, once 1.0 ships.
- Uncertainty: the ratio of fuel to wall time is unmeasured until M0; the default is provisional.
