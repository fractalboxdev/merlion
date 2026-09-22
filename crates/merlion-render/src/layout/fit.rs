//! Phase 5, container fit: choosing where to wrap a horizontal layout and where to
//! split a wide layer (specs/layout.md#5-container-fit).

use alloc::vec::Vec;

use super::lgraph::{Kind, LGraph};

/// Where to split the layer range `a..b` of a left-to-right layout into two rows:
/// the boundary `s` (layers `a..s` stay, `s..b` move below) with the fewest edges
/// crossing it, among boundaries in the middle half of the range's extent (all
/// boundaries when none lies there), ties broken by closeness to the midpoint.
/// `start[l]`/`end[l]` are layer extents along the layer axis, `crossing[s]` counts
/// edges between layers `< s` and `>= s`, and `blocked[s]` marks boundaries a cluster
/// spans.
pub fn wrap_split(
    a: usize,
    b: usize,
    start: &[f64],
    end: &[f64],
    crossing: &[usize],
    blocked: &[bool],
) -> Option<usize> {
    if b <= a + 1 || b > start.len() || b > end.len() {
        return None;
    }
    let lo = start[a];
    let hi = end[b - 1];
    let mid = (lo + hi) / 2.0;
    let quarter = (hi - lo) / 4.0;
    let candidates: Vec<usize> = (a + 1..b)
        .filter(|&s| !blocked.get(s).copied().unwrap_or(true))
        .collect();
    let centred: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(|&s| crate::math::abs(start[s] - mid) <= quarter)
        .collect();
    let pool = if centred.is_empty() { candidates } else { centred };
    pool.into_iter().min_by(|&x, &y| {
        let cx = crossing.get(x).copied().unwrap_or(usize::MAX);
        let cy = crossing.get(y).copied().unwrap_or(usize::MAX);
        cx.cmp(&cy).then(
            crate::math::abs(start[x] - mid)
                .partial_cmp(&crate::math::abs(start[y] - mid))
                .unwrap_or(core::cmp::Ordering::Equal),
        )
    })
}

/// Model nodes of layer `l` that move to a new pseudo-layer below it when the layer is
/// split into two rows: the real nodes from order position `k` on. `k` lies in the
/// middle half of the real nodes and separates the fewest edges: an edge counts when
/// its other end (in the layer above or below, order-axis coordinate `x`) lies strictly
/// on the far side of the cut. Ties go to the middle.
pub fn layer_split(g: &LGraph, l: usize, x: &[f64]) -> Option<Vec<usize>> {
    let layer = g.layers.get(l)?;
    let real: Vec<(usize, usize)> = layer
        .iter()
        .filter_map(|&v| match g.nodes.get(v)?.kind {
            Kind::Real(m) => Some((v, m)),
            _ => None,
        })
        .collect();
    let r = real.len();
    if r < 2 || x.len() < g.nodes.len() {
        return None;
    }
    let (lo, hi) = (r / 4, (3 * r).div_ceil(4));
    let mut best: Option<(usize, usize, usize)> = None;
    for k in lo.max(1)..=hi.min(r - 1) {
        let cut = (x[real[k - 1].0] + x[real[k].0]) / 2.0;
        let mut cost = 0usize;
        for (i, &(v, _)) in real.iter().enumerate() {
            let left = i < k;
            for &w in g.up[v].iter().chain(&g.down[v]) {
                if (left && x[w] > cut) || (!left && x[w] < cut) {
                    cost += 1;
                }
            }
        }
        let dist = (2 * k).abs_diff(r);
        if best.is_none_or(|(c, d, _)| (cost, dist) < (c, d)) {
            best = Some((cost, dist, k));
        }
    }
    let (_, _, k) = best?;
    Some(real[k..].iter().map(|&(_, m)| m).collect())
}

/// New layers after splitting: every split layer inserts one pseudo-layer right below
/// it holding its moved nodes, and every later layer shifts down by the number of
/// pseudo-layers above it. `splits` holds `(layer, moved model nodes)`.
pub fn apply_splits(layer: &[usize], splits: &[(usize, Vec<usize>)]) -> Vec<usize> {
    let mut moved = alloc::vec![false; layer.len()];
    for (_, nodes) in splits {
        for &m in nodes {
            if let Some(f) = moved.get_mut(m) {
                *f = true;
            }
        }
    }
    layer
        .iter()
        .enumerate()
        .map(|(m, &l)| {
            let above = splits.iter().filter(|(s, _)| *s < l).count();
            l + above + usize::from(moved[m])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::lgraph::{build, BuildIn, Clusters, EdgeIn, Extent};
    use super::*;
    use alloc::vec;

    #[test]
    fn wrap_split_prefers_few_crossings_near_the_middle() {
        // Six layers of width 10 with gaps of 10.
        let start: Vec<f64> = (0..6).map(|i| i as f64 * 20.0).collect();
        let end: Vec<f64> = start.iter().map(|s| s + 10.0).collect();
        let crossing = vec![0, 1, 1, 1, 1, 1];
        let none = vec![false; 6];
        assert_eq!(wrap_split(0, 6, &start, &end, &crossing, &none), Some(3));
        let crossing = vec![0, 1, 1, 2, 1, 1];
        let s = wrap_split(0, 6, &start, &end, &crossing, &none).unwrap();
        assert!(s == 2 || s == 4);
        let mut blocked = vec![false; 6];
        blocked[2] = true;
        blocked[3] = true;
        blocked[4] = true;
        let s = wrap_split(0, 6, &start, &end, &crossing, &blocked).unwrap();
        assert!(s == 1 || s == 5);
        assert_eq!(wrap_split(2, 3, &start, &end, &crossing, &none), None);
    }

    #[test]
    fn layer_split_moves_the_right_half() {
        let cl = Clusters::default();
        let real: Vec<Extent> = (0..7).map(|_| Extent { left: 10.0, right: 10.0, thick: 10.0 }).collect();
        let layers = [0, 1, 1, 1, 1, 1, 1];
        let edges: Vec<EdgeIn> = (1..7)
            .map(|i| EdgeIn { edge: i, upper: 0, lower: i, reversed: false, label: None })
            .collect();
        let g = build(&BuildIn {
            real: &real,
            layer: &layers,
            edges: &edges,
            clusters: &cl,
            titles: &[],
            empty_size: &[],
            max_nodes: 100,
            max_layers: 100,
        })
        .unwrap();
        // Children 30 px apart, the root centred above them.
        let mut x = vec![0.0; g.nodes.len()];
        for (i, &v) in g.layers[1].iter().enumerate() {
            x[v] = 30.0 * i as f64;
        }
        x[g.real[0]] = 75.0;
        let moved = layer_split(&g, 1, &x).unwrap();
        assert_eq!(moved, vec![4, 5, 6]);
        assert_eq!(layer_split(&g, 0, &x), None);
        let new = apply_splits(&layers, &[(1, moved)]);
        assert_eq!(new, vec![0, 1, 1, 1, 2, 2, 2]);
        assert_eq!(apply_splits(&[0, 1, 2], &[(0, vec![0])]), vec![1, 2, 3]);
    }
}
