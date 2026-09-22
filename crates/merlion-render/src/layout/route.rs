//! Phase 6, edge routing (specs/layout.md#6-edge-routing), in the layout frame: `x`
//! along the order axis, `y` along the layer axis, edges running from the upper to the
//! lower layer. The caller transforms the points to the final direction.
//!
//! - `Orthogonal`: vertical runs through the dummy nodes' positions and horizontal jogs
//!   in the gaps between layers. Jogs in one gap get distinct heights, spread evenly
//!   across the gap, so parallel jogs never overlap. Ports are spread evenly along the
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
    /// Towards larger order-axis coordinates.
    After,
    /// Towards smaller order-axis coordinates.
    Before,
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
        (TB | BT, Before) => Side::Left,
        (LR | RL, After) => Side::Bottom,
        (LR | RL, Before) => Side::Top,
    }
}

/// Sign between the layout-frame tangential offset along side `s` and the screen one.
fn tangent_sign(dir: Direction, s: LSide) -> f64 {
    match (s, dir) {
        (LSide::After | LSide::Before, Direction::BT | Direction::RL) => -1.0,
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
        measure::side_offset(self.shape, self.w, self.h, final_side(dir, s), tangent_sign(dir, s) * t)
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

/// Channels used by edges that cross a container-fit wrap.
pub struct WrapFrame {
    /// Layer-axis coordinate beyond every part, where wrap edges run along the order axis.
    pub chan_y: f64,
    /// Order-axis coordinate of the channel before each part (index = part).
    pub gap_x: Vec<f64>,
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
    /// Centre of the label dummy, when the chain has one and the route passes it.
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

/// Routes every chain of `inp.g`.
pub fn route(inp: &RouteIn) -> Vec<Routed> {
    let g = inp.g;
    let dir = inp.dir;
    let layer = |v: usize| g.nodes.get(v).map_or(0, |n| n.layer);
    let part = |l: usize| inp.part.get(l).copied().unwrap_or(0);
    let is_wrap: Vec<bool> = g
        .chains
        .iter()
        .map(|c| match (c.nodes.first(), c.nodes.last()) {
            (Some(&a), Some(&z)) => inp.wrap.is_some() && part(layer(a)) != part(layer(z)),
            _ => false,
        })
        .collect();

    // Port offsets per (chain, end): end 0 on the upper node's Down side, end 1 on the
    // lower node's Up side, ordered by where the edge heads.
    let mut ends: BTreeMap<(usize, bool), Vec<(f64, usize)>> = BTreeMap::new();
    for (ci, c) in g.chains.iter().enumerate() {
        if c.nodes.len() < 2 {
            continue;
        }
        let (a, z) = (c.nodes[0], c.nodes[c.nodes.len() - 1]);
        if layer(z) <= layer(a) {
            continue;
        }
        let (k_up, k_down) = if is_wrap[ci] {
            (f64::MAX, f64::MIN)
        } else {
            (inp.pos[c.nodes[1]].0, inp.pos[c.nodes[c.nodes.len() - 2]].0)
        };
        ends.entry((a, true)).or_default().push((k_up, ci));
        ends.entry((z, false)).or_default().push((k_down, ci));
    }
    let mut port: BTreeMap<(usize, bool), f64> = BTreeMap::new();
    for ((v, down), list) in ends.iter_mut() {
        list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
        let Some(shape) = model(g, *v).and_then(|m| inp.shapes.get(m)) else {
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
        let (cx, cy) = inp.pos[v];
        let t = port.get(&(ci, down)).copied().unwrap_or(0.0);
        let side = if down { LSide::Down } else { LSide::Up };
        let off = model(g, v)
            .and_then(|m| inp.shapes.get(m))
            .map_or(0.0, |s| s.offset(dir, side, t));
        if down {
            (cx + t, cy + off)
        } else {
            (cx + t, cy - off)
        }
    };

    // Orthogonal jogs: collect per gap, then give each a distinct height.
    let orthogonal = inp.style == EdgeStyle::Orthogonal;
    let mut jogs: BTreeMap<usize, Vec<(f64, f64, usize, usize)>> = BTreeMap::new();
    let mut plans: Vec<Vec<(usize, f64)>> = vec![Vec::new(); g.chains.len()];
    if orthogonal {
        for (ci, c) in g.chains.iter().enumerate() {
            let n = c.nodes.len();
            if n < 2 || is_wrap[ci] || layer(c.nodes[n - 1]) <= layer(c.nodes[0]) {
                continue;
            }
            let start = endpoint(ci, c.nodes[0], true);
            let end = endpoint(ci, c.nodes[n - 1], false);
            let mut cur = start.0;
            for i in 1..n {
                let tx = if i == n - 1 { end.0 } else { inp.pos[c.nodes[i]].0 };
                plans[ci].push((layer(c.nodes[i - 1]), tx));
                if abs(tx - cur) > 1e-9 {
                    let gap = layer(c.nodes[i - 1]);
                    let (lo, hi) = if cur < tx { (cur, tx) } else { (tx, cur) };
                    jogs.entry(gap).or_default().push((lo, hi, ci, i));
                }
                cur = tx;
            }
        }
    }
    let mut jog_y: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for (gap, list) in jogs.iter_mut() {
        list.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(core::cmp::Ordering::Equal)
                .then(a.1.partial_cmp(&b.1).unwrap_or(core::cmp::Ordering::Equal))
                .then(a.2.cmp(&b.2))
        });
        let top = inp.layer_bot.get(*gap).copied().unwrap_or(0.0);
        let bot = inp.layer_top.get(gap + 1).copied().unwrap_or(top);
        let k = list.len();
        for (s, &(_, _, ci, i)) in list.iter().enumerate() {
            jog_y.insert((ci, i), top + (bot - top) * (s as f64 + 1.0) / (k as f64 + 1.0));
        }
    }

    let mut wrap_index = 0usize;
    g.chains
        .iter()
        .enumerate()
        .map(|(ci, c)| {
            let n = c.nodes.len();
            if n < 2 {
                return Routed::default();
            }
            let (a, z) = (c.nodes[0], c.nodes[n - 1]);
            let label = c.label.map(|d| inp.pos[d]);
            if layer(z) <= layer(a) {
                // Same layer (never produced by phase 2): a straight line between outlines.
                let (pa, pz) = (inp.pos[a], inp.pos[z]);
                let sa = model(g, a).and_then(|m| inp.shapes.get(m));
                let sz = model(g, z).and_then(|m| inp.shapes.get(m));
                let ba = sa.map_or((0.0, 0.0), |s| s.toward(dir, pz.0 - pa.0, pz.1 - pa.1));
                let bz = sz.map_or((0.0, 0.0), |s| s.toward(dir, pa.0 - pz.0, pa.1 - pz.1));
                return Routed {
                    points: vec![(pa.0 + ba.0, pa.1 + ba.1), (pz.0 + bz.0, pz.1 + bz.1)],
                    label: None,
                    wrap: false,
                };
            }
            if is_wrap[ci] {
                let w = inp.wrap.map_or(0.0, |w| w.chan_y);
                let i = wrap_index as f64;
                wrap_index += 1;
                let start = endpoint(ci, a, true);
                let end = endpoint(ci, z, false);
                let lz = layer(z);
                let target_part = part(lz);
                let gap_x = inp
                    .wrap
                    .and_then(|w| w.gap_x.get(target_part).copied())
                    .unwrap_or(0.0)
                    + i * WRAP_STEP;
                let top = inp.layer_top.get(lz).copied().unwrap_or(end.1);
                let above = if lz > 0 && part(lz - 1) == target_part {
                    inp.layer_bot.get(lz - 1).copied().unwrap_or(top - 16.0)
                } else {
                    top - 24.0
                };
                let y_t = (top + above) / 2.0;
                let y_c = w + i * WRAP_STEP;
                let mut points = vec![start, (start.0, y_c), (gap_x, y_c), (gap_x, y_t), (end.0, y_t), end];
                simplify(&mut points);
                return Routed { points, label: None, wrap: true };
            }
            if orthogonal {
                let start = endpoint(ci, a, true);
                let end = endpoint(ci, z, false);
                let mut points = vec![start];
                let mut cur = start.0;
                for (i, &(_, tx)) in plans[ci].iter().enumerate() {
                    if abs(tx - cur) > 1e-9 {
                        let y = jog_y.get(&(ci, i + 1)).copied().unwrap_or(start.1);
                        points.push((cur, y));
                        points.push((tx, y));
                    }
                    cur = tx;
                }
                points.push(end);
                simplify(&mut points);
                return Routed { points, label, wrap: false };
            }
            // Polyline (and the spline fallback).
            let interior: Vec<(f64, f64)> = c.nodes[1..n - 1].iter().map(|&d| inp.pos[d]).collect();
            let (pa, pz) = (inp.pos[a], inp.pos[z]);
            let first = interior.first().copied().unwrap_or(pz);
            let last = interior.last().copied().unwrap_or(pa);
            let sa = model(g, a).and_then(|m| inp.shapes.get(m));
            let sz = model(g, z).and_then(|m| inp.shapes.get(m));
            let ba = sa.map_or((0.0, 0.0), |s| s.toward(dir, first.0 - pa.0, first.1 - pa.1));
            let bz = sz.map_or((0.0, 0.0), |s| s.toward(dir, last.0 - pz.0, last.1 - pz.1));
            let mut points = vec![(pa.0 + ba.0, pa.1 + ba.1)];
            points.extend(interior);
            points.push((pz.0 + bz.0, pz.1 + bz.1));
            Routed { points, label, wrap: false }
        })
        .collect()
}

/// Self-loop `index` on a node centred at `(cx, cy)` in the layout frame: out of the
/// order-axis "after" side a quarter of the node's thickness above the centre, around,
/// and back in below it. Returns the points and the far edge of the loop.
pub fn self_loop(dir: Direction, shape: &NodeShape, cx: f64, cy: f64, index: usize) -> (Vec<(f64, f64)>, f64) {
    let (hx, hy) = shape.half(dir);
    let q = hy / 2.0;
    let out = cx + hx + LOOP_OUT + LOOP_STEP * index as f64;
    let o1 = shape.offset(dir, LSide::After, -q);
    let o2 = shape.offset(dir, LSide::After, q);
    (vec![(cx + o1, cy - q), (out, cy - q), (out, cy + q), (cx + o2, cy + q)], out)
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
        let s = NodeShape { shape: Shape::Rect, w: 60.0, h: 40.0 };
        let (p, out) = self_loop(Direction::TB, &s, 100.0, 50.0, 0);
        assert_eq!(p[0], (130.0, 40.0));
        assert_eq!(p[3], (130.0, 60.0));
        assert_eq!(out, 146.0);
        let (p1, out1) = self_loop(Direction::TB, &s, 100.0, 50.0, 1);
        assert!(out1 > out && p1[1].0 == out1);
    }

    #[test]
    fn longest_segment_midpoint() {
        assert_eq!(longest_segment_mid(&[(0.0, 0.0), (0.0, 2.0), (10.0, 2.0)]), Some((5.0, 2.0)));
        assert_eq!(longest_segment_mid(&[(1.0, 1.0)]), None);
    }
}
