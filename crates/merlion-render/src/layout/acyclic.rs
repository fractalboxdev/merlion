//! Phase 1, cycle removal (specs/layout.md#1-cycle-removal).
//!
//! 1. A virtual root gets an edge to every entry node: nodes with no incoming edge,
//!    and the first declared node of every strongly connected component that has no
//!    incoming edge from outside it.
//! 2. The dominator tree from that root is computed with the iterative algorithm of
//!    Cooper, Harvey and Kennedy (*A Simple, Fast Dominance Algorithm*, 2001). An edge
//!    whose target dominates its source is a loop back-edge and is reversed.
//! 3. Every strongly connected component that still has a cycle (irreducible loops) is
//!    ordered by the Eades–Lin–Smyth greedy heuristic (IPL 1993); edges pointing
//!    backwards in that order are reversed.
//!
//! Every step is iterative (no recursion) and burns one fuel unit per edge or node
//! step, so adversarial input cannot exhaust the stack or run unbounded.

use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use crate::fuel::{Fuel, OutOfFuel};

const NONE: usize = usize::MAX;

/// Result of phase 1 for `n` nodes and the given edge list.
pub struct CycleInfo {
    /// `reversed[e]`: edge `e` points upwards in the layering (drawn as a back-edge).
    pub reversed: Vec<bool>,
    /// Immediate dominator; `None` for entry nodes (dominated only by the virtual root).
    pub idom: Vec<Option<usize>>,
    /// Dominator-tree depth clamped to 15 (entries are 0): `data-merlion-rank`.
    pub rank: Vec<u8>,
    /// Preorder index in a depth-first walk of the dominator tree, children in
    /// declaration order: the initial within-layer order of phase 3.
    pub dfs_key: Vec<usize>,
}

fn out_lists(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new(); n];
    for &(u, v) in edges {
        if u < n && v < n {
            out[u].push(v);
        }
    }
    out
}

/// Strongly connected components (Tarjan 1972, iterative). Component ids come in
/// reverse topological order of the condensation: sinks first.
pub fn scc(
    n: usize,
    edges: &[(usize, usize)],
    fuel: &mut Fuel,
) -> Result<Vec<usize>, OutOfFuel> {
    let out = out_lists(n, edges);
    let mut index = vec![NONE; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut comp = vec![NONE; n];
    let mut next_index = 0usize;
    let mut ncomp = 0usize;
    let mut call: Vec<(usize, usize)> = Vec::new();
    for s in 0..n {
        if index[s] != NONE {
            continue;
        }
        index[s] = next_index;
        low[s] = next_index;
        next_index += 1;
        stack.push(s);
        on_stack[s] = true;
        call.push((s, 0));
        while let Some(top) = call.last_mut() {
            fuel.burn(1)?;
            let v = top.0;
            if let Some(&w) = out[v].get(top.1) {
                top.1 += 1;
                if index[w] == NONE {
                    index[w] = next_index;
                    low[w] = next_index;
                    next_index += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    call.push((w, 0));
                } else if on_stack[w] && index[w] < low[v] {
                    low[v] = index[w];
                }
                continue;
            }
            call.pop();
            if low[v] == index[v] {
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    comp[w] = ncomp;
                    if w == v {
                        break;
                    }
                }
                ncomp += 1;
            }
            if let Some(&(p, _)) = call.last() {
                if low[v] < low[p] {
                    low[p] = low[v];
                }
            }
        }
    }
    Ok(comp)
}

/// Eades–Lin–Smyth greedy order of the nodes in `members`, using only `edges` whose
/// both ends are members. Sinks go to the end, sources to the front, and otherwise the
/// node with the largest out-degree minus in-degree (lowest index on ties) goes next.
fn els_order(
    members: &[usize],
    edges: &[(usize, usize)],
    local: &mut [usize],
    fuel: &mut Fuel,
) -> Result<Vec<usize>, OutOfFuel> {
    let k = members.len();
    for (i, &v) in members.iter().enumerate() {
        local[v] = i;
    }
    let mut outs: Vec<Vec<usize>> = vec![Vec::new(); k];
    let mut ins: Vec<Vec<usize>> = vec![Vec::new(); k];
    for &(u, v) in edges {
        let (lu, lv) = (local[u], local[v]);
        if lu != NONE && lv != NONE {
            outs[lu].push(lv);
            ins[lv].push(lu);
        }
    }
    let mut outdeg: Vec<usize> = outs.iter().map(Vec::len).collect();
    let mut indeg: Vec<usize> = ins.iter().map(Vec::len).collect();
    let mut removed = vec![false; k];
    let mut sinks: BTreeSet<usize> = (0..k).filter(|&i| outdeg[i] == 0).collect();
    let mut sources: BTreeSet<usize> = (0..k).filter(|&i| indeg[i] == 0).collect();
    let mut s1: Vec<usize> = Vec::with_capacity(k);
    let mut s2: Vec<usize> = Vec::new();
    let mut remaining = k;
    while remaining > 0 {
        let (u, to_front) = if let Some(u) = sinks.pop_first() {
            (u, false)
        } else if let Some(u) = sources.pop_first() {
            (u, true)
        } else {
            let mut best = NONE;
            let mut best_delta = i64::MIN;
            for i in 0..k {
                fuel.burn(1)?;
                if !removed[i] {
                    let d = outdeg[i] as i64 - indeg[i] as i64;
                    if d > best_delta {
                        best_delta = d;
                        best = i;
                    }
                }
            }
            if best == NONE {
                break;
            }
            (best, true)
        };
        if removed[u] {
            continue;
        }
        removed[u] = true;
        remaining -= 1;
        sinks.remove(&u);
        sources.remove(&u);
        for &w in &outs[u] {
            fuel.burn(1)?;
            if !removed[w] {
                indeg[w] = indeg[w].saturating_sub(1);
                if indeg[w] == 0 {
                    sources.insert(w);
                }
            }
        }
        for &w in &ins[u] {
            fuel.burn(1)?;
            if !removed[w] {
                outdeg[w] = outdeg[w].saturating_sub(1);
                if outdeg[w] == 0 {
                    sinks.insert(w);
                }
            }
        }
        if to_front {
            s1.push(members[u]);
        } else {
            s2.push(members[u]);
        }
    }
    for &v in members {
        local[v] = NONE;
    }
    s2.reverse();
    s1.extend(s2);
    Ok(s1)
}

/// Phase 1 for a flowchart with `n` nodes. Self-loops must be excluded by the caller;
/// edges with an endpoint `>= n` are ignored.
pub fn remove_cycles(
    n: usize,
    edges: &[(usize, usize)],
    fuel: &mut Fuel,
) -> Result<CycleInfo, OutOfFuel> {
    let root = n;
    // Entries: the first declared node of every source component of the condensation
    // (which includes every node without incoming edges).
    let comp = scc(n, edges, fuel)?;
    let ncomp = comp
        .iter()
        .filter(|&&c| c != NONE)
        .max()
        .map_or(0, |&c| c + 1);
    let mut comp_has_in = vec![false; ncomp];
    let mut comp_first = vec![NONE; ncomp];
    for (v, &c) in comp.iter().enumerate() {
        if c != NONE && comp_first[c] == NONE {
            comp_first[c] = v;
        }
    }
    for &(u, v) in edges {
        if u < n && v < n && comp[u] != comp[v] {
            comp_has_in[comp[v]] = true;
        }
    }
    let mut entries: Vec<usize> = (0..ncomp)
        .filter(|&c| !comp_has_in[c])
        .map(|c| comp_first[c])
        .filter(|&v| v != NONE)
        .collect();
    entries.sort_unstable();

    // Successors and predecessors including the virtual root.
    let mut succ = out_lists(n, edges);
    succ.push(entries);
    let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
    for (u, list) in succ.iter().enumerate() {
        for &v in list {
            pred[v].push(u);
        }
    }

    // Postorder numbers of an iterative depth-first search from the root.
    let mut po = vec![NONE; n + 1];
    let mut rpo: Vec<usize> = Vec::with_capacity(n + 1);
    let mut visited = vec![false; n + 1];
    let mut call: Vec<(usize, usize)> = vec![(root, 0)];
    visited[root] = true;
    while let Some(top) = call.last_mut() {
        fuel.burn(1)?;
        let v = top.0;
        if let Some(&w) = succ[v].get(top.1) {
            top.1 += 1;
            if !visited[w] {
                visited[w] = true;
                call.push((w, 0));
            }
            continue;
        }
        call.pop();
        po[v] = rpo.len();
        rpo.push(v);
    }
    rpo.reverse();

    // Cooper–Harvey–Kennedy: iterate idom[b] = intersection of the processed
    // predecessors, in reverse postorder, until nothing changes.
    let mut idom = vec![NONE; n + 1];
    idom[root] = root;
    let mut changed = true;
    while changed {
        changed = false;
        for &b in rpo.iter().skip(1) {
            let mut new_idom = NONE;
            for &p in &pred[b] {
                fuel.burn(1)?;
                if idom[p] == NONE {
                    continue;
                }
                if new_idom == NONE {
                    new_idom = p;
                    continue;
                }
                let (mut f1, mut f2) = (p, new_idom);
                while f1 != f2 {
                    while po[f1] < po[f2] {
                        fuel.burn(1)?;
                        f1 = idom[f1];
                    }
                    while po[f2] < po[f1] {
                        fuel.burn(1)?;
                        f2 = idom[f2];
                    }
                }
                new_idom = f1;
            }
            if new_idom != NONE && idom[b] != new_idom {
                idom[b] = new_idom;
                changed = true;
            }
        }
    }
    // Every node is reachable by construction; an unreached node hangs off the root.
    for d in idom.iter_mut().take(n) {
        if *d == NONE {
            *d = root;
        }
    }

    // Dominator tree: children in declaration order, preorder numbers, subtree sizes, depth.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
    for (v, &d) in idom.iter().enumerate().take(n) {
        children[d].push(v);
    }
    let mut pre = vec![0usize; n + 1];
    let mut size = vec![1usize; n + 1];
    let mut depth = vec![0usize; n + 1];
    let mut order: Vec<usize> = Vec::with_capacity(n + 1);
    let mut stack = vec![root];
    while let Some(v) = stack.pop() {
        fuel.burn(1)?;
        pre[v] = order.len();
        order.push(v);
        for &c in children[v].iter().rev() {
            depth[c] = if v == root { 0 } else { depth[v] + 1 };
            stack.push(c);
        }
    }
    for &v in order.iter().rev() {
        if v != root {
            let p = idom[v];
            size[p] += size[v];
        }
    }
    let dominates = |a: usize, b: usize| pre[a] <= pre[b] && pre[b] < pre[a] + size[a];

    // Loop back-edges: the target dominates the source.
    let mut reversed: Vec<bool> = edges
        .iter()
        .map(|&(u, v)| u < n && v < n && dominates(v, u))
        .collect();

    // Remaining cycles: Eades–Lin–Smyth per strongly connected component.
    let oriented: Vec<(usize, usize)> = edges
        .iter()
        .zip(&reversed)
        .map(|(&(u, v), &r)| if r { (v, u) } else { (u, v) })
        .collect();
    let comp2 = scc(n, &oriented, fuel)?;
    let ncomp2 = comp2
        .iter()
        .filter(|&&c| c != NONE)
        .max()
        .map_or(0, |&c| c + 1);
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); ncomp2];
    for (v, &c) in comp2.iter().enumerate() {
        if c != NONE {
            members[c].push(v);
        }
    }
    let mut position = vec![NONE; n];
    let mut local = vec![NONE; n];
    for (c, m) in members.iter().enumerate() {
        if m.len() < 2 {
            continue;
        }
        let inner: Vec<(usize, usize)> = oriented
            .iter()
            .copied()
            .filter(|&(u, v)| u < n && v < n && comp2[u] == c && comp2[v] == c)
            .collect();
        let ord = els_order(m, &inner, &mut local, fuel)?;
        for (i, &v) in ord.iter().enumerate() {
            position[v] = i;
        }
        for (e, &(u, v)) in oriented.iter().enumerate() {
            fuel.burn(1)?;
            if u < n && v < n && comp2[u] == c && comp2[v] == c && position[u] > position[v] {
                reversed[e] = !reversed[e];
            }
        }
    }

    Ok(CycleInfo {
        reversed,
        idom: idom
            .iter()
            .take(n)
            .map(|&d| if d == root { None } else { Some(d) })
            .collect(),
        rank: depth.iter().take(n).map(|&d| d.min(15) as u8).collect(),
        dfs_key: pre.iter().take(n).map(|&p| p.saturating_sub(1)).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fuel() -> Fuel {
        Fuel::new(1_000_000)
    }

    fn is_acyclic(n: usize, edges: &[(usize, usize)], rev: &[bool]) -> bool {
        let mut indeg = vec![0usize; n];
        let oriented: Vec<(usize, usize)> = edges
            .iter()
            .zip(rev)
            .map(|(&(u, v), &r)| if r { (v, u) } else { (u, v) })
            .collect();
        for &(_, v) in &oriented {
            indeg[v] += 1;
        }
        let mut stack: Vec<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
        let mut seen = 0;
        while let Some(u) = stack.pop() {
            seen += 1;
            for &(a, b) in &oriented {
                if a == u {
                    indeg[b] -= 1;
                    if indeg[b] == 0 {
                        stack.push(b);
                    }
                }
            }
        }
        seen == n
    }

    #[test]
    fn scc_groups_cycles() {
        // 0 -> 1 -> 2 -> 0, 2 -> 3, 3 -> 4 -> 3
        let e = [(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 3)];
        let c = scc(5, &e, &mut fuel()).unwrap();
        assert_eq!(c[0], c[1]);
        assert_eq!(c[1], c[2]);
        assert_ne!(c[2], c[3]);
        assert_eq!(c[3], c[4]);
    }

    #[test]
    fn dag_has_no_back_edges() {
        let e = [(0, 1), (0, 2), (1, 3), (2, 3)];
        let info = remove_cycles(4, &e, &mut fuel()).unwrap();
        assert_eq!(info.reversed, vec![false; 4]);
    }

    #[test]
    fn loop_back_edge_is_the_edge_to_the_dominator() {
        // 0 -> 1 -> 2 -> 3, 3 -> 1 (loop), 2 -> 4
        let e = [(0, 1), (1, 2), (2, 3), (3, 1), (2, 4)];
        let info = remove_cycles(5, &e, &mut fuel()).unwrap();
        assert_eq!(info.reversed, vec![false, false, false, true, false]);
    }

    #[test]
    fn dominators_of_a_diamond() {
        // 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3, 3 -> 4
        let e = [(0, 1), (0, 2), (1, 3), (2, 3), (3, 4)];
        let info = remove_cycles(5, &e, &mut fuel()).unwrap();
        assert_eq!(info.idom, vec![None, Some(0), Some(0), Some(0), Some(3)]);
        assert_eq!(info.rank, vec![0, 1, 1, 1, 2]);
    }

    #[test]
    fn cycle_without_entry_uses_first_declared_node() {
        // 0 -> 1 -> 2 -> 0: node 0 is the entry, so 2 -> 0 is the back-edge.
        let e = [(0, 1), (1, 2), (2, 0)];
        let info = remove_cycles(3, &e, &mut fuel()).unwrap();
        assert_eq!(info.reversed, vec![false, false, true]);
        assert_eq!(info.idom[0], None);
        assert_eq!(info.idom[2], Some(1));
    }

    #[test]
    fn irreducible_cycle_is_broken_by_feedback_arc_set() {
        // 0 -> 1, 0 -> 2, 1 <-> 2: neither 1 nor 2 dominates the other.
        let e = [(0, 1), (0, 2), (1, 2), (2, 1)];
        let info = remove_cycles(3, &e, &mut fuel()).unwrap();
        assert!(is_acyclic(3, &e, &info.reversed));
        assert_eq!(info.reversed.iter().filter(|&&r| r).count(), 1);
        assert!(!info.reversed[0] && !info.reversed[1]);
    }

    #[test]
    fn dfs_key_keeps_dominated_regions_contiguous() {
        // 0 -> 1 -> 2, 0 -> 3; 1 dominates 2, so 2 follows 1 before 3.
        let e = [(0, 3), (0, 1), (1, 2)];
        let info = remove_cycles(4, &e, &mut fuel()).unwrap();
        assert!(info.dfs_key[0] < info.dfs_key[1]);
        assert!(info.dfs_key[1] < info.dfs_key[2]);
        assert!(info.dfs_key[2] < info.dfs_key[3]);
    }

    #[test]
    fn random_graphs_become_acyclic() {
        let mut s: u64 = 99;
        let mut next = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 33) as usize
        };
        for _ in 0..200 {
            let n = 1 + next() % 30;
            let m = next() % 80;
            let e: Vec<(usize, usize)> = (0..m)
                .map(|_| (next() % n, next() % n))
                .filter(|(a, b)| a != b)
                .collect();
            let info = remove_cycles(n, &e, &mut Fuel::new(10_000_000)).unwrap();
            assert!(is_acyclic(n, &e, &info.reversed));
        }
    }

    #[test]
    fn exhausted_fuel_is_reported() {
        let e: Vec<(usize, usize)> = (0..100).map(|i| (i, (i + 1) % 100)).collect();
        assert!(remove_cycles(100, &e, &mut Fuel::new(10)).is_err());
    }
}
