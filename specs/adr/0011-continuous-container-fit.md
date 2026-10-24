# ADR-0011: Continuous flow before container wrapping

- **Status:** Proposed
- **Date:** 2026-10-25

## Context

Folding a horizontal graph into rows routes relationships around the outside and back into subsequent rows. A narrow drawing can therefore have long return paths even with few edge crossings. Orientation choice and container fit share a readability constraint.

[Ware et al. (2002)](https://vislab-ccom.unh.edu/pdfs/GraphAesthetics.pdf) find that path continuity and crossings affect shortest-path performance in spring-layout graphs. Their results motivate preserving continuity; they do not establish a universal geometric edge-length threshold for flowcharts. [ARCOL (2026)](https://arxiv.org/abs/2603.29618) integrates aspect ratio with orthogonal layout quality as a soft constraint. Merlion uses the trade-off, not ARCOL's HOLA-based algorithm.

## Decision

Direction defaults to `auto`. A fitting source or surviving hint direction stays. When it exceeds the container width, the other unwrapped orientation is tried before wrapping or splitting. Disconnected components follow the same policy at the packed-diagram level. `source` remains an explicit strict-direction option across Rust, CLI and WASM.

Rendered collisions between adjacent-layer label chips receive separate slots and additional gap space when needed. Each candidate permits one repair bounded by its first routing pass's fuel.

## Options considered and criteria

Keeping source direction by default preserves strict orientation but folds graphs whose other orientation fits. Always choosing vertical would replace one orientation preference with another. A geometric detour-length cutoff requires an unvalidated numerical threshold. Automatic orientation is selected for forward flow, container fit, deterministic output and an explicit compatibility override.

## Consequences

Diagrams can change orientation when their source direction exceeds the width. Strict consumers set `source`. Neither orientation guarantees a fit: bounded wrapping, splitting, label reflow and viewer zoom remain available. These rules have regression coverage, not a Merlion-specific human performance study.
