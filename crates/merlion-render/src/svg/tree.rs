//! Cluster tree and draw order.
//!
//! Z-order (specs/svg-output.md#ids-and-data-attributes: clusters have "members nested
//! inside"): each cluster is a `<g class="merlion-cluster">` holding, in order,
//!
//! 1. its box and title,
//! 2. its child clusters (recursively, same order),
//! 3. the edges whose endpoints' lowest common cluster is this one,
//! 4. its direct member nodes.
//!
//! The root level follows the same order: top-level clusters, root edges, root nodes.
//! So a cluster box is always under the edges and nodes inside it, an edge is under the
//! nodes of its own cluster, and an edge between two sibling clusters is drawn above
//! both boxes (it would otherwise vanish under the second box's fill).
//!
//! The model promises a parent precedes its children, but the draw stage never relies
//! on it: a parent index that is out of range or on a cycle makes the cluster top-level,
//! and the tree is walked with an explicit stack, so no input recurses or loops forever.

use alloc::vec::Vec;

use crate::model::Flowchart;

/// Parent of every subgraph after dropping invalid and cyclic links.
pub fn effective_parents(chart: &Flowchart) -> Vec<Option<usize>> {
    let n = chart.subgraphs.len();
    let raw: Vec<Option<usize>> = chart
        .subgraphs
        .iter()
        .enumerate()
        .map(|(i, s)| s.parent.filter(|&p| p < n && p != i))
        .collect();
    (0..n)
        .map(|i| {
            // Walk up at most n steps; a chain that does not reach a root within n steps cycles.
            let mut cur = i;
            for _ in 0..n {
                match raw.get(cur).copied().flatten() {
                    Some(p) if p == i => return None,
                    Some(p) => cur = p,
                    None => return raw.get(i).copied().flatten(),
                }
            }
            None
        })
        .collect()
}

/// Chain of clusters from `sg` up to the root (inclusive of `sg`), innermost first.
fn ancestors(parents: &[Option<usize>], sg: Option<usize>) -> Vec<usize> {
    let mut out = Vec::new();
    let mut cur = sg.filter(|&s| s < parents.len());
    while let Some(s) = cur {
        if out.len() > parents.len() {
            break;
        }
        out.push(s);
        cur = parents.get(s).copied().flatten();
    }
    out
}

/// Innermost cluster containing both endpoints, `None` for the root.
pub fn common_cluster(
    parents: &[Option<usize>],
    a: Option<usize>,
    b: Option<usize>,
) -> Option<usize> {
    let pa = ancestors(parents, a);
    let pb = ancestors(parents, b);
    pa.into_iter().find(|s| pb.contains(s))
}

/// One step of the draw order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// Open cluster `i`: its `<g>`, box and title.
    Open(usize),
    /// Close cluster `i`.
    Close(usize),
    Edge(usize),
    Node(usize),
}

/// The document order of every cluster, edge and node, per the module docs.
/// `edge_home[e]` is the cluster edge `e` belongs to; `node_home[n]` likewise.
pub fn draw_order(
    parents: &[Option<usize>],
    node_home: &[Option<usize>],
    edge_home: &[Option<usize>],
) -> Vec<Item> {
    let n = parents.len();
    // Children lists, in declaration order.
    let mut children: Vec<Vec<usize>> = alloc::vec![Vec::new(); n];
    let mut roots = Vec::new();
    for (i, p) in parents.iter().enumerate() {
        match p.and_then(|p| children.get_mut(p)) {
            Some(c) => c.push(i),
            None => roots.push(i),
        }
    }
    let mut edges_of: Vec<Vec<usize>> = alloc::vec![Vec::new(); n];
    let mut root_edges = Vec::new();
    for (e, h) in edge_home.iter().enumerate() {
        match h.and_then(|h| edges_of.get_mut(h)) {
            Some(v) => v.push(e),
            None => root_edges.push(e),
        }
    }
    let mut nodes_of: Vec<Vec<usize>> = alloc::vec![Vec::new(); n];
    let mut root_nodes = Vec::new();
    for (i, h) in node_home.iter().enumerate() {
        match h.and_then(|h| nodes_of.get_mut(h)) {
            Some(v) => v.push(i),
            None => root_nodes.push(i),
        }
    }

    let mut out = Vec::new();
    // Explicit stack of pending work; `Enter(c)` expands a cluster.
    enum Work {
        Enter(usize),
        Emit(Item),
    }
    let mut stack: Vec<Work> = Vec::new();
    // Pushed in reverse so they pop in order.
    for &i in root_nodes.iter().rev() {
        stack.push(Work::Emit(Item::Node(i)));
    }
    for &e in root_edges.iter().rev() {
        stack.push(Work::Emit(Item::Edge(e)));
    }
    for &c in roots.iter().rev() {
        stack.push(Work::Enter(c));
    }
    let mut entered = alloc::vec![false; n];
    while let Some(w) = stack.pop() {
        match w {
            Work::Emit(item) => out.push(item),
            Work::Enter(c) => {
                match entered.get_mut(c) {
                    Some(seen) if !*seen => *seen = true,
                    _ => continue,
                }
                out.push(Item::Open(c));
                stack.push(Work::Emit(Item::Close(c)));
                for &i in nodes_of.get(c).into_iter().flatten().rev() {
                    stack.push(Work::Emit(Item::Node(i)));
                }
                for &e in edges_of.get(c).into_iter().flatten().rev() {
                    stack.push(Work::Emit(Item::Edge(e)));
                }
                for &k in children.get(c).into_iter().flatten().rev() {
                    stack.push(Work::Enter(k));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Subgraph;
    use alloc::string::String;
    use alloc::vec;

    fn sg(parent: Option<usize>) -> Subgraph {
        Subgraph {
            id: String::from("s"),
            title: String::new(),
            parent,
            nodes: vec![],
            direction: None,
            span: Default::default(),
        }
    }

    #[test]
    fn cycles_and_bad_indices_become_roots() {
        let chart = Flowchart {
            subgraphs: vec![
                sg(Some(1)),
                sg(Some(0)),
                sg(Some(99)),
                sg(Some(3)),
                sg(None),
                sg(Some(4)),
            ],
            ..Flowchart::default()
        };
        assert_eq!(
            effective_parents(&chart),
            vec![None, None, None, None, None, Some(4)]
        );
    }

    #[test]
    fn common_cluster_is_innermost_shared() {
        let parents = vec![None, Some(0), Some(0), Some(1)];
        assert_eq!(common_cluster(&parents, Some(3), Some(2)), Some(0));
        assert_eq!(common_cluster(&parents, Some(3), Some(1)), Some(1));
        assert_eq!(common_cluster(&parents, Some(3), None), None);
        assert_eq!(common_cluster(&parents, Some(3), Some(3)), Some(3));
    }

    #[test]
    fn order_is_clusters_then_edges_then_nodes() {
        // Cluster 0 holds cluster 1; node 0 in 1, node 1 in 0, node 2 at root.
        let parents = vec![None, Some(0)];
        let order = draw_order(&parents, &[Some(1), Some(0), None], &[Some(0), None]);
        assert_eq!(
            order,
            vec![
                Item::Open(0),
                Item::Open(1),
                Item::Node(0),
                Item::Close(1),
                Item::Edge(0),
                Item::Node(1),
                Item::Close(0),
                Item::Edge(1),
                Item::Node(2),
            ]
        );
    }

    #[test]
    fn deep_nesting_does_not_recurse() {
        let parents: Vec<Option<usize>> = (0..10_000)
            .map(|i| if i == 0 { None } else { Some(i - 1) })
            .collect();
        let order = draw_order(&parents, &[Some(9_999)], &[]);
        assert_eq!(order.len(), 20_001);
    }
}
