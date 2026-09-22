//! Phase 6, edge routing (specs/layout.md#6-edge-routing), in the layout frame: `x`
//! along the order axis, `y` along the layer axis, edges running from the upper to the
//! lower layer. The caller transforms the points to the final direction.
//!
//! - `Orthogonal`: vertical runs through the dummy nodes' positions and horizontal jogs
//!   in the gaps between layers. Jogs in one gap get distinct heights, spread evenly
//!   across the gap, so parallel jogs never overlap, in the order that makes the fewest
//!   legs cross another jog ([`order_jogs`]). Ports are spread evenly along the
//!   node side, ordered by the position of the edge's other end. The draw stage rounds
//!   interior corners with a 6 px radius; the points here are the sharp corners.
//! - `Polyline`: straight segments through the dummy nodes' centres, meeting each node
//!   outline on the line towards its neighbour.
//! - `Spline` (sleeve routing) is not implemented; it falls back to `Polyline`, the
//!   fallback the spec prescribes when routing fuel runs out.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use super::lgraph::{Kind, LGraph};
use super::measure::{self, Side};
use crate::math::{abs, hypot};
use crate::model::Shape;
use crate::options::{Direction, EdgeStyle};

/// Self-loops extend this far beyond the node side, plus [`LOOP_STEP`] per extra loop.
pub const LOOP_OUT: f64 = 16.0;
pub const LOOP_STEP: f64 = 10.0;
/// Spacing between parallel wrap channels.
pub const WRAP_STEP: f64 = 6.0;

/// A node side in the layout frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LSide {
    /// Towards the next layer.
    Down,
    /// Towards the previous layer.
    Up,
    /// Towards larger order-axis coordinates (self-loops sit on this side).
    After,
}

/// The screen side a layout side becomes in direction `dir`.
pub fn final_side(dir: Direction, s: LSide) -> Side {
    use Direction::*;
    use LSide::*;
    match (dir, s) {
        (TB, Down) | (BT, Up) => Side::Bottom,
        (TB, Up) | (BT, Down) => Side::Top,
        (LR, Down) | (RL, Up) => Side::Right,
        (LR, Up) | (RL, Down) => Side::Left,
        (TB | BT, After) => Side::Right,
        (LR | RL, After) => Side::Bottom,
    }
}

/// Sign between the layout-frame tangential offset along side `s` and the screen one.
fn tangent_sign(dir: Direction, s: LSide) -> f64 {
    match (s, dir) {
        (LSide::After, Direction::BT | Direction::RL) => -1.0,
        _ => 1.0,
    }
}

/// Layout frame → screen frame (before the final translation).
pub fn to_final(dir: Direction, x: f64, y: f64) -> (f64, f64) {
    match dir {
        Direction::TB => (x, y),
        Direction::BT => (x, -y),
        Direction::LR => (y, x),
        Direction::RL => (-y, x),
    }
}

/// Screen-frame vector → layout-frame vector (inverse of [`to_final`]).
pub fn to_layout(dir: Direction, x: f64, y: f64) -> (f64, f64) {
    match dir {
        Direction::TB => (x, y),
        Direction::BT => (x, -y),
        Direction::LR => (y, x),
        Direction::RL => (y, -x),
    }
}

/// Shape and screen-frame size of one model node.
#[derive(Clone, Copy, Debug)]
pub struct NodeShape {
    pub shape: Shape,
    pub w: f64,
    pub h: f64,
}

impl NodeShape {
    /// Distance from the centre to the outline across layout side `s` at layout-frame
    /// tangential offset `t`.
    pub fn offset(&self, dir: Direction, s: LSide, t: f64) -> f64 {
        measure::side_offset(
            self.shape,
            self.w,
            self.h,
            final_side(dir, s),
            tangent_sign(dir, s) * t,
        )
    }

    pub fn span(&self, dir: Direction, s: LSide) -> f64 {
        measure::port_span(self.shape, self.w, self.h, final_side(dir, s))
    }

    /// Outline point towards layout-frame direction `(dx, dy)`, relative to the centre.
    pub fn toward(&self, dir: Direction, dx: f64, dy: f64) -> (f64, f64) {
        let (fx, fy) = to_final(dir, dx, dy);
        let (bx, by) = measure::boundary_toward(self.shape, self.w, self.h, fx, fy);
        to_layout(dir, bx, by)
    }

    /// Half extents along the order and layer axes.
    pub fn half(&self, dir: Direction) -> (f64, f64) {
        if dir.is_horizontal() {
            (self.h / 2.0, self.w / 2.0)
        } else {
            (self.w / 2.0, self.h / 2.0)
        }
    }
}

/// Channels used by edges that cross a container-fit wrap (specs/layout.md#5-container-fit).
///
/// A wrapped layout places consecutive layer ranges ("parts") one after another along
/// the order axis, each starting again at the beginning of the layer axis. A chain
/// segment between layer `s − 1` (end of part `p`) and layer `s` (start of part `p + 1`)
/// leaves along the layer axis past every part (`chan_y`), runs along the order axis to
/// the gap between the two parts (`gap_x[p]`), back along the layer axis to just before
/// part `p + 1` (`entry_y[p + 1]`), and along the order axis to its target. The steps
/// crossing one boundary take distinct tracks [`WRAP_STEP`] apart, nested so that
/// parallel detours neither overlap nor cross.
pub struct WrapFrame {
    /// Layer-axis coordinate beyond every part.
    pub chan_y: f64,
    /// Order-axis coordinate of the first channel between part `p` and `p + 1`.
    pub gap_x: Vec<f64>,
    /// Layer-axis coordinate of the first entry channel before part `p`.
    pub entry_y: Vec<f64>,
}

pub struct RouteIn<'a> {
    pub dir: Direction,
    pub style: EdgeStyle,
    pub g: &'a LGraph,
    /// Centre of every layered node (after wrap translation).
    pub pos: &'a [(f64, f64)],
    /// Layer-axis start and end of every layer (after wrap translation).
    pub layer_top: &'a [f64],
    pub layer_bot: &'a [f64],
    /// Wrap part of every layer.
    pub part: &'a [usize],
    /// Per model node.
    pub shapes: &'a [NodeShape],
    pub wrap: Option<&'a WrapFrame>,
}

/// Route of one chain, from its upper to its lower endpoint, in the layout frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Routed {
    pub points: Vec<(f64, f64)>,
    /// Centre of the label dummy, when the chain has one.
    pub label: Option<(f64, f64)>,
    pub wrap: bool,
}

fn model(g: &LGraph, v: usize) -> Option<usize> {
    match g.nodes.get(v)?.kind {
        Kind::Real(m) => Some(m),
        _ => None,
    }
}

/// Drops repeated points and interior points on a straight line.
pub fn simplify(points: &mut Vec<(f64, f64)>) {
    points.dedup_by(|b, a| abs(a.0 - b.0) < 1e-9 && abs(a.1 - b.1) < 1e-9);
    let mut i = 1;
    while i + 1 < points.len() {
        let (a, b, c) = (points[i - 1], points[i], points[i + 1]);
        let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        let forward = (b.0 - a.0) * (c.0 - b.0) + (b.1 - a.1) * (c.1 - b.1) >= 0.0;
        if abs(cross) < 1e-9 && forward {
            points.remove(i);
        } else {
            i += 1;
        }
    }
}

fn cmp_f(a: f64, b: f64) -> core::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(core::cmp::Ordering::Equal)
}

/// One horizontal jog in the gap between two layers: the edge arrives from above at
/// `from` and leaves downwards at `to`.
#[derive(Clone, Copy, Debug)]
struct Jog {
    from: f64,
    to: f64,
    chain: usize,
    step: usize,
}

impl Jog {
    fn lo(&self) -> f64 {
        if self.from < self.to {
            self.from
        } else {
            self.to
        }
    }

    /// Whether `x` lies strictly inside the jog's horizontal span.
    fn spans(&self, x: f64) -> bool {
        let hi = if self.from < self.to {
            self.to
        } else {
            self.from
        };
        x > self.lo() + 1e-9 && x < hi - 1e-9
    }
}

/// Crossings between jogs `a` and `b` when `a` runs above `b`: `a`'s downward leg
/// crosses `b`'s jog when it starts inside `b`'s span, and `b`'s upper leg crosses
/// `a`'s jog when it ends inside `a`'s span.
fn above_cost(a: &Jog, b: &Jog) -> i64 {
    i64::from(b.spans(a.to)) + i64::from(a.spans(b.from))
}

/// Jogs larger than this in one gap keep the left-to-right order: ordering them costs
/// quadratic time.
const MAX_ORDERED_JOGS: usize = 512;

/// Top-to-bottom order of the jogs in one gap. Jog `a` must run above `b` when `a`'s
/// upper leg lies inside `b`'s span, and below it when `a`'s lower leg does; only a
/// cycle of these constraints forces a crossing. The order is greedy on that
/// preference graph: repeatedly take the jog that saves the most crossings by going
/// next, ties to the leftmost jog.
fn order_jogs(list: &[Jog]) -> Vec<Jog> {
    let mut by_x: Vec<Jog> = list.to_vec();
    by_x.sort_by(|a, b| {
        cmp_f(a.lo(), b.lo())
            .then(cmp_f(a.from + a.to, b.from + b.to))
            .then(a.chain.cmp(&b.chain))
            .then(a.step.cmp(&b.step))
    });
    let k = by_x.len();
    if !(2..=MAX_ORDERED_JOGS).contains(&k) {
        return by_x;
    }
    // score[v]: crossings added by putting v above every remaining jog, minus those
    // saved; the lowest score goes next.
    let mut score: Vec<i64> = (0..k)
        .map(|v| {
            (0..k)
                .filter(|&u| u != v)
                .map(|u| above_cost(&by_x[v], &by_x[u]) - above_cost(&by_x[u], &by_x[v]))
                .sum()
        })
        .collect();
    let mut left = vec![true; k];
    let mut out = Vec::with_capacity(k);
    for _ in 0..k {
        let Some(w) = (0..k).filter(|&v| left[v]).min_by_key(|&v| (score[v], v)) else {
            break;
        };
        left[w] = false;
        out.push(by_x[w]);
        for v in 0..k {
            if left[v] {
                score[v] -= above_cost(&by_x[v], &by_x[w]) - above_cost(&by_x[w], &by_x[v]);
            }
        }
    }
    out
}

/// Routes every chain of `inp.g`.
pub fn route(inp: &RouteIn) -> Vec<Routed> {
    let g = inp.g;
    let dir = inp.dir;
    let layer = |v: usize| g.nodes.get(v).map_or(0, |n| n.layer);
    let part = |l: usize| inp.part.get(l).copied().unwrap_or(0);
    let at = |v: usize| inp.pos.get(v).copied().unwrap_or((0.0, 0.0));
    let shape_of = |v: usize| model(g, v).and_then(|m| inp.shapes.get(m));
    // Chain step `i` (from node i − 1 to node i) wraps when its layers lie in
    // different parts.
    let wraps_at =
        |c: &[usize], i: usize| inp.wrap.is_some() && part(layer(c[i - 1])) != part(layer(c[i]));
    let downward = |c: &[usize]| c.len() >= 2 && layer(c[c.len() - 1]) > layer(c[0]);

    // Port offsets per (chain, end): end `true` on the upper node's Down side, `false`
    // on the lower node's Up side, ordered by where the edge heads next. A wrapping
    // step heads into a later part, which lies further along the order axis, so it
    // sorts last on the Down side and first on the Up side.
    let mut ends: BTreeMap<(usize, bool), Vec<(f64, usize)>> = BTreeMap::new();
    for (ci, c) in g.chains.iter().enumerate() {
        let c = &c.nodes;
        if !downward(c) {
            continue;
        }
        let n = c.len();
        ends.entry((c[0], true)).or_default().push((at(c[1]).0, ci));
        ends.entry((c[n - 1], false))
            .or_default()
            .push((at(c[n - 2]).0, ci));
    }
    let mut port: BTreeMap<(usize, bool), f64> = BTreeMap::new();
    for ((v, down), list) in ends.iter_mut() {
        list.sort_by(|a, b| cmp_f(a.0, b.0).then(a.1.cmp(&b.1)));
        let Some(shape) = shape_of(*v) else {
            continue;
        };
        let side = if *down { LSide::Down } else { LSide::Up };
        let span = shape.span(dir, side);
        let k = list.len();
        for (i, &(_, ci)) in list.iter().enumerate() {
            let t = if k == 1 {
                0.0
            } else {
                -span + 2.0 * span * (i as f64 + 0.5) / k as f64
            };
            port.insert((ci, *down), t);
        }
    }
    let endpoint = |ci: usize, v: usize, down: bool| -> (f64, f64) {
        let (cx, cy) = at(v);
        let t = port.get(&(ci, down)).copied().unwrap_or(0.0);
        let side = if down { LSide::Down } else { LSide::Up };
        let off = shape_of(v).map_or(0.0, |s| s.offset(dir, side, t));
        if down {
            (cx + t, cy + off)
        } else {
            (cx + t, cy - off)
        }
    };

    // Wrap detours nest: at each boundary the step leaving furthest along the order
    // axis takes the innermost track in the outer channel and in the gap, and the
    // outermost entry track, so no two detours cross unless their targets are in the
    // opposite order to their sources. Boundaries share tracks: their channel runs never
    // overlap along the order axis.
    let orthogonal = inp.style == EdgeStyle::Orthogonal;
    let mut steps: BTreeMap<usize, Vec<(f64, f64, usize, usize)>> = BTreeMap::new();
    for (ci, c) in g.chains.iter().enumerate() {
        let c = &c.nodes;
        if !downward(c) {
            continue;
        }
        let n = c.len();
        for i in 1..n {
            if wraps_at(c, i) {
                let from = if orthogonal && i == 1 {
                    endpoint(ci, c[0], true).0
                } else {
                    at(c[i - 1]).0
                };
                let to = if orthogonal && i == n - 1 {
                    endpoint(ci, c[n - 1], false).0
                } else {
                    at(c[i]).0
                };
                steps
                    .entry(part(layer(c[i - 1])))
                    .or_default()
                    .push((from, to, ci, i));
            }
        }
    }
    // (track, tracks at the boundary) per wrapping step.
    let mut wrap_slot: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();
    for list in steps.values_mut() {
        list.sort_by(|a, b| {
            cmp_f(b.0, a.0)
                .then(cmp_f(b.1, a.1))
                .then(a.2.cmp(&b.2))
                .then(a.3.cmp(&b.3))
        });
        let k = list.len();
        for (r, &(_, _, ci, i)) in list.iter().enumerate() {
            wrap_slot.insert((ci, i), (r, k));
        }
    }
    let detour = |ci: usize, c: &[usize], i: usize, from_x: f64, to_x: f64| -> [(f64, f64); 4] {
        let (r, k) = wrap_slot.get(&(ci, i)).copied().unwrap_or((0, 1));
        let outer = k.saturating_sub(1).saturating_sub(r);
        let w = inp.wrap;
        let pa = part(layer(c[i - 1]));
        let pb = part(layer(c[i]));
        let chan = w.map_or(0.0, |w| w.chan_y) + WRAP_STEP * r as f64;
        let gx = w.and_then(|w| w.gap_x.get(pa).copied()).unwrap_or(0.0) + WRAP_STEP * r as f64;
        let entry =
            w.and_then(|w| w.entry_y.get(pb).copied()).unwrap_or(0.0) - WRAP_STEP * outer as f64;
        [(from_x, chan), (gx, chan), (gx, entry), (to_x, entry)]
    };

    // Orthogonal jogs: collected per gap, then each given a distinct height spread
    // evenly across the gap so parallel jogs never overlap.
    let mut jogs: BTreeMap<usize, Vec<Jog>> = BTreeMap::new();
    if orthogonal {
        for (ci, c) in g.chains.iter().enumerate() {
            let c = &c.nodes;
            if !downward(c) {
                continue;
            }
            let n = c.len();
            let start = endpoint(ci, c[0], true);
            let end = endpoint(ci, c[n - 1], false);
            let mut cur = start.0;
            for i in 1..n {
                let tx = if i == n - 1 { end.0 } else { at(c[i]).0 };
                if !wraps_at(c, i) && abs(tx - cur) > 1e-9 {
                    jogs.entry(layer(c[i - 1])).or_default().push(Jog {
                        from: cur,
                        to: tx,
                        chain: ci,
                        step: i,
                    });
                }
                cur = tx;
            }
        }
    }
    let mut jog_y: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for (gap, list) in jogs.iter() {
        let top = inp.layer_bot.get(*gap).copied().unwrap_or(0.0);
        let bot = inp.layer_top.get(gap + 1).copied().unwrap_or(top);
        let k = list.len();
        for (s, j) in order_jogs(list).into_iter().enumerate() {
            jog_y.insert(
                (j.chain, j.step),
                top + (bot - top) * (s as f64 + 1.0) / (k as f64 + 1.0),
            );
        }
    }

    g.chains
        .iter()
        .enumerate()
        .map(|(ci, chain)| {
            let c = &chain.nodes;
            let n = c.len();
            if n < 2 {
                return Routed::default();
            }
            let (a, z) = (c[0], c[n - 1]);
            let label = chain.label.map(at);
            if !downward(c) {
                // Same layer (never produced by phase 2): a straight line between outlines.
                let (pa, pz) = (at(a), at(z));
                let ba =
                    shape_of(a).map_or((0.0, 0.0), |s| s.toward(dir, pz.0 - pa.0, pz.1 - pa.1));
                let bz =
                    shape_of(z).map_or((0.0, 0.0), |s| s.toward(dir, pa.0 - pz.0, pa.1 - pz.1));
                return Routed {
                    points: vec![(pa.0 + ba.0, pa.1 + ba.1), (pz.0 + bz.0, pz.1 + bz.1)],
                    label: None,
                    wrap: false,
                };
            }
            let wrap = (1..n).any(|i| wraps_at(c, i));
            let mut points: Vec<(f64, f64)>;
            if orthogonal {
                let start = endpoint(ci, a, true);
                let end = endpoint(ci, z, false);
                points = vec![start];
                let mut cur = start.0;
                for i in 1..n {
                    let tx = if i == n - 1 { end.0 } else { at(c[i]).0 };
                    if wraps_at(c, i) {
                        points.extend(detour(ci, c, i, cur, tx));
                    } else if abs(tx - cur) > 1e-9 {
                        let y = jog_y.get(&(ci, i)).copied().unwrap_or(start.1);
                        points.push((cur, y));
                        points.push((tx, y));
                    }
                    cur = tx;
                }
                points.push(end);
            } else {
                // Polyline (and the spline fallback): straight segments through the
                // dummy centres, axis-aligned detours at wraps, and ends on the
                // outlines towards the neighbouring route point.
                let mut via: Vec<(f64, f64)> = Vec::new();
                for i in 1..n {
                    if wraps_at(c, i) {
                        via.extend(detour(ci, c, i, at(c[i - 1]).0, at(c[i]).0));
                    }
                    if i < n - 1 {
                        via.push(at(c[i]));
                    }
                }
                let (pa, pz) = (at(a), at(z));
                let first = via.first().copied().unwrap_or(pz);
                let last = via.last().copied().unwrap_or(pa);
                let ba = shape_of(a).map_or((0.0, 0.0), |s| {
                    s.toward(dir, first.0 - pa.0, first.1 - pa.1)
                });
                let bz =
                    shape_of(z).map_or((0.0, 0.0), |s| s.toward(dir, last.0 - pz.0, last.1 - pz.1));
                points = vec![(pa.0 + ba.0, pa.1 + ba.1)];
                points.extend(via);
                points.push((pz.0 + bz.0, pz.1 + bz.1));
            }
            simplify(&mut points);
            Routed {
                points,
                label,
                wrap,
            }
        })
        .collect()
}

/// Self-loop `index` on a node centred at `(cx, cy)` in the layout frame: out of the
/// order-axis "after" side a quarter of the node's thickness above the centre, around,
/// and back in below it. Returns the points and the far edge of the loop.
pub fn self_loop(
    dir: Direction,
    shape: &NodeShape,
    cx: f64,
    cy: f64,
    index: usize,
) -> (Vec<(f64, f64)>, f64) {
    let (hx, hy) = shape.half(dir);
    let q = hy / 2.0;
    let out = cx + hx + LOOP_OUT + LOOP_STEP * index as f64;
    let o1 = shape.offset(dir, LSide::After, -q);
    let o2 = shape.offset(dir, LSide::After, q);
    (
        vec![
            (cx + o1, cy - q),
            (out, cy - q),
            (out, cy + q),
            (cx + o2, cy + q),
        ],
        out,
    )
}

/// Midpoint of the longest segment of a polyline.
pub fn longest_segment_mid(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let mut best: Option<(f64, (f64, f64))> = None;
    for s in points.windows(2) {
        let len = hypot(s[1].0 - s[0].0, s[1].1 - s[0].1);
        if best.is_none_or(|(b, _)| len > b) {
            best = Some((len, ((s[0].0 + s[1].0) / 2.0, (s[0].1 + s[1].1) / 2.0)));
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_inverse() {
        for dir in [Direction::TB, Direction::BT, Direction::LR, Direction::RL] {
            let (fx, fy) = to_final(dir, 3.0, 7.0);
            assert_eq!(to_layout(dir, fx, fy), (3.0, 7.0), "{:?}", dir);
        }
    }

    #[test]
    fn layout_sides_map_to_screen_sides() {
        assert_eq!(final_side(Direction::TB, LSide::Down), Side::Bottom);
        assert_eq!(final_side(Direction::BT, LSide::Down), Side::Top);
        assert_eq!(final_side(Direction::LR, LSide::Down), Side::Right);
        assert_eq!(final_side(Direction::RL, LSide::Down), Side::Left);
        assert_eq!(final_side(Direction::LR, LSide::After), Side::Bottom);
        assert_eq!(final_side(Direction::TB, LSide::After), Side::Right);
    }

    #[test]
    fn simplify_drops_duplicates_and_collinear_points() {
        let mut p = vec![(0.0, 0.0), (0.0, 0.0), (0.0, 5.0), (0.0, 10.0), (4.0, 10.0)];
        simplify(&mut p);
        assert_eq!(p, vec![(0.0, 0.0), (0.0, 10.0), (4.0, 10.0)]);
        // A reversal is kept.
        let mut q = vec![(0.0, 0.0), (0.0, 10.0), (0.0, 5.0)];
        simplify(&mut q);
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn self_loop_leaves_and_returns_on_the_side() {
        let s = NodeShape {
            shape: Shape::Rect,
            w: 60.0,
            h: 40.0,
        };
        let (p, out) = self_loop(Direction::TB, &s, 100.0, 50.0, 0);
        assert_eq!(p[0], (130.0, 40.0));
        assert_eq!(p[3], (130.0, 60.0));
        assert_eq!(out, 146.0);
        let (p1, out1) = self_loop(Direction::TB, &s, 100.0, 50.0, 1);
        assert!(out1 > out && p1[1].0 == out1);
    }

    fn chain_graph(n: usize) -> LGraph {
        use super::super::lgraph::{build, BuildIn, Clusters, EdgeIn, Extent};
        let real: Vec<Extent> = (0..n)
            .map(|_| Extent {
                left: 20.0,
                right: 20.0,
                thick: 20.0,
            })
            .collect();
        let layers: Vec<usize> = (0..n).collect();
        let edges: Vec<EdgeIn> = (1..n)
            .map(|i| EdgeIn {
                edge: i - 1,
                upper: i - 1,
                lower: i,
                reversed: false,
                label: None,
            })
            .collect();
        build(&BuildIn {
            real: &real,
            layer: &layers,
            edges: &edges,
            clusters: &Clusters::default(),
            titles: &[],
            empty_size: &[],
            max_nodes: 100,
            max_layers: 100,
        })
        .unwrap()
    }

    fn graph(layers: &[usize], pairs: &[(usize, usize)]) -> LGraph {
        use super::super::lgraph::{build, BuildIn, Clusters, EdgeIn, Extent};
        let real: Vec<Extent> = layers
            .iter()
            .map(|_| Extent {
                left: 20.0,
                right: 20.0,
                thick: 20.0,
            })
            .collect();
        let edges: Vec<EdgeIn> = pairs
            .iter()
            .enumerate()
            .map(|(e, &(u, v))| EdgeIn {
                edge: e,
                upper: u,
                lower: v,
                reversed: false,
                label: None,
            })
            .collect();
        build(&BuildIn {
            real: &real,
            layer: layers,
            edges: &edges,
            clusters: &Clusters::default(),
            titles: &[],
            empty_size: &[],
            max_nodes: 100,
            max_layers: 100,
        })
        .unwrap()
    }

    fn proper_cross(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
        let o = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| {
            (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
        };
        let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
        d1 * d2 < -1e-9 && d3 * d4 < -1e-9
    }

    fn crossings(r: &[Routed]) -> usize {
        let mut n = 0;
        for (i, a) in r.iter().enumerate() {
            for b in &r[i + 1..] {
                for sa in a.points.windows(2) {
                    for sb in b.points.windows(2) {
                        if proper_cross(sa[0], sa[1], sb[0], sb[1]) {
                            n += 1;
                        }
                    }
                }
            }
        }
        n
    }

    fn two_layers(g: &LGraph, pos: &[(f64, f64)]) -> Vec<Routed> {
        let shapes = vec![
            NodeShape {
                shape: Shape::Rect,
                w: 40.0,
                h: 20.0,
            };
            g.real.len()
        ];
        let part = vec![0; g.layers.len()];
        route(&RouteIn {
            dir: Direction::TB,
            style: EdgeStyle::Orthogonal,
            g,
            pos,
            layer_top: &[-10.0, 90.0],
            layer_bot: &[10.0, 110.0],
            part: &part,
            shapes: &shapes,
            wrap: None,
        })
    }

    #[test]
    fn parallel_jogs_in_one_gap_do_not_cross() {
        // A → C and B → D both jog right (a staircase), then both jog left.
        let g = graph(&[0, 0, 1, 1], &[(0, 2), (1, 3)]);
        let right = [(0.0, 0.0), (50.0, 0.0), (100.0, 100.0), (150.0, 100.0)];
        assert_eq!(crossings(&two_layers(&g, &right)), 0);
        let left = [(100.0, 0.0), (150.0, 0.0), (0.0, 100.0), (50.0, 100.0)];
        assert_eq!(crossings(&two_layers(&g, &left)), 0);
    }

    #[test]
    fn a_fan_and_parallel_multi_edges_nest() {
        // Three parallel A → B edges offset to the right, and A → C to the left.
        let g = graph(&[0, 1, 1], &[(0, 1), (0, 1), (0, 1), (0, 2)]);
        let pos = [(60.0, 0.0), (140.0, 100.0), (0.0, 100.0)];
        assert_eq!(crossings(&two_layers(&g, &pos)), 0);
    }

    #[test]
    fn wrapped_chain_detours_around_the_outside() {
        // Four layers in two parts: layers 0-1 at order 0..40, layers 2-3 moved below
        // (order 80..120) and back to the start of the layer axis.
        let g = chain_graph(4);
        let pos = [(20.0, 10.0), (20.0, 60.0), (100.0, 10.0), (100.0, 60.0)];
        let top = [0.0, 50.0, 0.0, 50.0];
        let bot = [20.0, 70.0, 20.0, 70.0];
        let shapes = [NodeShape {
            shape: Shape::Rect,
            w: 40.0,
            h: 20.0,
        }; 4];
        let wrap = WrapFrame {
            chan_y: 90.0,
            gap_x: vec![60.0],
            entry_y: vec![0.0, -20.0],
        };
        for style in [EdgeStyle::Orthogonal, EdgeStyle::Polyline] {
            let r = route(&RouteIn {
                dir: Direction::TB,
                style,
                g: &g,
                pos: &pos,
                layer_top: &top,
                layer_bot: &bot,
                part: &[0, 0, 1, 1],
                shapes: &shapes,
                wrap: Some(&wrap),
            });
            assert_eq!(
                r.iter().map(|x| x.wrap).collect::<Vec<_>>(),
                vec![false, true, false]
            );
            let w = &r[1].points;
            assert_eq!(w.first(), Some(&(20.0, 70.0)));
            assert_eq!(w.last(), Some(&(100.0, 0.0)));
            assert_eq!(
                &w[1..w.len() - 1],
                &[(20.0, 90.0), (60.0, 90.0), (60.0, -20.0), (100.0, -20.0)]
            );
        }
    }

    #[test]
    fn wrap_detours_nest_instead_of_crossing() {
        // A → C and B → D both cross the wrap between part 0 (layer 0) and part 1
        // (layer 1, moved after part 0 and back to the top).
        let g = graph(&[0, 0, 1, 1], &[(0, 2), (1, 3)]);
        let pos = [(20.0, 10.0), (80.0, 10.0), (160.0, 10.0), (220.0, 10.0)];
        let shapes = [NodeShape {
            shape: Shape::Rect,
            w: 40.0,
            h: 20.0,
        }; 4];
        let wrap = WrapFrame {
            chan_y: 60.0,
            gap_x: vec![120.0],
            entry_y: vec![-30.0, -30.0],
        };
        for style in [EdgeStyle::Orthogonal, EdgeStyle::Polyline] {
            let r = route(&RouteIn {
                dir: Direction::TB,
                style,
                g: &g,
                pos: &pos,
                layer_top: &[0.0, 0.0],
                layer_bot: &[20.0, 20.0],
                part: &[0, 1],
                shapes: &shapes,
                wrap: Some(&wrap),
            });
            assert!(r.iter().all(|x| x.wrap));
            assert_eq!(crossings(&r), 0, "{:?}", style);
        }
    }

    #[test]
    fn longest_segment_midpoint() {
        assert_eq!(
            longest_segment_mid(&[(0.0, 0.0), (0.0, 2.0), (10.0, 2.0)]),
            Some((5.0, 2.0))
        );
        assert_eq!(longest_segment_mid(&[(1.0, 1.0)]), None);
    }
}
