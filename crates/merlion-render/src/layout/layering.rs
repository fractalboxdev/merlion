//! Phase 2, layer assignment (specs/layout.md#2-layer-assignment).
//!
//! Flowcharts use longest-path layering from the virtual root: every node sits as high
//! as its predecessors allow, one `min_len` below the lowest of them, and therefore
//! below all its dominators (VEIL, Schaad, Ben-Nun, Hoefler 2025).

use alloc::vec;
use alloc::vec::Vec;

use crate::fuel::{Fuel, OutOfFuel};

/// Layers for `n` nodes over acyclic `(upper, lower, min_len)` edges; nodes without
/// predecessors sit at layer 0. Kahn's topological order, processed in index order so
/// the result is deterministic. Edges with an endpoint `>= n` are ignored; lengths
/// saturate instead of overflowing. Should the edges contain a cycle, the nodes on it
/// keep the layer of their processed predecessors (the caller guarantees acyclicity).
pub fn longest_path(
    n: usize,
    edges: &[(usize, usize, usize)],
    fuel: &mut Fuel,
) -> Result<Vec<usize>, OutOfFuel> {
    let mut out: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for &(u, v, len) in edges {
        if u < n && v < n && u != v {
            out[u].push((v, len.max(1)));
            indeg[v] += 1;
        }
    }
    let mut layer = vec![0usize; n];
    let mut ready: alloc::collections::BTreeSet<usize> =
        (0..n).filter(|&v| indeg[v] == 0).collect();
    while let Some(u) = ready.pop_first() {
        fuel.burn(1)?;
        for &(v, len) in &out[u] {
            fuel.burn(1)?;
            let cand = layer[u].saturating_add(len);
            if cand > layer[v] {
                layer[v] = cand;
            }
            indeg[v] -= 1;
            if indeg[v] == 0 {
                ready.insert(v);
            }
        }
    }
    Ok(layer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(n: usize, e: &[(usize, usize, usize)]) -> Vec<usize> {
        longest_path(n, e, &mut Fuel::new(100_000)).unwrap()
    }

    #[test]
    fn chain_gets_consecutive_layers() {
        assert_eq!(run(3, &[(0, 1, 1), (1, 2, 1)]), [0, 1, 2]);
    }

    #[test]
    fn node_sits_below_all_predecessors() {
        // 0 -> 1 -> 2 -> 3 and 0 -> 3: 3 sits at layer 3.
        assert_eq!(
            run(4, &[(0, 1, 1), (1, 2, 1), (2, 3, 1), (0, 3, 1)]),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn min_len_is_respected() {
        let l = run(3, &[(0, 1, 3), (1, 2, 1), (0, 2, 1)]);
        assert_eq!(l, [0, 3, 4]);
        assert!(l[1] - l[0] >= 3);
    }

    #[test]
    fn sources_start_at_layer_zero() {
        assert_eq!(run(3, &[(1, 2, 1)]), [0, 0, 1]);
    }

    #[test]
    fn every_edge_points_down_on_random_dags() {
        let mut s: u64 = 5;
        let mut next = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 33) as usize
        };
        for _ in 0..100 {
            let n = 2 + next() % 40;
            let e: Vec<(usize, usize, usize)> = (0..next() % 100)
                .map(|_| {
                    let a = next() % n;
                    let b = next() % n;
                    (a.min(b), a.max(b), 1 + next() % 3)
                })
                .filter(|&(a, b, _)| a != b)
                .collect();
            let l = run(n, &e);
            for &(a, b, len) in &e {
                assert!(l[b] >= l[a] + len);
            }
        }
    }

    #[test]
    fn fuel_exhaustion_is_reported() {
        let e: Vec<(usize, usize, usize)> = (0..50).map(|i| (i, i + 1, 1)).collect();
        assert!(longest_path(51, &e, &mut Fuel::new(5)).is_err());
    }
}
