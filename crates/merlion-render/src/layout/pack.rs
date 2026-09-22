//! Disconnected components (specs/layout.md#5-container-fit, component packing).
//!
//! Each weakly connected component, with every cluster holding one of its nodes, is
//! laid out on its own and the drawings are packed in declaration order: along the
//! order axis first, so `TB`/`BT` components form rows that start a new row below
//! when the next component would pass `target_width`, and `LR`/`RL` components stack
//! in one column. A component therefore moves only when a component declared before
//! it in the same row changes size.

use alloc::vec;
use alloc::vec::Vec;

use crate::geometry::{ClusterGeom, EdgeGeom, Geometry, NodeGeom, Point};
use crate::math::max;
use crate::model::Flowchart;
use crate::options::Direction;
use crate::text::LabelLayout;

use super::pipeline::MARGIN;

/// One component: model indices in ascending order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Component {
    pub nodes: Vec<usize>,
    pub edges: Vec<usize>,
    pub subgraphs: Vec<usize>,
}

fn find(parent: &mut [usize], mut v: usize) -> usize {
    let mut root = v;
    while parent[root] != root {
        root = parent[root];
    }
    while parent[v] != root {
        let next = parent[v];
        parent[v] = root;
        v = next;
    }
    root
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    // The smaller index is the root, so roots follow declaration order.
    if ra < rb {
        parent[rb] = ra;
    } else if rb < ra {
        parent[ra] = rb;
    }
}

/// The components of `chart` in declaration order (by first node), or `None` when it
/// has fewer than two, or a cluster no node belongs to (which stays in one layered
/// graph with the rest).
pub fn components(chart: &Flowchart) -> Option<Vec<Component>> {
    let n = chart.nodes.len();
    let ns = chart.subgraphs.len();
    if n < 2 {
        return None;
    }
    // Outermost ancestor of every subgraph; parents precede their children.
    let mut top = vec![0usize; ns];
    for (s, sg) in chart.subgraphs.iter().enumerate() {
        top[s] = match sg.parent {
            None => s,
            Some(p) if p < s => top[p],
            Some(_) => return None,
        };
    }
    let mut parent: Vec<usize> = (0..n).collect();
    for e in &chart.edges {
        if e.from < n && e.to < n {
            union(&mut parent, e.from, e.to);
        }
    }
    let mut anchor: Vec<Option<usize>> = vec![None; ns];
    for (v, node) in chart.nodes.iter().enumerate() {
        let Some(s) = node.subgraph else { continue };
        let t = *top.get(s)?;
        match anchor[t] {
            Some(a) => union(&mut parent, a, v),
            None => anchor[t] = Some(v),
        }
    }
    // A subgraph without a descendant node has no component to join.
    let mut has_node = vec![false; ns];
    for node in &chart.nodes {
        let mut cur = node.subgraph;
        let mut guard = 0;
        while let Some(s) = cur {
            if s >= ns || has_node[s] || guard > ns {
                break;
            }
            has_node[s] = true;
            cur = chart.subgraphs[s].parent;
            guard += 1;
        }
    }
    if has_node.iter().any(|&h| !h) {
        return None;
    }
    let mut index = vec![usize::MAX; n];
    let mut comps: Vec<Component> = Vec::new();
    for v in 0..n {
        let r = find(&mut parent, v);
        if index[r] == usize::MAX {
            index[r] = comps.len();
            comps.push(Component::default());
        }
        comps[index[r]].nodes.push(v);
    }
    if comps.len() < 2 {
        return None;
    }
    for (e, edge) in chart.edges.iter().enumerate() {
        if edge.from < n && edge.to < n {
            let r = find(&mut parent, edge.from);
            comps[index[r]].edges.push(e);
        }
    }
    for s in 0..ns {
        if let Some(a) = anchor[top[s]] {
            let r = find(&mut parent, a);
            comps[index[r]].subgraphs.push(s);
        }
    }
    Some(comps)
}

/// The flowchart of one component in direction `dir`, with indices renumbered.
pub fn sub_chart(chart: &Flowchart, c: &Component, dir: Direction) -> Flowchart {
    let mut node_map = vec![usize::MAX; chart.nodes.len()];
    for (i, &v) in c.nodes.iter().enumerate() {
        node_map[v] = i;
    }
    let mut sub_map = vec![usize::MAX; chart.subgraphs.len()];
    for (i, &s) in c.subgraphs.iter().enumerate() {
        sub_map[s] = i;
    }
    let local_sub = |s: Option<usize>| {
        s.and_then(|s| sub_map.get(s).copied())
            .filter(|&s| s != usize::MAX)
    };
    let nodes = c
        .nodes
        .iter()
        .map(|&v| {
            let mut node = chart.nodes[v].clone();
            node.subgraph = local_sub(node.subgraph);
            node
        })
        .collect();
    let subgraphs = c
        .subgraphs
        .iter()
        .map(|&s| {
            let mut sg = chart.subgraphs[s].clone();
            sg.parent = local_sub(sg.parent);
            sg.nodes = sg
                .nodes
                .iter()
                .filter_map(|&v| node_map.get(v).copied().filter(|&i| i != usize::MAX))
                .collect();
            sg
        })
        .collect();
    let edges = c
        .edges
        .iter()
        .map(|&e| {
            let mut edge = chart.edges[e].clone();
            edge.from = node_map[edge.from];
            edge.to = node_map[edge.to];
            edge
        })
        .collect();
    Flowchart {
        meta: chart.meta.clone(),
        direction: dir,
        nodes,
        edges,
        subgraphs,
        class_defs: chart.class_defs.clone(),
        default_link_style: chart.default_link_style.clone(),
    }
}

/// Packs the component drawings `geoms` (one per component, each with its
/// [`MARGIN`]) into one geometry indexed like `chart`.
pub fn merge(
    chart: &Flowchart,
    comps: &[Component],
    geoms: &[Geometry],
    dir: Direction,
    target_width: f64,
    node_spacing: f64,
    rank_spacing: f64,
) -> Geometry {
    let content = |g: &Geometry| {
        (
            max(g.width - 2.0 * MARGIN, 0.0),
            max(g.height - 2.0 * MARGIN, 0.0),
        )
    };
    // Top-left corner of each component's content, relative to the drawing's content.
    let mut at = vec![(0.0f64, 0.0f64); geoms.len()];
    let mut width = 0.0f64;
    let height;
    if dir.is_horizontal() {
        let widest = geoms.iter().map(|g| content(g).0).fold(0.0, max);
        let mut y = 0.0;
        for (k, g) in geoms.iter().enumerate() {
            let (w, h) = content(g);
            if k > 0 {
                y += node_spacing;
            }
            let x = if dir == Direction::RL {
                widest - w
            } else {
                0.0
            };
            at[k] = (x, y);
            y += h;
        }
        (width, height) = (widest, y);
    } else {
        let inner = target_width - 2.0 * MARGIN;
        // Rows of component indices.
        let mut rows: Vec<Vec<usize>> = Vec::new();
        let mut row_w = 0.0;
        for (k, g) in geoms.iter().enumerate() {
            let w = content(g).0;
            match rows.last_mut() {
                Some(row) if row_w + node_spacing + w <= inner => {
                    row_w += node_spacing + w;
                    row.push(k);
                }
                _ => {
                    rows.push(vec![k]);
                    row_w = w;
                }
            }
        }
        let mut y = 0.0;
        for (r, row) in rows.iter().enumerate() {
            if r > 0 {
                y += rank_spacing;
            }
            let row_h = row.iter().map(|&k| content(&geoms[k]).1).fold(0.0, max);
            let mut x = 0.0;
            for (i, &k) in row.iter().enumerate() {
                let (w, h) = content(&geoms[k]);
                if i > 0 {
                    x += node_spacing;
                }
                // Layer 0 sits on the row's first side: the top for TB, the bottom for BT.
                let dy = if dir == Direction::BT { row_h - h } else { 0.0 };
                at[k] = (x, y + dy);
                x += w;
            }
            width = max(width, x);
            y += row_h;
        }
        height = y;
    }

    let blank = NodeGeom {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
        label: LabelLayout::default(),
        rank: 0,
    };
    let mut nodes = vec![blank; chart.nodes.len()];
    let mut edges: Vec<EdgeGeom> = chart
        .edges
        .iter()
        .map(|_| EdgeGeom {
            points: Vec::new(),
            label: None,
            back: false,
            wrap: false,
        })
        .collect();
    let mut clusters = vec![None; chart.subgraphs.len()];
    let mut layers: Vec<Vec<usize>> = Vec::new();
    for ((c, g), &(ox, oy)) in comps.iter().zip(geoms).zip(&at) {
        // Component content starts at MARGIN in its own drawing and at MARGIN + at
        // in the packed one.
        let (dx, dy) = (ox, oy);
        for (i, &v) in c.nodes.iter().enumerate() {
            if let (Some(slot), Some(ng)) = (nodes.get_mut(v), g.nodes.get(i)) {
                let mut ng = ng.clone();
                ng.x += dx;
                ng.y += dy;
                *slot = ng;
            }
        }
        for (i, &e) in c.edges.iter().enumerate() {
            if let (Some(slot), Some(eg)) = (edges.get_mut(e), g.edges.get(i)) {
                let mut eg = eg.clone();
                for p in eg.points.iter_mut() {
                    *p = Point::new(p.x + dx, p.y + dy);
                }
                if let Some(l) = eg.label.as_mut() {
                    l.x += dx;
                    l.y += dy;
                }
                *slot = eg;
            }
        }
        for (i, &s) in c.subgraphs.iter().enumerate() {
            if let (Some(slot), Some(cg)) = (clusters.get_mut(s), g.clusters.get(i)) {
                let mut cg = cg.clone();
                cg.x += dx;
                cg.y += dy;
                cg.label_x += dx;
                cg.label_y += dy;
                *slot = Some(cg);
            }
        }
        for (l, layer) in g.layers.iter().enumerate() {
            if layers.len() <= l {
                layers.resize(l + 1, Vec::new());
            }
            layers[l].extend(layer.iter().filter_map(|&i| c.nodes.get(i).copied()));
        }
    }
    Geometry {
        width: width + 2.0 * MARGIN,
        height: height + 2.0 * MARGIN,
        direction: dir,
        nodes,
        edges,
        clusters: clusters
            .into_iter()
            .map(|c| {
                c.unwrap_or(ClusterGeom {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                    label: LabelLayout::default(),
                    label_x: 0.0,
                    label_y: 0.0,
                })
            })
            .collect(),
        layers,
        fuel_used: 0,
    }
}
