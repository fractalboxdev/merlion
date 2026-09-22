//! The layered graph: real nodes, dummy nodes for long edges, label dummies, and
//! cluster fillers, each in one layer (specs/layout.md#2-layer-assignment).
//!
//! Coordinates in the layered graph use a direction-free frame: the *order axis* runs
//! along a layer and the *layer axis* across layers (x and y for `TB`). Each node has an
//! extent on the order axis to the left and right of its centre, and a thickness on the
//! layer axis.

use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use crate::model::Flowchart;

/// Deepest cluster nesting the layout follows; deeper parent links are cut. Matches the
/// parser's nesting limit (specs/architecture.md#boundaries).
pub const MAX_CLUSTER_DEPTH: usize = 64;

/// The validated cluster (subgraph) tree. Parent links that point out of range, at
/// the cluster itself, into a cycle, or deeper than [`MAX_CLUSTER_DEPTH`] are cut, so
/// every chain of parents ends at the root within 64 steps.
#[derive(Clone, Debug, Default)]
pub struct Clusters {
    pub parent: Vec<Option<usize>>,
    pub depth: Vec<usize>,
    /// Innermost cluster of every model node (`None` at the top level).
    pub node_cluster: Vec<Option<usize>>,
}

impl Clusters {
    pub fn from_chart(chart: &Flowchart) -> Self {
        let k = chart.subgraphs.len();
        let mut parent: Vec<Option<usize>> = chart
            .subgraphs
            .iter()
            .enumerate()
            .map(|(i, s)| s.parent.filter(|&p| p < k && p != i))
            .collect();
        for i in 0..k {
            let mut cur = parent[i];
            let mut steps = 0usize;
            while let Some(c) = cur {
                if c == i || steps >= MAX_CLUSTER_DEPTH {
                    parent[i] = None;
                    break;
                }
                cur = parent[c];
                steps += 1;
            }
        }
        let depth = (0..k)
            .map(|i| {
                let mut d = 0;
                let mut cur = parent[i];
                while let Some(c) = cur {
                    d += 1;
                    cur = parent[c];
                    if d > MAX_CLUSTER_DEPTH {
                        break;
                    }
                }
                d
            })
            .collect();
        let node_cluster = chart
            .nodes
            .iter()
            .map(|n| n.subgraph.filter(|&s| s < k))
            .collect();
        Clusters {
            parent,
            depth,
            node_cluster,
        }
    }

    pub fn len(&self) -> usize {
        self.parent.len()
    }

    fn up(&self, c: Option<usize>) -> Option<usize> {
        c.and_then(|c| self.parent.get(c).copied().flatten())
    }

    fn depth_of(&self, c: Option<usize>) -> usize {
        c.and_then(|c| self.depth.get(c).copied())
            .map_or(0, |d| d + 1)
    }

    /// Lowest common ancestor of two clusters (`None` is the root).
    pub fn lca(&self, a: Option<usize>, b: Option<usize>) -> Option<usize> {
        let (mut a, mut b) = (a, b);
        let (mut da, mut db) = (self.depth_of(a), self.depth_of(b));
        while da > db {
            a = self.up(a);
            da -= 1;
        }
        while db > da {
            b = self.up(b);
            db -= 1;
        }
        let mut guard = 0;
        while a != b && guard <= MAX_CLUSTER_DEPTH {
            a = self.up(a);
            b = self.up(b);
            guard += 1;
        }
        if a == b {
            a
        } else {
            None
        }
    }

    /// The clusters containing `c`, outermost first, ending with `c`.
    pub fn chain(&self, c: Option<usize>) -> Vec<usize> {
        let mut out = Vec::new();
        let mut cur = c.filter(|&c| c < self.len());
        while let Some(x) = cur {
            out.push(x);
            if out.len() > MAX_CLUSTER_DEPTH {
                break;
            }
            cur = self.up(Some(x));
        }
        out.reverse();
        out
    }

    /// Whether cluster `c` lies inside `anc` or is `anc`.
    #[cfg(test)]
    pub fn within(&self, c: Option<usize>, anc: usize) -> bool {
        let mut cur = c;
        let mut guard = 0;
        while let Some(x) = cur {
            if x == anc {
                return true;
            }
            guard += 1;
            if guard > MAX_CLUSTER_DEPTH {
                return false;
            }
            cur = self.up(Some(x));
        }
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Model node index.
    Real(usize),
    /// Intermediate node of a long edge (model edge index).
    Dummy(usize),
    /// The dummy of a labelled edge that carries its label (model edge index).
    Label(usize),
    /// Keeps a cluster present in a layer it spans but has no node in, or reserves the
    /// width of its title in its first layer (cluster index).
    Filler(usize),
    /// Stands in for a cluster with no nodes (cluster index).
    EmptyCluster(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LNode {
    pub kind: Kind,
    pub layer: usize,
    /// Extent along the order axis before and after the centre.
    pub left: f64,
    pub right: f64,
    /// Extent along the layer axis.
    pub thick: f64,
    /// Innermost cluster.
    pub cluster: Option<usize>,
}

impl LNode {
    pub fn is_dummy(&self) -> bool {
        matches!(self.kind, Kind::Dummy(_) | Kind::Label(_))
    }
}

/// The layered path of one model edge, from its upper to its lower endpoint.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub edge: usize,
    pub nodes: Vec<usize>,
    /// The edge points upwards in the source (drawn in its original direction).
    pub reversed: bool,
    pub label: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct LGraph {
    pub nodes: Vec<LNode>,
    /// Node ids per layer, in order.
    pub layers: Vec<Vec<usize>>,
    /// Neighbours in the layer above / below, one entry per segment.
    pub up: Vec<Vec<usize>>,
    pub down: Vec<Vec<usize>>,
    pub chains: Vec<Chain>,
    /// Layered node of each model node.
    pub real: Vec<usize>,
}

impl LGraph {
    /// Position of every node within its layer.
    pub fn positions(&self) -> Vec<usize> {
        let mut pos = vec![0usize; self.nodes.len()];
        for layer in &self.layers {
            for (i, &v) in layer.iter().enumerate() {
                if let Some(p) = pos.get_mut(v) {
                    *p = i;
                }
            }
        }
        pos
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Extent {
    pub left: f64,
    pub right: f64,
    pub thick: f64,
}

/// One model edge oriented downwards: `upper` and `lower` are model node indices.
pub struct EdgeIn {
    pub edge: usize,
    pub upper: usize,
    pub lower: usize,
    pub reversed: bool,
    pub label: Option<Extent>,
}

pub struct BuildIn<'a> {
    /// Extent of every model node.
    pub real: &'a [Extent],
    /// Layer of every model node.
    pub layer: &'a [usize],
    pub edges: &'a [EdgeIn],
    pub clusters: &'a Clusters,
    /// Title extent per cluster (a zero `left + right` means no title filler).
    pub titles: &'a [Extent],
    /// Placeholder size per cluster, used when the cluster has no nodes.
    pub empty_size: &'a [Extent],
    pub max_nodes: usize,
    pub max_layers: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    TooManyNodes,
    TooManyLayers,
}

/// Builds the layered graph. Layers are checked against `max_layers` and the total node
/// count (real, dummy, filler) against `max_nodes` before anything large is allocated.
pub fn build(input: &BuildIn) -> Result<LGraph, BuildError> {
    let n = input.real.len().min(input.layer.len());
    let cl = input.clusters;
    let k = cl.len();
    let layer_of = |v: usize| input.layer.get(v).copied().unwrap_or(0);
    let top_layer = (0..n).map(layer_of).max().unwrap_or(0);
    if top_layer >= input.max_layers {
        return Err(BuildError::TooManyLayers);
    }
    let mut count = n;
    for e in input.edges {
        if e.upper < n && e.lower < n {
            let span = layer_of(e.lower).saturating_sub(layer_of(e.upper));
            count = count.saturating_add(span.saturating_sub(1));
        }
    }
    if count > input.max_nodes {
        return Err(BuildError::TooManyNodes);
    }

    let mut g = LGraph::default();
    let push = |g: &mut LGraph, node: LNode| -> usize {
        g.nodes.push(node);
        g.up.push(Vec::new());
        g.down.push(Vec::new());
        g.nodes.len() - 1
    };
    let node_cluster = |v: usize| cl.node_cluster.get(v).copied().flatten();
    for v in 0..n {
        let e = input.real[v];
        let id = push(
            &mut g,
            LNode {
                kind: Kind::Real(v),
                layer: layer_of(v),
                left: e.left,
                right: e.right,
                thick: e.thick,
                cluster: node_cluster(v),
            },
        );
        g.real.push(id);
    }

    // Clusters without any node become placeholders inside their parent, at the
    // parent's first layer; nested empty clusters share their top empty ancestor's box.
    let mut has_real = vec![false; k];
    let mut min_layer = vec![usize::MAX; k];
    for v in 0..n {
        for c in cl.chain(node_cluster(v)) {
            has_real[c] = true;
            min_layer[c] = min_layer[c].min(layer_of(v));
        }
    }
    for c in 0..k {
        if has_real[c] {
            continue;
        }
        let parent = cl.parent[c];
        if parent.is_some_and(|p| !has_real[p]) {
            continue;
        }
        let e = input.empty_size.get(c).copied().unwrap_or_default();
        push(
            &mut g,
            LNode {
                kind: Kind::EmptyCluster(c),
                layer: parent.map_or(0, |p| min_layer[p]),
                left: e.left,
                right: e.right,
                thick: e.thick,
                cluster: parent,
            },
        );
    }

    for e in input.edges {
        if e.upper >= n || e.lower >= n {
            continue;
        }
        let (ru, rv) = (g.real[e.upper], g.real[e.lower]);
        let (lu, lv) = (layer_of(e.upper), layer_of(e.lower));
        let mut chain = Chain {
            edge: e.edge,
            nodes: vec![ru],
            reversed: e.reversed,
            label: None,
        };
        if lv > lu {
            let cluster = cl.lca(node_cluster(e.upper), node_cluster(e.lower));
            let label_layer = if lv - lu >= 2 && e.label.is_some() {
                Some(lu + (lv - lu) / 2)
            } else {
                None
            };
            let mut prev = ru;
            for layer in lu + 1..lv {
                let is_label = label_layer == Some(layer);
                let ext = if is_label {
                    e.label.unwrap_or_default()
                } else {
                    Extent::default()
                };
                let d = push(
                    &mut g,
                    LNode {
                        kind: if is_label {
                            Kind::Label(e.edge)
                        } else {
                            Kind::Dummy(e.edge)
                        },
                        layer,
                        left: ext.left,
                        right: ext.right,
                        thick: ext.thick,
                        cluster,
                    },
                );
                if is_label {
                    chain.label = Some(d);
                }
                g.down[prev].push(d);
                g.up[d].push(prev);
                chain.nodes.push(d);
                prev = d;
            }
            g.down[prev].push(rv);
            g.up[rv].push(prev);
        }
        chain.nodes.push(rv);
        g.chains.push(chain);
    }

    // Cluster spans and presence per layer, then fillers, deepest clusters first so a
    // filler also marks its ancestors present.
    let mut lo = vec![usize::MAX; k];
    let mut hi = vec![0usize; k];
    let mut present: BTreeSet<(usize, usize)> = BTreeSet::new();
    for node in &g.nodes {
        for c in cl.chain(node.cluster) {
            lo[c] = lo[c].min(node.layer);
            hi[c] = hi[c].max(node.layer);
            present.insert((c, node.layer));
        }
    }
    let mut by_depth: Vec<usize> = (0..k).collect();
    by_depth.sort_by_key(|&c| (core::cmp::Reverse(cl.depth[c]), c));
    for c in by_depth {
        if lo[c] == usize::MAX {
            continue;
        }
        let title = input.titles.get(c).copied().unwrap_or_default();
        if title.left + title.right > 0.0 || title.thick > 0.0 {
            count = count.saturating_add(1);
            push(
                &mut g,
                LNode {
                    kind: Kind::Filler(c),
                    layer: lo[c],
                    left: title.left,
                    right: title.right,
                    thick: title.thick,
                    cluster: Some(c),
                },
            );
        }
        for layer in lo[c]..=hi[c] {
            if present.contains(&(c, layer)) {
                continue;
            }
            count = count.saturating_add(1);
            if count > input.max_nodes {
                return Err(BuildError::TooManyNodes);
            }
            push(
                &mut g,
                LNode {
                    kind: Kind::Filler(c),
                    layer,
                    left: 0.0,
                    right: 0.0,
                    thick: 0.0,
                    cluster: Some(c),
                },
            );
            for a in cl.chain(Some(c)) {
                present.insert((a, layer));
            }
        }
    }
    if g.nodes.len() > input.max_nodes {
        return Err(BuildError::TooManyNodes);
    }

    let layer_count = g.nodes.iter().map(|v| v.layer + 1).max().unwrap_or(0);
    g.layers = vec![Vec::new(); layer_count];
    for (i, node) in g.nodes.iter().enumerate() {
        g.layers[node.layer].push(i);
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, Shape, Style, Subgraph};
    use alloc::string::String;
    use alloc::vec;

    fn ext(w: f64) -> Extent {
        Extent {
            left: w / 2.0,
            right: w / 2.0,
            thick: 20.0,
        }
    }

    fn chart_with_clusters(parents: &[Option<usize>], node_sub: &[Option<usize>]) -> Flowchart {
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
        c
    }

    fn input<'a>(
        real: &'a [Extent],
        layer: &'a [usize],
        edges: &'a [EdgeIn],
        cl: &'a Clusters,
        titles: &'a [Extent],
    ) -> BuildIn<'a> {
        BuildIn {
            real,
            layer,
            edges,
            clusters: cl,
            titles,
            empty_size: titles,
            max_nodes: 1000,
            max_layers: 100,
        }
    }

    #[test]
    fn clusters_validate_parents_and_compute_lca() {
        // 0 <- 1 <- 2, 3 is a sibling of 1; 4 has a cyclic parent chain with 5.
        let chart = chart_with_clusters(
            &[None, Some(0), Some(1), Some(0), Some(5), Some(4), Some(99)],
            &[Some(2), Some(3), None, Some(42)],
        );
        let cl = Clusters::from_chart(&chart);
        assert_eq!(cl.parent[2], Some(1));
        assert_eq!(cl.depth[2], 2);
        assert_eq!(cl.parent[6], None);
        // A cycle is cut so that every chain reaches the root.
        assert!(cl.parent[4].is_none() || cl.parent[5].is_none());
        assert_eq!(cl.node_cluster, vec![Some(2), Some(3), None, None]);
        assert_eq!(cl.lca(Some(2), Some(3)), Some(0));
        assert_eq!(cl.lca(Some(2), Some(1)), Some(1));
        assert_eq!(cl.lca(Some(2), None), None);
        assert_eq!(cl.chain(Some(2)), vec![0, 1, 2]);
    }

    #[test]
    fn long_edges_get_one_dummy_per_intermediate_layer() {
        let cl = Clusters::default();
        let real = [ext(40.0), ext(40.0)];
        let edges = [EdgeIn {
            edge: 0,
            upper: 0,
            lower: 1,
            reversed: false,
            label: None,
        }];
        let g = build(&input(&real, &[0, 3], &edges, &cl, &[])).unwrap();
        assert_eq!(g.nodes.len(), 4);
        assert_eq!(g.layers.len(), 4);
        let ch = &g.chains[0];
        assert_eq!(ch.nodes.len(), 4);
        for (i, &v) in ch.nodes.iter().enumerate() {
            assert_eq!(g.nodes[v].layer, i);
        }
        assert_eq!(g.nodes[ch.nodes[1]].kind, Kind::Dummy(0));
        assert_eq!(g.down[ch.nodes[1]], vec![ch.nodes[2]]);
        assert_eq!(g.up[ch.nodes[1]], vec![ch.nodes[0]]);
    }

    #[test]
    fn label_dummy_sits_mid_edge_with_label_extent() {
        let cl = Clusters::default();
        let real = [ext(40.0), ext(40.0)];
        let edges = [EdgeIn {
            edge: 0,
            upper: 0,
            lower: 1,
            reversed: true,
            label: Some(ext(70.0)),
        }];
        let g = build(&input(&real, &[0, 2], &edges, &cl, &[])).unwrap();
        let ch = &g.chains[0];
        let l = ch.label.unwrap();
        assert_eq!(g.nodes[l].kind, Kind::Label(0));
        assert_eq!(g.nodes[l].layer, 1);
        assert_eq!(g.nodes[l].left, 35.0);
        assert!(ch.reversed);
    }

    #[test]
    fn dummies_belong_to_the_common_cluster() {
        let chart = chart_with_clusters(&[None, Some(0)], &[Some(1), Some(0), None]);
        let cl = Clusters::from_chart(&chart);
        let real = [ext(40.0), ext(40.0), ext(40.0)];
        let edges = [
            EdgeIn {
                edge: 0,
                upper: 0,
                lower: 1,
                reversed: false,
                label: None,
            },
            EdgeIn {
                edge: 1,
                upper: 0,
                lower: 2,
                reversed: false,
                label: None,
            },
        ];
        let titles = [ext(0.0), ext(0.0)];
        let g = build(&input(&real, &[0, 2, 2], &edges, &cl, &titles)).unwrap();
        assert_eq!(g.nodes[g.chains[0].nodes[1]].cluster, Some(0));
        assert_eq!(g.nodes[g.chains[1].nodes[1]].cluster, None);
    }

    #[test]
    fn fillers_keep_clusters_present_in_every_spanned_layer() {
        // Cluster 0 holds nodes at layers 0 and 2 without an edge between them.
        let chart = chart_with_clusters(&[None], &[Some(0), Some(0), None]);
        let cl = Clusters::from_chart(&chart);
        let real = [ext(40.0), ext(40.0), ext(40.0)];
        let titles = [ext(90.0)];
        let g = build(&input(&real, &[0, 2, 1], &[], &cl, &titles)).unwrap();
        let fillers: Vec<&LNode> = g
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Filler(0)))
            .collect();
        // One gap filler at layer 1 and one title filler at layer 0.
        assert_eq!(fillers.len(), 2);
        assert!(fillers.iter().any(|f| f.layer == 1 && f.left == 0.0));
        assert!(fillers.iter().any(|f| f.layer == 0 && f.left == 45.0));
    }

    #[test]
    fn empty_cluster_becomes_a_placeholder_node() {
        let chart = chart_with_clusters(&[None, Some(0)], &[Some(0)]);
        let cl = Clusters::from_chart(&chart);
        let real = [ext(40.0)];
        let titles = [ext(30.0), ext(50.0)];
        let g = build(&input(&real, &[3], &[], &cl, &titles)).unwrap();
        let p = g
            .nodes
            .iter()
            .find(|n| n.kind == Kind::EmptyCluster(1))
            .unwrap();
        assert_eq!(p.cluster, Some(0));
        assert_eq!(p.layer, 3);
    }

    #[test]
    fn limits_are_enforced() {
        let cl = Clusters::default();
        let real = [ext(40.0), ext(40.0)];
        let edges = [EdgeIn {
            edge: 0,
            upper: 0,
            lower: 1,
            reversed: false,
            label: None,
        }];
        let mut inp = input(&real, &[0, 50], &edges, &cl, &[]);
        inp.max_nodes = 20;
        assert_eq!(build(&inp).unwrap_err(), BuildError::TooManyNodes);
        let mut inp = input(&real, &[0, 50], &edges, &cl, &[]);
        inp.max_layers = 10;
        assert_eq!(build(&inp).unwrap_err(), BuildError::TooManyLayers);
    }
}
