//! Phase 4, coordinate assignment (specs/layout.md#4-coordinate-assignment).
//!
//! Order-axis coordinates come from Brandes–Köpf (*Fast and Simple Horizontal
//! Coordinate Assignment*, GD 2001): four vertical alignments (upper/lower medians,
//! left/right priority), each compacted, then balanced by aligning them to the
//! narrowest and taking the average of the two median candidates per node.
//!
//! Compaction places every block (a vertically aligned set of nodes) by a longest-path
//! pass over the block constraint graph, which gives the leftmost placement that keeps
//! the separation between neighbours in every layer. Each of the four layouts keeps the
//! separation, and so does the balanced one: for neighbours `u` left of `v`, every
//! candidate of `v` exceeds the matching candidate of `u` by the separation, so the
//! same holds for the second and third smallest candidates.
//!
//! When clusters exist, a second pass ([`fit_clusters`]) turns every cluster into a
//! solid block: one left and one right boundary variable per cluster, shared by all
//! layers it spans, with non-members kept outside and members inside with padding.
//! Nodes only move right of their Brandes–Köpf position, just enough to satisfy these
//! constraints; a vertical run of one-to-one segments moves as one piece unless that
//! makes the drawing wider.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use super::lgraph::{Clusters, LGraph};
use crate::fuel::{Fuel, OutOfFuel};
use crate::math::{max, min};

/// Gap between a cluster box that ends in one layer and whatever starts in the next.
pub const CLUSTER_LAYER_GAP: f64 = 8.0;

/// How much wider than the untied solve a cluster solve with straight runs tied may be.
const TIE_SLACK: f64 = 0.0;

/// Padding of a cluster box around its members, in the layout frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pad {
    pub order_before: f64,
    pub order_after: f64,
    pub layer_before: f64,
    pub layer_after: f64,
}

/// Axis-aligned rectangle in the layout frame: `x` along the order axis, `y` along the
/// layer axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// Minimum distance between the centres of neighbours `u` (left) and `v` in a layer:
/// their extents plus `spacing`, halved next to a dummy (edges pack more tightly).
pub fn separation(g: &LGraph, u: usize, v: usize, spacing: f64) -> f64 {
    let (a, b) = (&g.nodes[u], &g.nodes[v]);
    let gap = if a.is_dummy() || b.is_dummy() {
        spacing / 2.0
    } else {
        spacing
    };
    a.right + gap + b.left
}

/// One Brandes–Köpf layout in the canonical orientation (align with upper neighbours,
/// left priority) over `layers`, where `up` gives the upper neighbours.
fn bk_one(
    layers: &[Vec<usize>],
    up: &[Vec<usize>],
    dummy: &[bool],
    sep: &dyn Fn(usize, usize) -> f64,
    n: usize,
    fuel: &mut Fuel,
) -> Result<Vec<f64>, OutOfFuel> {
    let mut pos = vec![0usize; n];
    for layer in layers {
        for (i, &v) in layer.iter().enumerate() {
            pos[v] = i;
        }
    }
    // Type 1 conflicts: a non-inner segment crossing an inner segment (both ends dummy)
    // is marked, so alignment prefers straight long edges (BK Alg. 1).
    let mut marked: alloc::collections::BTreeSet<(usize, usize)> = Default::default();
    for i in 0..layers.len().saturating_sub(1) {
        let (upper, lower) = (&layers[i], &layers[i + 1]);
        if upper.is_empty() {
            continue;
        }
        let mut k0 = 0usize;
        let mut l = 0usize;
        for (l1, &v) in lower.iter().enumerate() {
            fuel.burn(1)?;
            let inner = if dummy[v] {
                up[v].iter().copied().find(|&u| dummy[u])
            } else {
                None
            };
            if l1 + 1 == lower.len() || inner.is_some() {
                let k1 = inner.map_or(upper.len() - 1, |u| pos[u]);
                while l <= l1 {
                    let w = lower[l];
                    for &u in &up[w] {
                        fuel.burn(1)?;
                        if (pos[u] < k0 || pos[u] > k1) && !(dummy[u] && dummy[w]) {
                            marked.insert((u, w));
                        }
                    }
                    l += 1;
                }
                k0 = k1;
            }
        }
    }
    // Vertical alignment with the median upper neighbours (BK Alg. 2).
    let mut root: Vec<usize> = (0..n).collect();
    let mut align: Vec<usize> = (0..n).collect();
    let mut ups: Vec<usize> = Vec::new();
    for layer in layers {
        let mut r: Option<usize> = None;
        for &v in layer {
            ups.clear();
            ups.extend(up[v].iter().copied());
            ups.sort_by_key(|&u| pos[u]);
            fuel.burn(ups.len() as u64 + 1)?;
            let d = ups.len();
            if d == 0 {
                continue;
            }
            for m in [(d - 1) / 2, d / 2] {
                if align[v] != v {
                    break;
                }
                let u = ups[m];
                if !marked.contains(&(u, v)) && r.is_none_or(|r| r < pos[u]) {
                    align[u] = v;
                    root[v] = root[u];
                    align[v] = root[v];
                    r = Some(pos[u]);
                }
            }
        }
    }
    // Compaction: longest path over the block graph (block = alignment root).
    let mut succ: BTreeMap<usize, Vec<(usize, f64)>> = BTreeMap::new();
    let mut indeg = vec![0usize; n];
    for layer in layers {
        for w in layer.windows(2) {
            fuel.burn(1)?;
            let (a, b) = (root[w[0]], root[w[1]]);
            succ.entry(a).or_default().push((b, sep(w[0], w[1])));
            indeg[b] += 1;
        }
    }
    let mut x = vec![0.0f64; n];
    let mut stack: Vec<usize> = (0..n).filter(|&v| root[v] == v && indeg[v] == 0).collect();
    stack.reverse();
    let mut done = 0usize;
    while let Some(b) = stack.pop() {
        done += 1;
        if let Some(list) = succ.get(&b) {
            for &(c, w) in list {
                fuel.burn(1)?;
                x[c] = max(x[c], x[b] + w);
                indeg[c] -= 1;
                if indeg[c] == 0 {
                    stack.push(c);
                }
            }
        }
    }
    let blocks = (0..n).filter(|&v| root[v] == v).count();
    if done < blocks {
        // Alignments never cross, so the block graph is acyclic; should that ever fail,
        // fall back to packing each layer on its own.
        for layer in layers {
            let mut cur = 0.0;
            for (i, &v) in layer.iter().enumerate() {
                if i > 0 {
                    cur += sep(layer[i - 1], v);
                }
                x[v] = cur;
            }
        }
        return Ok(x);
    }
    let block_x = x.clone();
    for v in 0..n {
        x[v] = block_x[root[v]];
    }
    Ok(x)
}

/// Brandes–Köpf order-axis coordinates, balanced over the four alignments, shifted so
/// the leftmost node edge is at 0.
pub fn assign_x(g: &LGraph, spacing: f64, fuel: &mut Fuel) -> Result<Vec<f64>, OutOfFuel> {
    let n = g.nodes.len();
    let dummy: Vec<bool> = g.nodes.iter().map(|v| v.is_dummy()).collect();
    let sep = |u: usize, v: usize| separation(g, u, v, spacing);
    let sep_mirror = |u: usize, v: usize| separation(g, v, u, spacing);
    let mirrored: Vec<Vec<usize>> = g
        .layers
        .iter()
        .map(|l| l.iter().rev().copied().collect())
        .collect();
    let bottom_up: Vec<Vec<usize>> = g.layers.iter().rev().cloned().collect();
    let bottom_up_mirrored: Vec<Vec<usize>> = mirrored.iter().rev().cloned().collect();
    let mut runs: Vec<(Vec<f64>, bool)> = vec![
        (bk_one(&g.layers, &g.up, &dummy, &sep, n, fuel)?, false),
        (
            bk_one(&mirrored, &g.up, &dummy, &sep_mirror, n, fuel)?,
            true,
        ),
        (bk_one(&bottom_up, &g.down, &dummy, &sep, n, fuel)?, false),
        (
            bk_one(&bottom_up_mirrored, &g.down, &dummy, &sep_mirror, n, fuel)?,
            true,
        ),
    ];
    // Right-priority layouts run mirrored; negate them back.
    for (xs, right) in runs.iter_mut() {
        if *right {
            for x in xs.iter_mut() {
                *x = -*x;
            }
        }
    }
    let extent = |xs: &[f64]| {
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for (v, node) in g.nodes.iter().enumerate() {
            lo = min(lo, xs[v] - node.left);
            hi = max(hi, xs[v] + node.right);
        }
        (lo, hi)
    };
    if n == 0 {
        return Ok(Vec::new());
    }
    let ext: Vec<(f64, f64)> = runs.iter().map(|(xs, _)| extent(xs)).collect();
    let narrowest = (0..4)
        .min_by(|&a, &b| {
            (ext[a].1 - ext[a].0)
                .partial_cmp(&(ext[b].1 - ext[b].0))
                .unwrap_or(core::cmp::Ordering::Equal)
        })
        .unwrap_or(0);
    let (ref_lo, ref_hi) = ext[narrowest];
    for (k, (xs, right)) in runs.iter_mut().enumerate() {
        let shift = if *right {
            ref_hi - ext[k].1
        } else {
            ref_lo - ext[k].0
        };
        for x in xs.iter_mut() {
            *x += shift;
        }
    }
    let mut x = vec![0.0; n];
    for (v, out) in x.iter_mut().enumerate() {
        fuel.burn(1)?;
        let mut c = [runs[0].0[v], runs[1].0[v], runs[2].0[v], runs[3].0[v]];
        c.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        *out = (c[1] + c[2]) / 2.0;
    }
    let (lo, _) = extent(&x);
    for v in x.iter_mut() {
        *v -= lo;
    }
    Ok(x)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Node(usize),
    Open(usize),
    Close(usize),
}

/// Makes every cluster a solid block in the order axis (see the module docs). The
/// constraint graph is acyclic because sibling clusters keep one order in all layers
/// (phase 3); nodes move right of their current position only as far as needed.
///
/// A straight run of one-to-one segments (upper node with one lower neighbour, lower
/// node with one upper neighbour, same cluster, same position) moves as one piece, so a
/// chain that Brandes–Köpf drew vertical stays vertical when one of its nodes is pushed.
/// Should tying the runs close a cycle, every node moves on its own instead.
pub fn fit_clusters(
    g: &LGraph,
    cl: &Clusters,
    pads: &[Pad],
    spacing: f64,
    x: &mut [f64],
    fuel: &mut Fuel,
) -> Result<(), OutOfFuel> {
    let n = g.nodes.len();
    if cl.len() == 0 || x.len() != n {
        return Ok(());
    }
    let mut rep: Vec<usize> = (0..n).collect();
    for layer in &g.layers {
        for &u in layer {
            fuel.burn(1)?;
            let [v] = g.down[u][..] else { continue };
            if g.up.get(v).is_some_and(|up| up.len() == 1)
                && g.nodes[u].cluster == g.nodes[v].cluster
                && x[u] == x[v]
            {
                rep[v] = rep[u];
            }
        }
    }
    let own: Vec<usize> = (0..n).collect();
    let free = solve_clusters(g, cl, pads, spacing, x, &own, fuel)?;
    let tied = if rep == own {
        None
    } else {
        solve_clusters(g, cl, pads, spacing, x, &rep, fuel)?
    };
    let width = |xs: &[f64]| {
        let (lo, hi) = g
            .nodes
            .iter()
            .enumerate()
            .fold((f64::MAX, f64::MIN), |(lo, hi), (v, node)| {
                (min(lo, xs[v] - node.left), max(hi, xs[v] + node.right))
            });
        hi - lo
    };
    let solved = match (tied, free) {
        (Some(t), Some(f)) if width(&t) <= width(&f) + TIE_SLACK => Some(t),
        (_, f) => f,
    };
    if let Some(val) = solved {
        x.copy_from_slice(&val);
    }
    Ok(())
}

/// Longest-path solve of the cluster constraints with node `v` placed by variable
/// `rep[v]`; `None` when the constraints form a cycle.
fn solve_clusters(
    g: &LGraph,
    cl: &Clusters,
    pads: &[Pad],
    spacing: f64,
    x: &[f64],
    rep: &[usize],
    fuel: &mut Fuel,
) -> Result<Option<Vec<f64>>, OutOfFuel> {
    let k = cl.len();
    let n = g.nodes.len();
    let pad = |c: usize| pads.get(c).copied().unwrap_or_default();
    let var = |it: Item| match it {
        Item::Node(v) => rep[v],
        Item::Open(c) => n + 2 * c,
        Item::Close(c) => n + 2 * c + 1,
    };
    let total = n + 2 * k;
    let mut succ: Vec<Vec<(usize, f64)>> = vec![Vec::new(); total];
    let mut indeg = vec![0usize; total];
    let chains: Vec<Vec<usize>> = g.nodes.iter().map(|v| cl.chain(v.cluster)).collect();
    let mut items: Vec<Item> = Vec::new();
    for layer in &g.layers {
        items.clear();
        let mut open: Vec<usize> = Vec::new();
        for &v in layer {
            fuel.burn(1)?;
            let ch = &chains[v];
            let common = open.iter().zip(ch).take_while(|(a, b)| a == b).count();
            while open.len() > common {
                if let Some(c) = open.pop() {
                    items.push(Item::Close(c));
                }
            }
            for &c in &ch[common..] {
                open.push(c);
                items.push(Item::Open(c));
            }
            items.push(Item::Node(v));
        }
        while let Some(c) = open.pop() {
            items.push(Item::Close(c));
        }
        for w in items.windows(2) {
            let (a, b) = (w[0], w[1]);
            let gap = match (a, b) {
                (Item::Node(u), Item::Node(v)) => separation(g, u, v, spacing),
                (Item::Open(c), Item::Node(v)) => pad(c).order_before + g.nodes[v].left,
                (Item::Open(c), Item::Open(_)) => pad(c).order_before,
                (Item::Node(u), Item::Close(c)) => g.nodes[u].right + pad(c).order_after,
                (Item::Close(_), Item::Close(c)) => pad(c).order_after,
                (Item::Close(_), Item::Node(v)) => spacing + g.nodes[v].left,
                (Item::Close(_), Item::Open(_)) => spacing,
                (Item::Node(u), Item::Open(_)) => g.nodes[u].right + spacing,
                (Item::Open(_), Item::Close(_)) => 0.0,
            };
            let (va, vb) = (var(a), var(b));
            succ[va].push((vb, gap));
            indeg[vb] += 1;
        }
    }
    let mut val: Vec<f64> = (0..total)
        .map(|i| if i < n { x[i] } else { f64::MIN })
        .collect();
    let mut stack: Vec<usize> = (0..total).filter(|&i| indeg[i] == 0).collect();
    stack.reverse();
    let mut done = 0usize;
    while let Some(a) = stack.pop() {
        done += 1;
        if val[a] == f64::MIN {
            val[a] = 0.0;
        }
        for &(b, w) in &succ[a] {
            fuel.burn(1)?;
            val[b] = max(val[b], val[a] + w);
            indeg[b] -= 1;
            if indeg[b] == 0 {
                stack.push(b);
            }
        }
    }
    if done < total {
        return Ok(None);
    }
    let mut out: Vec<f64> = (0..n).map(|v| val[rep[v]]).collect();
    let lo = g
        .nodes
        .iter()
        .enumerate()
        .map(|(v, node)| out[v] - node.left)
        .fold(f64::MAX, min);
    let lo = (0..k)
        .map(|c| val[n + 2 * c])
        .filter(|v| *v > f64::MIN)
        .fold(lo, min);
    if lo.is_finite() {
        for v in out.iter_mut() {
            *v -= lo;
        }
    }
    Ok(Some(out))
}

/// Clusters' first and last layer.
fn spans(g: &LGraph, cl: &Clusters) -> Vec<Option<(usize, usize)>> {
    let mut out: Vec<Option<(usize, usize)>> = vec![None; cl.len()];
    for node in &g.nodes {
        for c in cl.chain(node.cluster) {
            out[c] = Some(match out[c] {
                None => (node.layer, node.layer),
                Some((lo, hi)) => (lo.min(node.layer), hi.max(node.layer)),
            });
        }
    }
    out
}

/// Clusters sorted deepest first (children before parents).
fn deepest_first(cl: &Clusters) -> Vec<usize> {
    let mut v: Vec<usize> = (0..cl.len()).collect();
    v.sort_by_key(|&c| (core::cmp::Reverse(cl.depth[c]), c));
    v
}

/// Layer-axis centre and thickness of every layer. Neighbouring layers are `gap` apart,
/// more where nested cluster boxes start or end between them (their padding and titles
/// stack up, plus [`CLUSTER_LAYER_GAP`]), and at least `min_gap[l]` between layer `l`
/// and `l + 1` (room for edge labels in that gap).
pub fn layer_y(
    g: &LGraph,
    cl: &Clusters,
    pads: &[Pad],
    gap: f64,
    min_gap: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    let nl = g.layers.len();
    let mut thick = vec![0.0f64; nl];
    for node in &g.nodes {
        if let Some(t) = thick.get_mut(node.layer) {
            *t = max(*t, node.thick);
        }
    }
    let span = spans(g, cl);
    let pad = |c: usize| pads.get(c).copied().unwrap_or_default();
    // Nested padding above the first layer (t) and below the last layer (b) of each cluster.
    let mut t = vec![0.0f64; cl.len()];
    let mut b = vec![0.0f64; cl.len()];
    for c in deepest_first(cl) {
        t[c] += pad(c).layer_before;
        b[c] += pad(c).layer_after;
        if let (Some(p), Some((lo, hi))) = (cl.parent[c], span[c]) {
            if let Some((plo, phi)) = span[p] {
                if plo == lo {
                    t[p] = max(t[p], t[c]);
                }
                if phi == hi {
                    b[p] = max(b[p], b[c]);
                }
            }
        }
    }
    // t and b above include the child's own padding before the parent's is added,
    // because children are processed first and the parent adds its own afterwards.
    let mut top_need = vec![0.0f64; nl];
    let mut bottom_need = vec![0.0f64; nl];
    for c in 0..cl.len() {
        if let Some((lo, hi)) = span[c] {
            top_need[lo] = max(top_need[lo], t[c]);
            bottom_need[hi] = max(bottom_need[hi], b[c]);
        }
    }
    let mut y = vec![0.0f64; nl];
    let top0 = top_need.first().copied().unwrap_or(0.0);
    let mut cur = top0 + thick.first().copied().unwrap_or(0.0) / 2.0;
    for l in 0..nl {
        if l > 0 {
            let clusters_need = if bottom_need[l - 1] + top_need[l] > 0.0 {
                bottom_need[l - 1] + top_need[l] + CLUSTER_LAYER_GAP
            } else {
                0.0
            };
            let label_need = min_gap.get(l - 1).copied().unwrap_or(0.0);
            cur += thick[l - 1] / 2.0 + max(max(gap, label_need), clusters_need) + thick[l] / 2.0;
        }
        y[l] = cur;
    }
    (y, thick)
}

/// Box of every cluster: its members (nodes and nested boxes) plus padding. Clusters
/// with no layered node get `None` (the caller places empty clusters).
pub fn cluster_boxes(
    g: &LGraph,
    cl: &Clusters,
    pads: &[Pad],
    x: &[f64],
    y: &[f64],
) -> Vec<Option<Rect>> {
    let k = cl.len();
    let mut own: Vec<Option<Rect>> = vec![None; k];
    let grow = |r: &mut Option<Rect>, add: Rect| {
        *r = Some(match *r {
            None => add,
            Some(o) => Rect {
                x0: min(o.x0, add.x0),
                y0: min(o.y0, add.y0),
                x1: max(o.x1, add.x1),
                y1: max(o.y1, add.y1),
            },
        });
    };
    for (v, node) in g.nodes.iter().enumerate() {
        let Some(c) = node.cluster else { continue };
        let (Some(&xv), Some(&yl)) = (x.get(v), y.get(node.layer)) else {
            continue;
        };
        if c < k {
            grow(
                &mut own[c],
                Rect {
                    x0: xv - node.left,
                    y0: yl - node.thick / 2.0,
                    x1: xv + node.right,
                    y1: yl + node.thick / 2.0,
                },
            );
        }
    }
    let pad = |c: usize| pads.get(c).copied().unwrap_or_default();
    let mut boxes: Vec<Option<Rect>> = vec![None; k];
    for c in deepest_first(cl) {
        if let Some(r) = own[c] {
            let p = pad(c);
            let bx = Rect {
                x0: r.x0 - p.order_before,
                y0: r.y0 - p.layer_before,
                x1: r.x1 + p.order_after,
                y1: r.y1 + p.layer_after,
            };
            boxes[c] = Some(bx);
            if let Some(parent) = cl.parent[c] {
                grow(&mut own[parent], bx);
            }
        }
    }
    boxes
}

#[cfg(test)]
mod tests {
    use super::super::lgraph::{build, BuildIn, EdgeIn, Extent, Kind};
    use super::super::order;
    use super::*;
    use crate::model::{Flowchart, Node, Shape, Style, Subgraph};
    use alloc::string::String;
    use alloc::vec;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) as usize
        }
    }

    fn clusters(parents: &[Option<usize>], node_sub: &[Option<usize>]) -> Clusters {
        let mut c = Flowchart::default();
        for (i, p) in parents.iter().enumerate() {
            c.subgraphs.push(Subgraph {
                id: alloc::format!("s{}", i),
                title: String::new(),
                parent: *p,
                nodes: vec![],
                direction: None,
                span: Default::default(),
            });
        }
        for (i, s) in node_sub.iter().enumerate() {
            c.nodes.push(Node {
                id: alloc::format!("n{}", i),
                label: String::new(),
                shape: Shape::Rect,
                classes: vec![],
                style: Style::default(),
                link: None,
                subgraph: *s,
                span: Default::default(),
            });
        }
        Clusters::from_chart(&c)
    }

    fn graph(
        widths: &[f64],
        layers: &[usize],
        edges: &[(usize, usize)],
        cl: &Clusters,
        title: f64,
    ) -> LGraph {
        let real: Vec<Extent> = widths
            .iter()
            .map(|&w| Extent {
                left: w / 2.0,
                right: w / 2.0,
                thick: 20.0,
            })
            .collect();
        let e: Vec<EdgeIn> = edges
            .iter()
            .enumerate()
            .map(|(i, &(u, v))| EdgeIn {
                edge: i,
                upper: u,
                lower: v,
                reversed: false,
                label: None,
            })
            .collect();
        let titles = vec![
            Extent {
                left: title / 2.0,
                right: title / 2.0,
                thick: 0.0
            };
            cl.len()
        ];
        let mut g = build(&BuildIn {
            real: &real,
            layer: layers,
            edges: &e,
            clusters: cl,
            titles: &titles,
            empty_size: &titles,
            max_nodes: 100_000,
            max_layers: 1000,
        })
        .unwrap();
        let keys: Vec<f64> = (0..g.nodes.len()).map(|i| i as f64).collect();
        order::initial_order(&mut g, cl, &keys, None);
        order::minimise(&mut g, cl, None, &mut Fuel::new(10_000_000)).unwrap();
        g
    }

    fn fuel() -> Fuel {
        Fuel::new(10_000_000)
    }

    fn assert_separated(g: &LGraph, x: &[f64], spacing: f64) {
        for layer in &g.layers {
            for w in layer.windows(2) {
                let need = separation(g, w[0], w[1], spacing);
                assert!(
                    x[w[1]] - x[w[0]] >= need - 1e-9,
                    "{} vs {}",
                    x[w[1]] - x[w[0]],
                    need
                );
            }
        }
    }

    #[test]
    fn separation_combines_extents_and_spacing() {
        let cl = Clusters::default();
        let g = graph(&[40.0, 60.0], &[0, 0], &[], &cl, 0.0);
        assert_eq!(separation(&g, 0, 1, 24.0), 20.0 + 24.0 + 30.0);
    }

    #[test]
    fn straight_chain_is_vertical() {
        let cl = Clusters::default();
        let g = graph(&[40.0, 80.0, 30.0], &[0, 1, 2], &[(0, 1), (1, 2)], &cl, 0.0);
        let x = assign_x(&g, 24.0, &mut fuel()).unwrap();
        assert_eq!(x[0], x[1]);
        assert_eq!(x[1], x[2]);
    }

    #[test]
    fn parent_sits_between_its_children() {
        let cl = Clusters::default();
        let g = graph(&[40.0, 40.0, 40.0], &[0, 1, 1], &[(0, 1), (0, 2)], &cl, 0.0);
        let x = assign_x(&g, 24.0, &mut fuel()).unwrap();
        let (a, b) = (x[1].min(x[2]), x[1].max(x[2]));
        assert!(a <= x[0] && x[0] <= b);
        assert_separated(&g, &x, 24.0);
    }

    #[test]
    fn long_edge_dummies_line_up() {
        let cl = Clusters::default();
        // 0 -> 3 spans three layers next to a busy chain 1 -> 2 -> 4.
        let g = graph(
            &[40.0; 5],
            &[0, 0, 1, 3, 2],
            &[(0, 3), (1, 2), (2, 4)],
            &cl,
            0.0,
        );
        let x = assign_x(&g, 24.0, &mut fuel()).unwrap();
        let ch = &g.chains[0];
        assert_eq!(x[ch.nodes[1]], x[ch.nodes[2]]);
    }

    #[test]
    fn random_graphs_keep_node_spacing() {
        let mut r = Lcg(8);
        let cl = Clusters::default();
        for _ in 0..30 {
            let n = 5 + r.next() % 30;
            let widths: Vec<f64> = (0..n).map(|_| 20.0 + (r.next() % 100) as f64).collect();
            let layers: Vec<usize> = (0..n).map(|_| r.next() % 5).collect();
            let edges: Vec<(usize, usize)> = (0..n * 2)
                .map(|_| (r.next() % n, r.next() % n))
                .filter(|&(a, b)| layers[a] < layers[b])
                .collect();
            let g = graph(&widths, &layers, &edges, &cl, 0.0);
            let x = assign_x(&g, 24.0, &mut fuel()).unwrap();
            assert_separated(&g, &x, 24.0);
            let min = g
                .nodes
                .iter()
                .enumerate()
                .map(|(i, v)| x[i] - v.left)
                .fold(f64::MAX, f64::min);
            assert!(min.abs() < 1e-9);
        }
    }

    fn pads(k: usize) -> Vec<Pad> {
        vec![
            Pad {
                order_before: 12.0,
                order_after: 12.0,
                layer_before: 30.0,
                layer_after: 12.0
            };
            k
        ]
    }

    #[test]
    fn cluster_boxes_enclose_members_and_exclude_others() {
        let mut r = Lcg(31);
        for _ in 0..30 {
            let n = 6 + r.next() % 16;
            let parents = [None, Some(0), None];
            let subs: Vec<Option<usize>> = (0..n)
                .map(|_| match r.next() % 4 {
                    0 => None,
                    1 => Some(0),
                    2 => Some(1),
                    _ => Some(2),
                })
                .collect();
            let cl = clusters(&parents, &subs);
            let widths: Vec<f64> = (0..n).map(|_| 20.0 + (r.next() % 80) as f64).collect();
            let layers: Vec<usize> = (0..n).map(|_| r.next() % 4).collect();
            let edges: Vec<(usize, usize)> = (0..n * 2)
                .map(|_| (r.next() % n, r.next() % n))
                .filter(|&(a, b)| layers[a] < layers[b])
                .collect();
            let g = graph(&widths, &layers, &edges, &cl, 50.0);
            let p = pads(cl.len());
            let mut x = assign_x(&g, 24.0, &mut fuel()).unwrap();
            fit_clusters(&g, &cl, &p, 24.0, &mut x, &mut fuel()).unwrap();
            assert_separated(&g, &x, 24.0);
            let (y, thick) = layer_y(&g, &cl, &p, 48.0, &[]);
            let boxes = cluster_boxes(&g, &cl, &p, &x, &y);
            for (c, bx) in boxes.iter().enumerate() {
                let Some(bx) = bx else { continue };
                assert!(bx.x1 - bx.x0 >= 50.0 + 24.0 - 1e-9, "title width");
                for (v, node) in g.nodes.iter().enumerate() {
                    let (nx0, nx1) = (x[v] - node.left, x[v] + node.right);
                    let ny = y[node.layer];
                    let (ny0, ny1) = (ny - node.thick / 2.0, ny + node.thick / 2.0);
                    let member = cl.within(node.cluster, c);
                    if member {
                        assert!(nx0 >= bx.x0 + 12.0 - 1e-9 && nx1 <= bx.x1 - 12.0 + 1e-9);
                        assert!(ny0 >= bx.y0 + 30.0 - 1e-9 && ny1 <= bx.y1 - 12.0 + 1e-9);
                    } else if !matches!(node.kind, Kind::Dummy(_)) || true {
                        let overlap = nx0 < bx.x1 && nx1 > bx.x0 && ny0 < bx.y1 && ny1 > bx.y0;
                        let is_ancestor_filler =
                            matches!(node.kind, Kind::Filler(f) if cl.within(Some(c), f));
                        assert!(
                            !overlap || is_ancestor_filler,
                            "node {:?} overlaps cluster {}",
                            node.kind,
                            c
                        );
                    }
                }
            }
            // Sibling cluster boxes do not overlap.
            if let (Some(a), Some(b)) = (boxes[0], boxes[2]) {
                assert!(a.x1 <= b.x0 || b.x1 <= a.x0 || a.y1 <= b.y0 || b.y1 <= a.y0);
            }
            assert!(thick.iter().all(|&t| t >= 0.0));
        }
    }

    #[test]
    fn layer_gaps_respect_rank_spacing_and_cluster_titles() {
        let cl = clusters(&[None], &[None, Some(0)]);
        let g = graph(&[40.0, 40.0], &[0, 1], &[(0, 1)], &cl, 0.0);
        let p = vec![Pad {
            order_before: 12.0,
            order_after: 12.0,
            layer_before: 80.0,
            layer_after: 12.0,
        }];
        let (y, thick) = layer_y(&g, &cl, &p, 48.0, &[]);
        assert_eq!(thick, vec![20.0, 20.0]);
        // The cluster starting at layer 1 needs 80 px above its members plus a gap.
        assert!(y[1] - y[0] - 20.0 >= 80.0);
        let cl0 = Clusters::default();
        let g0 = graph(&[40.0, 40.0], &[0, 1], &[(0, 1)], &cl0, 0.0);
        let (y0, _) = layer_y(&g0, &cl0, &[], 48.0, &[]);
        assert_eq!(y0[1] - y0[0], 48.0 + 20.0);
        // A wider minimum for one gap (an edge label) widens only that gap.
        let (y1, _) = layer_y(&g0, &cl0, &[], 48.0, &[70.0]);
        assert_eq!(y1[1] - y1[0], 70.0 + 20.0);
    }

    #[test]
    fn exhausted_fuel_is_reported() {
        let cl = Clusters::default();
        let g = graph(
            &[40.0; 6],
            &[0, 1, 2, 0, 1, 2],
            &[(0, 1), (1, 2), (3, 4), (4, 5)],
            &cl,
            0.0,
        );
        assert!(assign_x(&g, 24.0, &mut Fuel::new(3)).is_err());
    }
}
