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
    let pool = if centred.is_empty() {
        candidates
    } else {
        centred
    };
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

/// Splits layer `l` into `rows` sub-rows (specs/layout.md#5-container-fit, step 3) and
/// returns the model nodes of rows 1.. (row 0 stays). Rows hold consecutive real nodes
/// in order, balanced by width. Each cut lies within a quarter of a row of its
/// balanced position and separates the fewest edges there: an edge counts when its
/// other end (order-axis coordinate `x`) lies strictly on the far side of the cut.
/// Ties go to the balanced position. `None` when the layer has fewer than two real
/// nodes.
pub fn layer_rows(g: &LGraph, l: usize, x: &[f64], rows: usize) -> Option<Vec<Vec<usize>>> {
    let layer = g.layers.get(l)?;
    let real: Vec<(usize, usize)> = layer
        .iter()
        .filter_map(|&v| match g.nodes.get(v)?.kind {
            Kind::Real(m) => Some((v, m)),
            _ => None,
        })
        .collect();
    let r = real.len();
    if r < 2 || rows < 2 || x.len() < g.nodes.len() {
        return None;
    }
    let rows = rows.min(r);
    // Cumulative width before each node.
    let widths: Vec<f64> = real
        .iter()
        .map(|&(v, _)| g.nodes[v].left + g.nodes[v].right)
        .collect();
    let total: f64 = widths.iter().sum();
    let mut before = Vec::with_capacity(r + 1);
    let mut acc = 0.0;
    for w in &widths {
        before.push(acc);
        acc += w;
    }
    let separated = |k: usize| -> usize {
        let cut = (x[real[k - 1].0] + x[real[k].0]) / 2.0;
        let mut cost = 0usize;
        for (i, &(v, _)) in real.iter().enumerate() {
            let left = i < k;
            for &w in g.up[v].iter().chain(&g.down[v]) {
                let xw = x.get(w).copied().unwrap_or(cut);
                if (left && xw > cut) || (!left && xw < cut) {
                    cost += 1;
                }
            }
        }
        cost
    };
    // Two rows follow the spec's rule (fewest separated edges near the middle). With
    // more rows the cuts stay balanced: a node's edges to a parent row above separate
    // at every cut on one side of the parent, which would skew every cut away from it.
    let slack = if rows == 2 { r / rows / 4 } else { 0 };
    let mut cuts: Vec<usize> = Vec::with_capacity(rows - 1);
    let mut prev = 0usize;
    for j in 1..rows {
        // Balanced position: the first node starting at or past j/rows of the width.
        let target = total * j as f64 / rows as f64;
        let ideal = (1..r).find(|&k| before[k] >= target).unwrap_or(r - 1);
        // Leave at least one node for every remaining row.
        let (first, last) = (prev + 1, r - (rows - j));
        let ideal = ideal.clamp(first, last);
        let lo = first.max(ideal.saturating_sub(slack));
        let hi = last.min(ideal + slack);
        let k = (lo..=hi)
            .min_by_key(|&k| (separated(k), k.abs_diff(ideal)))
            .unwrap_or(lo);
        cuts.push(k);
        prev = k;
    }
    cuts.push(r);
    let mut out = Vec::with_capacity(rows - 1);
    for w in cuts.windows(2) {
        out.push(real[w[0]..w[1]].iter().map(|&(_, m)| m).collect());
    }
    Some(out)
}

/// New layers after splitting: a layer split into `k + 1` rows keeps row 0 in place,
/// puts row `i` in the `i`-th pseudo-layer right below it, and every later layer
/// shifts down by the number of pseudo-layers inserted above it. `splits` holds
/// `(layer, rows 1..)` of model nodes.
pub fn apply_splits(layer: &[usize], splits: &[(usize, Vec<Vec<usize>>)]) -> Vec<usize> {
    let mut row = alloc::vec![0usize; layer.len()];
    for (_, rows) in splits {
        for (i, nodes) in rows.iter().enumerate() {
            for &m in nodes {
                if let Some(f) = row.get_mut(m) {
                    *f = i + 1;
                }
            }
        }
    }
    layer
        .iter()
        .enumerate()
        .map(|(m, &l)| {
            let above: usize = splits
                .iter()
                .filter(|(s, _)| *s < l)
                .map(|(_, rows)| rows.len())
                .sum();
            l + above + row[m]
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

    fn fan(k: usize, width: f64) -> (LGraph, Vec<usize>) {
        let cl = Clusters::default();
        let real: Vec<Extent> = (0..=k)
            .map(|_| Extent {
                left: width / 2.0,
                right: width / 2.0,
                thick: 10.0,
            })
            .collect();
        let mut layers = vec![1; k + 1];
        layers[0] = 0;
        let edges: Vec<EdgeIn> = (1..=k)
            .map(|i| EdgeIn {
                edge: i,
                upper: 0,
                lower: i,
                reversed: false,
                label: None,
            })
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
        (g, layers)
    }

    #[test]
    fn two_rows_split_at_the_middle() {
        let (g, layers) = fan(6, 20.0);
        // Children 30 px apart, the root centred above them.
        let mut x = vec![0.0; g.nodes.len()];
        for (i, &v) in g.layers[1].iter().enumerate() {
            x[v] = 30.0 * i as f64;
        }
        x[g.real[0]] = 75.0;
        let rows = layer_rows(&g, 1, &x, 2).unwrap();
        assert_eq!(rows, vec![vec![4, 5, 6]]);
        assert_eq!(layer_rows(&g, 0, &x, 2), None);
        let new = apply_splits(&layers, &[(1, rows)]);
        assert_eq!(new, vec![0, 1, 1, 1, 2, 2, 2]);
        assert_eq!(
            apply_splits(&[0, 1, 2], &[(0, vec![vec![0]])]),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn many_rows_are_balanced_and_shift_later_layers() {
        let (g, _) = fan(10, 20.0);
        let mut x = vec![0.0; g.nodes.len()];
        for (i, &v) in g.layers[1].iter().enumerate() {
            x[v] = 30.0 * i as f64;
        }
        x[g.real[0]] = 135.0;
        let rows = layer_rows(&g, 1, &x, 3).unwrap();
        assert_eq!(rows.len(), 2);
        let sizes: Vec<usize> = rows.iter().map(Vec::len).collect();
        assert!(sizes.iter().all(|&s| (3..=4).contains(&s)), "{:?}", sizes);
        // Order is kept: rows hold consecutive nodes.
        let flat: Vec<usize> = rows.concat();
        assert!(flat.windows(2).all(|w| w[0] < w[1]));
        // Never more rows than nodes.
        assert_eq!(layer_rows(&g, 1, &x, 50).unwrap().len(), 9);
        // Three rows of layer 0 push layer 1 down by two.
        let new = apply_splits(&[0, 0, 0, 1], &[(0, vec![vec![1], vec![2]])]);
        assert_eq!(new, vec![0, 1, 2, 3]);
    }
}
