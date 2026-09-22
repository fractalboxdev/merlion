//! Phase 3, crossing minimisation (specs/layout.md#3-crossing-minimisation).
//!
//! - Pass 1 (mandatory, [`minimise`]): alternating down and up layer sweeps that order
//!   each layer by the weighted median of its neighbours' positions (Gansner et al.,
//!   TSE 1993), with the barycenter as tie-breaker, followed by greedy adjacent
//!   transposition. It stops after 24 sweeps or 4 sweeps without improvement and keeps
//!   the best order seen. Crossings are counted with the accumulator tree of Barth,
//!   Jünger and Mutzel (*Simple and Efficient Bilayer Cross Counting*, GD 2002).
//! - Pass 2 (optional, [`refine`]): exact one-sided crossing minimisation by branch and
//!   bound over the pairwise crossing matrix for layer pairs with at most
//!   [`EXACT_MAX_CROSSINGS`] crossings (Dujmović and Whitesides, Algorithmica 2004).
//! - Pass 3 (optional, [`refine`]): adjacent swaps that lower the largest number of
//!   crossings on a single edge without changing the total.
//!
//! Constraints kept by every pass:
//! - **Clusters.** Each cluster occupies a contiguous span of every layer, and sibling
//!   clusters appear in one global order in all layers (so the coordinate phase can
//!   treat each cluster box as a solid block). Layers are arranged hierarchically:
//!   the leaves of a cluster are sorted by key and merged with its child clusters, which
//!   are sorted by their global rank.
//! - **Stable layout.** Surviving nodes keep the hint's relative order, and at most
//!   `stability` new real nodes precede any survivor within its layer
//!   (specs/layout.md#stable-layout).

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Ordering;

use super::lgraph::{Clusters, Kind, LGraph};
use crate::fuel::{Fuel, OutOfFuel};

/// Pass 1 stops after this many sweeps.
pub const MAX_SWEEPS: usize = 24;
/// Pass 1 stops after this many sweeps without improvement.
pub const MAX_STALE_SWEEPS: usize = 4;
/// Pass 2 refines a layer pair only when it has at most this many crossings (`k`).
pub const EXACT_MAX_CROSSINGS: usize = 8;
/// Pass 2 permutes runs of at most this many nodes.
pub const EXACT_MAX_SEGMENT: usize = 12;
/// Greedy transposition rounds after each sweep.
const TRANSPOSE_ROUNDS: usize = 8;

/// Stable-layout constraint: `fixed[v]` is the hint rank of surviving node `v` within
/// its layer; new nodes and dummies are `None`.
pub struct Stable {
    pub fixed: Vec<Option<usize>>,
    pub stability: usize,
}

/// Draws fuel for mandatory or optional work.
enum Budget<'a> {
    Mandatory(&'a mut Fuel),
    Optional(&'a mut Fuel),
}

impl Budget<'_> {
    fn spend(&mut self, units: u64) -> Result<(), OutOfFuel> {
        match self {
            Budget::Mandatory(f) => f.burn(units),
            Budget::Optional(f) => f.burn_optional(units),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Key {
    /// Median (or initial key).
    m: f64,
    /// Barycenter.
    b: f64,
    /// Current position, the final tie-breaker.
    p: usize,
}

fn cmp_f(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

fn cmp_key(a: &Key, b: &Key) -> Ordering {
    cmp_f(a.m, b.m).then(cmp_f(a.b, b.b)).then(a.p.cmp(&b.p))
}

/// Crossings between the edges of `u` and of `v` (towards the same neighbour layer)
/// when `u` is left of `v`: pairs of neighbours `a` of `u`, `b` of `v` with `a` right of `b`.
fn pair_crossings(
    a: &[usize],
    b: &[usize],
    pos: &[usize],
    budget: &mut Budget,
) -> Result<usize, OutOfFuel> {
    budget.spend((a.len() * b.len()) as u64 + 1)?;
    let mut c = 0;
    for &x in a {
        for &y in b {
            if pos.get(x) > pos.get(y) {
                c += 1;
            }
        }
    }
    Ok(c)
}

fn bilayer(g: &LGraph, pos: &[usize], l: usize, budget: &mut Budget) -> Result<usize, OutOfFuel> {
    let lower_len = g.layers.get(l + 1).map_or(0, Vec::len);
    let Some(upper) = g.layers.get(l) else {
        return Ok(0);
    };
    if lower_len == 0 {
        return Ok(0);
    }
    let mut first = 1usize;
    while first < lower_len {
        first *= 2;
    }
    let mut tree = vec![0usize; 2 * first - 1];
    first -= 1;
    let mut cross = 0usize;
    let mut ps: Vec<usize> = Vec::new();
    for &u in upper {
        ps.clear();
        ps.extend(g.down[u].iter().filter_map(|&w| pos.get(w).copied()));
        ps.sort_unstable();
        for &p in &ps {
            if p >= lower_len {
                continue;
            }
            let mut idx = p + first;
            tree[idx] += 1;
            while idx > 0 {
                budget.spend(1)?;
                if idx % 2 == 1 {
                    cross += tree[idx + 1];
                }
                idx = (idx - 1) / 2;
                tree[idx] += 1;
            }
        }
    }
    Ok(cross)
}

/// Crossings between layer `l` and `l + 1` (Barth–Jünger–Mutzel accumulator tree).
#[cfg(test)]
pub fn bilayer_crossings(
    g: &LGraph,
    pos: &[usize],
    l: usize,
    fuel: &mut Fuel,
) -> Result<usize, OutOfFuel> {
    bilayer(g, pos, l, &mut Budget::Mandatory(fuel))
}

fn total(g: &LGraph, pos: &[usize], budget: &mut Budget) -> Result<usize, OutOfFuel> {
    let mut sum = 0usize;
    for l in 0..g.layers.len().saturating_sub(1) {
        sum = sum.saturating_add(bilayer(g, pos, l, budget)?);
    }
    Ok(sum)
}

/// Total crossings of the current order.
#[cfg(test)]
pub fn total_crossings(g: &LGraph, fuel: &mut Fuel) -> Result<usize, OutOfFuel> {
    total(g, &g.positions(), &mut Budget::Mandatory(fuel))
}

struct Search<'a> {
    cost: &'a [Vec<usize>],
    fixed: &'a [Option<usize>],
    best: usize,
    best_perm: Vec<usize>,
    placed: Vec<usize>,
}

impl Search<'_> {
    /// Depth-first branch and bound; depth is at most the segment size (≤ 16).
    fn go(&mut self, rem: u32, cost: usize, lb: usize, fuel: &mut Fuel) -> Result<(), OutOfFuel> {
        fuel.burn_optional(1)?;
        let k = self.cost.len();
        if rem == 0 {
            if cost < self.best {
                self.best = cost;
                self.best_perm = self.placed.clone();
            }
            return Ok(());
        }
        for r in 0..k {
            if rem & (1 << r) == 0 {
                continue;
            }
            // Fixed nodes enter in rank order.
            if let Some(fr) = self.fixed[r] {
                let blocked = (0..k).any(|q| {
                    q != r && rem & (1 << q) != 0 && self.fixed[q].is_some_and(|fq| fq < fr)
                });
                if blocked {
                    continue;
                }
            }
            let mut inc = 0usize;
            let mut lb_drop = 0usize;
            for o in 0..k {
                if o != r && rem & (1 << o) != 0 {
                    inc += self.cost[r][o];
                    lb_drop += self.cost[r][o].min(self.cost[o][r]);
                }
            }
            let lb_new = lb - lb_drop;
            if cost + inc + lb_new >= self.best {
                continue;
            }
            self.placed.push(r);
            let res = self.go(rem & !(1 << r), cost + inc, lb_new, fuel);
            self.placed.pop();
            res?;
        }
        Ok(())
    }
}

/// Exact minimum of `Σ cost[p_i][p_j]` over permutations `p` (i < j) in which the
/// entries with `fixed = Some(rank)` appear in rank order. Branch and bound with the
/// lower bound `Σ min(cost[a][b], cost[b][a])` over unordered pairs not yet placed;
/// one optional fuel unit per search node. `None` when fuel runs out or there are more
/// than 16 entries. On ties the input order wins.
pub fn best_permutation(
    cost: &[Vec<usize>],
    fixed: &[Option<usize>],
    fuel: &mut Fuel,
) -> Option<(Vec<usize>, usize)> {
    let k = cost.len();
    if k > 16 || fixed.len() != k || cost.iter().any(|row| row.len() != k) {
        return None;
    }
    // Feasible start: input order with the fixed entries sorted into their own slots.
    let mut start: Vec<usize> = (0..k).collect();
    let mut slots: Vec<usize> = (0..k).filter(|&i| fixed[i].is_some()).collect();
    let mut by_rank = slots.clone();
    by_rank.sort_by_key(|&i| (fixed[i], i));
    for (slot, v) in slots.drain(..).zip(by_rank) {
        start[slot] = v;
    }
    let eval = |p: &[usize]| {
        let mut s = 0usize;
        for i in 0..p.len() {
            for j in i + 1..p.len() {
                s += cost[p[i]][p[j]];
            }
        }
        s
    };
    let start_cost = eval(&start);
    let mut lb = 0usize;
    for (a, row) in cost.iter().enumerate() {
        for (b, &ab) in row.iter().enumerate().skip(a + 1) {
            lb += ab.min(cost[b][a]);
        }
    }
    let mut s = Search {
        cost,
        fixed,
        best: start_cost,
        best_perm: start,
        placed: Vec::with_capacity(k),
    };
    let rem = if k == 0 { 0 } else { u32::MAX >> (32 - k) };
    s.go(rem, 0, lb, fuel).ok()?;
    Some((s.best_perm, s.best))
}

struct Ctx {
    /// Cluster chain (outermost first) of every node.
    chains: Vec<Vec<usize>>,
    fixed: Vec<Option<usize>>,
    new_real: Vec<bool>,
    stability: usize,
    /// Global order of clusters among their siblings.
    rank: Vec<f64>,
}

impl Ctx {
    fn new(g: &LGraph, cl: &Clusters, stable: Option<&Stable>) -> Self {
        let n = g.nodes.len();
        let fixed: Vec<Option<usize>> = (0..n)
            .map(|v| stable.and_then(|s| s.fixed.get(v).copied().flatten()))
            .collect();
        let new_real = (0..n)
            .map(|v| {
                stable.is_some() && fixed[v].is_none() && matches!(g.nodes[v].kind, Kind::Real(_))
            })
            .collect();
        Ctx {
            chains: g.nodes.iter().map(|v| cl.chain(v.cluster)).collect(),
            fixed,
            new_real,
            stability: stable.map_or(usize::MAX, |s| s.stability),
            rank: vec![0.0; cl.len()],
        }
    }

    fn set_ranks(&mut self, g: &LGraph, value: impl Fn(usize, usize, usize) -> f64) {
        let k = self.rank.len();
        let mut sum = vec![0.0; k];
        let mut cnt = vec![0usize; k];
        for layer in &g.layers {
            for (i, &v) in layer.iter().enumerate() {
                let x = value(v, i, layer.len());
                for &c in &self.chains[v] {
                    sum[c] += x;
                    cnt[c] += 1;
                }
            }
        }
        for c in 0..k {
            if cnt[c] > 0 {
                self.rank[c] = sum[c] / cnt[c] as f64;
            }
        }
    }

    /// Hierarchical arrangement of `items` (all inside the cluster at `level - 1` of
    /// their chains). Recursion depth is bounded by the cluster depth (≤ 65).
    fn arrange(&self, items: Vec<usize>, level: usize, key: &[Key], out: &mut Vec<usize>) {
        let mut surv: Vec<usize> = Vec::new();
        let mut free: Vec<usize> = Vec::new();
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for v in items {
            match self.chains[v].get(level) {
                Some(&c) if level <= super::lgraph::MAX_CLUSTER_DEPTH => {
                    groups.entry(c).or_default().push(v)
                }
                _ => {
                    if self.fixed[v].is_some() {
                        surv.push(v)
                    } else {
                        free.push(v)
                    }
                }
            }
        }
        surv.sort_by_key(|&v| (self.fixed[v], v));
        free.sort_by(|&a, &b| cmp_key(&key[a], &key[b]).then(a.cmp(&b)));
        // Leaves: survivors in hint order, free nodes merged in by key.
        let mut leaves: Vec<usize> = Vec::with_capacity(surv.len() + free.len());
        let (mut i, mut j) = (0, 0);
        while i < surv.len() || j < free.len() {
            let take_free = match (surv.get(i), free.get(j)) {
                (Some(&s), Some(&f)) => cmp_key(&key[f], &key[s]) == Ordering::Less,
                (None, Some(_)) => true,
                _ => false,
            };
            if take_free {
                leaves.push(free[j]);
                j += 1;
            } else {
                leaves.push(surv[i]);
                i += 1;
            }
        }
        // Child clusters in global rank order, each keyed by its members' mean key.
        let mut blocks: Vec<(usize, Key, Vec<usize>)> = groups
            .into_iter()
            .map(|(c, vs)| {
                let n = vs.len().max(1) as f64;
                let m = vs.iter().map(|&v| key[v].m).sum::<f64>() / n;
                let b = vs.iter().map(|&v| key[v].b).sum::<f64>() / n;
                let p = vs.iter().map(|&v| key[v].p).min().unwrap_or(0);
                (c, Key { m, b, p }, vs)
            })
            .collect();
        blocks.sort_by(|a, b| {
            cmp_f(
                self.rank.get(a.0).copied().unwrap_or(0.0),
                self.rank.get(b.0).copied().unwrap_or(0.0),
            )
            .then(a.0.cmp(&b.0))
        });
        let mut li = 0;
        let mut blocks = blocks.into_iter().peekable();
        loop {
            let take_leaf = match (leaves.get(li), blocks.peek()) {
                (Some(&v), Some(blk)) => cmp_key(&key[v], &blk.1) != Ordering::Greater,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if take_leaf {
                out.push(leaves[li]);
                li += 1;
            } else if let Some((_, _, vs)) = blocks.next() {
                self.arrange(vs, level + 1, key, out);
            }
        }
    }

    fn arrange_layer(&self, g: &mut LGraph, pos: &mut [usize], l: usize, key: &[Key]) {
        let items = core::mem::take(&mut g.layers[l]);
        let mut out = Vec::with_capacity(items.len());
        self.arrange(items, 0, key, &mut out);
        g.layers[l] = out;
        self.repair(g, l);
        for (i, &v) in g.layers[l].iter().enumerate() {
            pos[v] = i;
        }
    }

    fn new_before(&self, layer: &[usize], idx: usize) -> usize {
        layer
            .iter()
            .take(idx)
            .filter(|&&v| self.new_real[v])
            .count()
    }

    fn stability_ok(&self, layer: &[usize]) -> bool {
        let mut new = 0usize;
        for &v in layer {
            if self.new_real[v] {
                new += 1;
            } else if self.fixed[v].is_some() && new > self.stability {
                return false;
            }
        }
        true
    }

    /// Moves new real nodes from before a survivor to just after it while more than
    /// `stability` of them precede it. Only nodes of the survivor's own cluster move,
    /// which keeps every cluster contiguous.
    fn repair(&self, g: &mut LGraph, l: usize) {
        if self.stability == usize::MAX {
            return;
        }
        let cluster: Vec<Option<usize>> = g.nodes.iter().map(|v| v.cluster).collect();
        let layer = &mut g.layers[l];
        let mut i = 0;
        while i < layer.len() {
            let s = layer[i];
            if self.fixed[s].is_some() {
                let mut nb = self.new_before(layer, i);
                while nb > self.stability {
                    let Some(x) = (0..i)
                        .rev()
                        .find(|&j| self.new_real[layer[j]] && cluster[layer[j]] == cluster[s])
                    else {
                        break;
                    };
                    let node = layer.remove(x);
                    // The survivor shifted left by one; insert right after it.
                    layer.insert(i, node);
                    i -= 1;
                    nb -= 1;
                }
            }
            i += 1;
        }
    }

    fn allowed(&self, g: &LGraph, layer: &[usize], i: usize) -> bool {
        let (Some(&u), Some(&v)) = (layer.get(i), layer.get(i + 1)) else {
            return false;
        };
        if g.nodes[u].cluster != g.nodes[v].cluster {
            return false;
        }
        if self.fixed[u].is_some() && self.fixed[v].is_some() {
            return false;
        }
        if self.fixed[u].is_some()
            && self.new_real[v]
            && self.new_before(layer, i) + 1 > self.stability
        {
            return false;
        }
        true
    }

    fn swap_gain(
        &self,
        g: &LGraph,
        pos: &[usize],
        u: usize,
        v: usize,
        budget: &mut Budget,
    ) -> Result<(usize, usize), OutOfFuel> {
        let uv = pair_crossings(&g.up[u], &g.up[v], pos, budget)?
            + pair_crossings(&g.down[u], &g.down[v], pos, budget)?;
        let vu = pair_crossings(&g.up[v], &g.up[u], pos, budget)?
            + pair_crossings(&g.down[v], &g.down[u], pos, budget)?;
        Ok((uv, vu))
    }

    fn transpose_layer(
        &self,
        g: &mut LGraph,
        pos: &mut [usize],
        l: usize,
        budget: &mut Budget,
    ) -> Result<bool, OutOfFuel> {
        let mut improved = false;
        let len = g.layers[l].len();
        for i in 0..len.saturating_sub(1) {
            if !self.allowed(g, &g.layers[l], i) {
                continue;
            }
            let (u, v) = (g.layers[l][i], g.layers[l][i + 1]);
            let (uv, vu) = self.swap_gain(g, pos, u, v, budget)?;
            if vu < uv {
                g.layers[l].swap(i, i + 1);
                pos[u] = i + 1;
                pos[v] = i;
                improved = true;
            }
        }
        Ok(improved)
    }

    /// Median / barycenter keys of layer `l` from the neighbouring layer (above when
    /// `from_up`). Nodes without neighbours there keep their relative position.
    fn sweep_keys(
        &self,
        g: &LGraph,
        pos: &[usize],
        l: usize,
        from_up: bool,
        key: &mut [Key],
        fuel: &mut Fuel,
    ) -> Result<(), OutOfFuel> {
        let this_len = g.layers[l].len().max(1) as f64;
        let other = if from_up {
            l.checked_sub(1)
        } else {
            Some(l + 1)
        };
        let other_len = other
            .and_then(|o| g.layers.get(o))
            .map_or(1, Vec::len)
            .max(1) as f64;
        let mut ps: Vec<f64> = Vec::new();
        for &v in &g.layers[l] {
            let nbrs = if from_up { &g.up[v] } else { &g.down[v] };
            fuel.burn(nbrs.len() as u64 + 1)?;
            ps.clear();
            ps.extend(nbrs.iter().map(|&w| pos[w] as f64));
            ps.sort_by(|a, b| cmp_f(*a, *b));
            let (m, b) = if ps.is_empty() {
                let x = (pos[v] as f64 + 0.5) * other_len / this_len - 0.5;
                (x, x)
            } else {
                (
                    weighted_median(&ps),
                    ps.iter().sum::<f64>() / ps.len() as f64,
                )
            };
            key[v] = Key { m, b, p: pos[v] };
        }
        Ok(())
    }
}

/// Weighted median of sorted positions (Gansner et al. 1993, §3): the middle value for
/// odd counts, and for even counts the two middle values weighted towards the side
/// where the neighbours are packed more tightly.
fn weighted_median(p: &[f64]) -> f64 {
    let n = p.len();
    let m = n / 2;
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        return p[m];
    }
    if n == 2 {
        return (p[0] + p[1]) / 2.0;
    }
    let left = p[m - 1] - p[0];
    let right = p[n - 1] - p[m];
    if left + right == 0.0 {
        (p[m - 1] + p[m]) / 2.0
    } else {
        (p[m - 1] * right + p[m] * left) / (left + right)
    }
}

/// Initial order: each layer sorted by `keys` (the dominator-tree preorder), subject to
/// the cluster and stability constraints.
pub fn initial_order(g: &mut LGraph, cl: &Clusters, keys: &[f64], stable: Option<&Stable>) {
    let mut ctx = Ctx::new(g, cl, stable);
    let key: Vec<Key> = (0..g.nodes.len())
        .map(|v| {
            let k = keys.get(v).copied().unwrap_or(v as f64);
            Key { m: k, b: k, p: v }
        })
        .collect();
    ctx.set_ranks(g, |v, _, _| key[v].m);
    let mut pos = g.positions();
    for l in 0..g.layers.len() {
        ctx.arrange_layer(g, &mut pos, l, &key);
    }
}

/// Pass 1: layer sweeps and transposition (mandatory; fuel exhaustion is an error).
pub fn minimise(
    g: &mut LGraph,
    cl: &Clusters,
    stable: Option<&Stable>,
    fuel: &mut Fuel,
) -> Result<(), OutOfFuel> {
    let mut ctx = Ctx::new(g, cl, stable);
    ctx.set_ranks(g, |_, i, len| (i as f64 + 0.5) / len as f64);
    let mut pos = g.positions();
    let mut best = total(g, &pos, &mut Budget::Mandatory(fuel))?;
    let mut best_layers = g.layers.clone();
    if best == 0 {
        return Ok(());
    }
    let nl = g.layers.len();
    let mut key = vec![Key::default(); g.nodes.len()];
    let mut stale = 0;
    for sweep in 0..MAX_SWEEPS {
        if sweep % 2 == 0 {
            for l in 1..nl {
                ctx.sweep_keys(g, &pos, l, true, &mut key, fuel)?;
                ctx.arrange_layer(g, &mut pos, l, &key);
            }
        } else {
            for l in (0..nl.saturating_sub(1)).rev() {
                ctx.sweep_keys(g, &pos, l, false, &mut key, fuel)?;
                ctx.arrange_layer(g, &mut pos, l, &key);
            }
        }
        for _ in 0..TRANSPOSE_ROUNDS {
            let mut improved = false;
            for l in 0..nl {
                improved |= ctx.transpose_layer(g, &mut pos, l, &mut Budget::Mandatory(fuel))?;
            }
            if !improved {
                break;
            }
        }
        ctx.set_ranks(g, |_, i, len| (i as f64 + 0.5) / len as f64);
        let c = total(g, &pos, &mut Budget::Mandatory(fuel))?;
        if c < best {
            best = c;
            best_layers.clone_from(&g.layers);
            stale = 0;
        } else {
            stale += 1;
        }
        if best == 0 || stale >= MAX_STALE_SWEEPS {
            break;
        }
    }
    g.layers = best_layers;
    Ok(())
}

/// Largest number of crossings on one segment between layers `l` and `l + 1`.
fn max_edge_crossings(
    g: &LGraph,
    pos: &[usize],
    l: usize,
    fuel: &mut Fuel,
) -> Result<usize, OutOfFuel> {
    let Some(upper) = g.layers.get(l) else {
        return Ok(0);
    };
    let segs: Vec<(usize, usize)> = upper
        .iter()
        .flat_map(|&u| g.down[u].iter().map(move |&w| (u, w)))
        .map(|(u, w)| (pos[u], pos[w]))
        .collect();
    fuel.burn_optional((segs.len() * segs.len()) as u64 + 1)?;
    let mut best = 0;
    for &(a, b) in &segs {
        let c = segs
            .iter()
            .filter(|&&(x, y)| (x < a && y > b) || (x > a && y < b))
            .count();
        best = best.max(c);
    }
    Ok(best)
}

/// Passes 2 and 3 (optional). Every change is kept only if it lowers its objective, and
/// each pass stops as soon as optional fuel runs out, keeping what it has.
pub fn refine(g: &mut LGraph, cl: &Clusters, stable: Option<&Stable>, fuel: &mut Fuel) {
    let ctx = Ctx::new(g, cl, stable);
    let mut pos = g.positions();
    let _ = exact_pass(&ctx, g, &mut pos, fuel);
    let _ = local_pass(&ctx, g, &mut pos, fuel);
}

fn local_total(g: &LGraph, pos: &[usize], l: usize, fuel: &mut Fuel) -> Result<usize, OutOfFuel> {
    let mut b = Budget::Optional(fuel);
    let above = match l.checked_sub(1) {
        Some(p) => bilayer(g, pos, p, &mut b)?,
        None => 0,
    };
    Ok(above + bilayer(g, pos, l, &mut b)?)
}

fn exact_pass(
    ctx: &Ctx,
    g: &mut LGraph,
    pos: &mut [usize],
    fuel: &mut Fuel,
) -> Result<(), OutOfFuel> {
    let nl = g.layers.len();
    for l in 0..nl.saturating_sub(1) {
        for (free, from_up) in [(l + 1, true), (l, false)] {
            let cr = bilayer(g, pos, l, &mut Budget::Optional(fuel))?;
            if cr == 0 || cr > EXACT_MAX_CROSSINGS {
                continue;
            }
            // Maximal runs of adjacent nodes with the same innermost cluster.
            let layer = g.layers[free].clone();
            let mut start = 0;
            while start < layer.len() {
                let mut end = start + 1;
                while end < layer.len()
                    && g.nodes[layer[end]].cluster == g.nodes[layer[start]].cluster
                {
                    end += 1;
                }
                if (2..=EXACT_MAX_SEGMENT).contains(&(end - start)) {
                    refine_segment(ctx, g, pos, free, start, end, from_up, fuel)?;
                }
                start = end;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn refine_segment(
    ctx: &Ctx,
    g: &mut LGraph,
    pos: &mut [usize],
    l: usize,
    start: usize,
    end: usize,
    from_up: bool,
    fuel: &mut Fuel,
) -> Result<(), OutOfFuel> {
    let seg: Vec<usize> = g.layers[l][start..end].to_vec();
    let k = seg.len();
    let mut cost = vec![vec![0usize; k]; k];
    {
        let mut b = Budget::Optional(fuel);
        for i in 0..k {
            for j in 0..k {
                if i != j {
                    let (a, c) = if from_up {
                        (&g.up[seg[i]], &g.up[seg[j]])
                    } else {
                        (&g.down[seg[i]], &g.down[seg[j]])
                    };
                    cost[i][j] = pair_crossings(a, c, pos, &mut b)?;
                }
            }
        }
    }
    let fixed: Vec<Option<usize>> = seg.iter().map(|&v| ctx.fixed[v]).collect();
    let Some((perm, c)) = best_permutation(&cost, &fixed, fuel) else {
        return Err(OutOfFuel);
    };
    let current: usize = (0..k)
        .flat_map(|i| (i + 1..k).map(move |j| (i, j)))
        .map(|(i, j)| cost[i][j])
        .sum();
    if c >= current {
        return Ok(());
    }
    let before = local_total(g, pos, l, fuel)?;
    let old = g.layers[l].clone();
    for (i, &p) in perm.iter().enumerate() {
        g.layers[l][start + i] = seg[p];
        pos[seg[p]] = start + i;
    }
    let after = local_total(g, pos, l, fuel);
    let keep = matches!(after, Ok(a) if a < before) && ctx.stability_ok(&g.layers[l]);
    if !keep {
        g.layers[l] = old;
        for (i, &v) in g.layers[l].iter().enumerate() {
            pos[v] = i;
        }
    }
    after.map(|_| ())
}

fn local_pass(
    ctx: &Ctx,
    g: &mut LGraph,
    pos: &mut [usize],
    fuel: &mut Fuel,
) -> Result<(), OutOfFuel> {
    let nl = g.layers.len();
    let local_max =
        |g: &LGraph, pos: &[usize], l: usize, fuel: &mut Fuel| -> Result<usize, OutOfFuel> {
            let above = match l.checked_sub(1) {
                Some(p) => max_edge_crossings(g, pos, p, fuel)?,
                None => 0,
            };
            Ok(above.max(max_edge_crossings(g, pos, l, fuel)?))
        };
    for l in 0..nl {
        for i in 0..g.layers[l].len().saturating_sub(1) {
            if !ctx.allowed(g, &g.layers[l], i) {
                continue;
            }
            let (u, v) = (g.layers[l][i], g.layers[l][i + 1]);
            let (uv, vu) = ctx.swap_gain(g, pos, u, v, &mut Budget::Optional(fuel))?;
            if uv != vu || uv == 0 {
                continue;
            }
            let before = local_max(g, pos, l, fuel)?;
            g.layers[l].swap(i, i + 1);
            pos[u] = i + 1;
            pos[v] = i;
            let after = local_max(g, pos, l, fuel);
            if !matches!(after, Ok(a) if a < before) {
                g.layers[l].swap(i, i + 1);
                pos[u] = i;
                pos[v] = i + 1;
            }
            after?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::lgraph::{build, BuildIn, EdgeIn, Extent, Kind};
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

    fn graph(layers: &[usize], edges: &[(usize, usize)], cl: &Clusters) -> LGraph {
        let real: Vec<Extent> = layers
            .iter()
            .map(|_| Extent {
                left: 10.0,
                right: 10.0,
                thick: 10.0,
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
        let titles = vec![Extent::default(); cl.len()];
        build(&BuildIn {
            real: &real,
            layer: layers,
            edges: &e,
            clusters: cl,
            titles: &titles,
            empty_size: &titles,
            max_nodes: 100_000,
            max_layers: 1000,
        })
        .unwrap()
    }

    fn brute_bilayer(g: &LGraph, pos: &[usize], l: usize) -> usize {
        let mut segs = Vec::new();
        for &u in &g.layers[l] {
            for &w in &g.down[u] {
                segs.push((pos[u], pos[w]));
            }
        }
        let mut c = 0;
        for i in 0..segs.len() {
            for j in i + 1..segs.len() {
                let (a, b) = (segs[i], segs[j]);
                if (a.0 < b.0 && a.1 > b.1) || (a.0 > b.0 && a.1 < b.1) {
                    c += 1;
                }
            }
        }
        c
    }

    fn fuel() -> Fuel {
        Fuel::new(50_000_000)
    }

    #[test]
    fn bilayer_count_matches_brute_force() {
        let mut r = Lcg(3);
        let cl = Clusters::default();
        for _ in 0..50 {
            let a = 1 + r.next() % 8;
            let b = 1 + r.next() % 8;
            let mut layers = vec![0; a];
            layers.extend(vec![1; b]);
            let edges: Vec<(usize, usize)> = (0..r.next() % 20)
                .map(|_| (r.next() % a, a + r.next() % b))
                .collect();
            let mut g = graph(&layers, &edges, &cl);
            // Shuffle the lower layer.
            let l1 = &mut g.layers[1];
            for i in (1..l1.len()).rev() {
                l1.swap(i, r.next() % (i + 1));
            }
            let pos = g.positions();
            assert_eq!(
                bilayer_crossings(&g, &pos, 0, &mut fuel()).unwrap(),
                brute_bilayer(&g, &pos, 0)
            );
        }
    }

    #[test]
    fn sweeps_remove_avoidable_crossings() {
        // a->d, b->c with order [a, b] / [c, d]: one crossing, optimum 0.
        let cl = Clusters::default();
        let mut g = graph(&[0, 0, 1, 1], &[(0, 3), (1, 2)], &cl);
        assert_eq!(total_crossings(&g, &mut fuel()).unwrap(), 1);
        minimise(&mut g, &cl, None, &mut fuel()).unwrap();
        assert_eq!(total_crossings(&g, &mut fuel()).unwrap(), 0);
    }

    #[test]
    fn k22_keeps_its_one_unavoidable_crossing() {
        let cl = Clusters::default();
        let mut g = graph(&[0, 0, 1, 1], &[(0, 2), (0, 3), (1, 2), (1, 3)], &cl);
        minimise(&mut g, &cl, None, &mut fuel()).unwrap();
        assert_eq!(total_crossings(&g, &mut fuel()).unwrap(), 1);
    }

    #[test]
    fn three_layer_crafted_instance_reaches_zero() {
        // Two interleaved chains and a fan that are planar when untangled.
        let layers = [0, 0, 0, 1, 1, 1, 2, 2, 2];
        let edges = [(0, 5), (1, 4), (2, 3), (3, 8), (4, 7), (5, 6), (0, 4)];
        let cl = Clusters::default();
        let mut g = graph(&layers, &edges, &cl);
        assert!(total_crossings(&g, &mut fuel()).unwrap() > 0);
        minimise(&mut g, &cl, None, &mut fuel()).unwrap();
        refine(&mut g, &cl, None, &mut fuel());
        assert_eq!(total_crossings(&g, &mut fuel()).unwrap(), 0);
    }

    #[test]
    fn exact_permutation_matches_brute_force() {
        let mut r = Lcg(11);
        for _ in 0..40 {
            let k = 2 + r.next() % 5;
            let cost: Vec<Vec<usize>> = (0..k)
                .map(|i| {
                    (0..k)
                        .map(|j| if i == j { 0 } else { r.next() % 4 })
                        .collect()
                })
                .collect();
            let (perm, c) = best_permutation(&cost, &vec![None; k], &mut fuel()).unwrap();
            let eval = |p: &[usize]| {
                let mut s = 0;
                for i in 0..p.len() {
                    for j in i + 1..p.len() {
                        s += cost[p[i]][p[j]];
                    }
                }
                s
            };
            assert_eq!(eval(&perm), c);
            // Brute force over all permutations (Heap's algorithm, iterative).
            let mut p: Vec<usize> = (0..k).collect();
            let mut best = eval(&p);
            let mut cs = vec![0; k];
            let mut i = 0;
            while i < k {
                if cs[i] < i {
                    if i % 2 == 0 {
                        p.swap(0, i)
                    } else {
                        p.swap(cs[i], i)
                    }
                    best = best.min(eval(&p));
                    cs[i] += 1;
                    i = 0;
                } else {
                    cs[i] = 0;
                    i += 1;
                }
            }
            assert_eq!(c, best);
        }
    }

    #[test]
    fn exact_permutation_keeps_fixed_nodes_in_order() {
        // Cost prefers reversing everything, but 0 and 2 are fixed in that order.
        let cost = vec![vec![0, 5, 5], vec![0, 0, 5], vec![0, 0, 0]];
        let (perm, _) = best_permutation(&cost, &[Some(0), None, Some(1)], &mut fuel()).unwrap();
        let p0 = perm.iter().position(|&x| x == 0).unwrap();
        let p2 = perm.iter().position(|&x| x == 2).unwrap();
        assert!(p0 < p2);
    }

    #[test]
    fn exact_permutation_gives_up_on_tiny_fuel() {
        let cost: Vec<Vec<usize>> = (0..8)
            .map(|i| (0..8).map(|j| (i * 7 + j * 3) % 5).collect())
            .collect();
        assert!(best_permutation(&cost, &[None; 8], &mut Fuel::new(3)).is_none());
    }

    fn chart_clusters(parents: &[Option<usize>], node_sub: &[Option<usize>]) -> Clusters {
        let mut c = Flowchart::default();
        for (i, p) in parents.iter().enumerate() {
            c.subgraphs.push(Subgraph {
                id: alloc::format!("s{}", i),
                title: String::new(),
                parent: *p,
                nodes: vec![],
                direction: None,
                classes: Vec::new(),
                style: Default::default(),
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
                reserve: Default::default(),
            });
        }
        Clusters::from_chart(&c)
    }

    fn assert_contiguous(g: &LGraph, cl: &Clusters) {
        for layer in &g.layers {
            for c in 0..cl.len() {
                let idx: Vec<usize> = layer
                    .iter()
                    .enumerate()
                    .filter(|(_, &v)| cl.within(g.nodes[v].cluster, c))
                    .map(|(i, _)| i)
                    .collect();
                if let (Some(&a), Some(&b)) = (idx.first(), idx.last()) {
                    assert_eq!(b - a + 1, idx.len(), "cluster {} split in {:?}", c, layer);
                }
            }
        }
    }

    #[test]
    fn clusters_stay_contiguous_and_sibling_order_is_consistent() {
        let mut r = Lcg(21);
        for _ in 0..30 {
            let n = 6 + r.next() % 20;
            let parents = [None, Some(0), None, Some(2)];
            let subs: Vec<Option<usize>> = (0..n)
                .map(|_| match r.next() % 6 {
                    0 => None,
                    1 => Some(0),
                    2 => Some(1),
                    3 => Some(2),
                    _ => Some(3),
                })
                .collect();
            let cl = chart_clusters(&parents, &subs);
            let layers: Vec<usize> = (0..n).map(|_| r.next() % 4).collect();
            let edges: Vec<(usize, usize)> = (0..n * 2)
                .map(|_| (r.next() % n, r.next() % n))
                .filter(|&(a, b)| layers[a] < layers[b])
                .collect();
            let mut g = graph(&layers, &edges, &cl);
            let keys: Vec<f64> = (0..g.nodes.len()).map(|i| i as f64).collect();
            initial_order(&mut g, &cl, &keys, None);
            assert_contiguous(&g, &cl);
            minimise(&mut g, &cl, None, &mut fuel()).unwrap();
            refine(&mut g, &cl, None, &mut fuel());
            assert_contiguous(&g, &cl);
            // Sibling clusters 0 and 2 appear in the same relative order in every layer.
            let mut seen: Option<bool> = None;
            for layer in &g.layers {
                let f0 = layer.iter().position(|&v| cl.within(g.nodes[v].cluster, 0));
                let f2 = layer.iter().position(|&v| cl.within(g.nodes[v].cluster, 2));
                if let (Some(a), Some(b)) = (f0, f2) {
                    let o = a < b;
                    assert!(seen.is_none() || seen == Some(o));
                    seen = Some(o);
                }
            }
        }
    }

    #[test]
    fn survivors_keep_order_and_displacement_limit() {
        // Layer 1 holds survivors 1..=4 in hint order and new nodes 5..=8 that all
        // attach to the leftmost parent, which pulls them to the front.
        let layers = [0, 1, 1, 1, 1, 1, 1, 1, 1];
        let mut edges = vec![(0, 1), (0, 2), (0, 3), (0, 4)];
        for v in 5..9 {
            edges.push((0, v));
        }
        let cl = Clusters::default();
        let mut g = graph(&layers, &edges, &cl);
        let mut fixed = vec![None; g.nodes.len()];
        for (rank, v) in [4usize, 3, 2, 1].iter().enumerate() {
            fixed[*v] = Some(rank);
        }
        let stable = Stable {
            fixed,
            stability: 2,
        };
        let keys: Vec<f64> = (0..g.nodes.len()).map(|i| (10 - i) as f64).collect();
        initial_order(&mut g, &cl, &keys, Some(&stable));
        minimise(&mut g, &cl, Some(&stable), &mut fuel()).unwrap();
        refine(&mut g, &cl, Some(&stable), &mut fuel());
        let layer = &g.layers[1];
        let surv: Vec<usize> = layer
            .iter()
            .copied()
            .filter(|&v| (1..5).contains(&v))
            .collect();
        assert_eq!(surv, vec![4, 3, 2, 1]);
        for (rank, &s) in surv.iter().enumerate() {
            let idx = layer.iter().position(|&v| v == s).unwrap();
            assert!(idx <= rank + 2, "survivor {} at {}", s, idx);
        }
    }

    #[test]
    fn minimise_is_deterministic_and_bounded_on_random_graphs() {
        let mut r = Lcg(77);
        let cl = Clusters::default();
        for _ in 0..20 {
            let n = 10 + r.next() % 40;
            let layers: Vec<usize> = (0..n).map(|_| r.next() % 6).collect();
            let edges: Vec<(usize, usize)> = (0..n * 2)
                .map(|_| (r.next() % n, r.next() % n))
                .filter(|&(a, b)| layers[a] < layers[b])
                .collect();
            let mut g1 = graph(&layers, &edges, &cl);
            let mut g2 = graph(&layers, &edges, &cl);
            let before = total_crossings(&g1, &mut fuel()).unwrap();
            minimise(&mut g1, &cl, None, &mut fuel()).unwrap();
            refine(&mut g1, &cl, None, &mut fuel());
            minimise(&mut g2, &cl, None, &mut fuel()).unwrap();
            refine(&mut g2, &cl, None, &mut fuel());
            assert_eq!(g1.layers, g2.layers);
            assert!(total_crossings(&g1, &mut fuel()).unwrap() <= before);
            for (l, layer) in g1.layers.iter().enumerate() {
                assert!(layer.iter().all(|&v| g1.nodes[v].layer == l));
            }
        }
    }

    #[test]
    fn mandatory_sweeps_fail_on_tiny_fuel() {
        let cl = Clusters::default();
        let layers: Vec<usize> = (0..40).map(|i| i / 10).collect();
        let edges: Vec<(usize, usize)> = (0..30)
            .map(|i| (i, (i / 10 + 1) * 10 + (i * 7 + 3) % 10))
            .collect();
        let mut g = graph(&layers, &edges, &cl);
        assert!(minimise(&mut g, &cl, None, &mut Fuel::new(5)).is_err());
        let _ = Kind::Real(0);
    }
}
