//! The layout pipeline for flowcharts: measure, phases 1–7 and container fit
//! (specs/layout.md). Each phase lives in its own module; this module sizes the
//! layered graph for a direction, runs coordinate assignment and routing, places edge
//! labels and cluster boxes, and chooses between container-fit candidates.
//!
//! Frames: phases 2–6 work in the *layout frame* (`x` along a layer, `y` across
//! layers, `TB` orientation). [`route::to_final`] maps it to the screen for the chosen
//! direction, and the result is translated so everything starts at [`MARGIN`].

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use super::coords::{self, Pad, Rect};
use super::fit;
use super::hint;
use super::lgraph::{self, BuildError, BuildIn, Clusters, EdgeIn, Extent, Kind, LGraph};
use super::measure;
use super::order::{self, Stable};
use super::route::{self, NodeShape, RouteIn, WrapFrame};
use super::{acyclic, layering, LayoutError};
use crate::diag::{Diagnostics, Severity, Span};
use crate::fuel::{Fuel, OutOfFuel};
use crate::geometry::{chip_size, ClusterGeom, EdgeGeom, EdgeLabelGeom, Geometry, NodeGeom, Point};
use crate::math::{abs, clamp, hypot, max, min};
use crate::model::Flowchart;
use crate::options::{Direction, DirectionOption, EdgeStyle, RenderOptions};
use crate::text::{self, LabelLayout, TextStyle, Weight};

/// Blank border around the drawing on every side.
pub const MARGIN: f64 = 8.0;
/// Padding between a cluster box and its members (specs/layout.md#7-clusters-subgraphs).
pub const CLUSTER_PAD: f64 = 12.0;
/// Space above a cluster title inside the box; the title band is
/// `CLUSTER_PAD + title height` and leaves the same space below the title.
pub const TITLE_TOP: f64 = CLUSTER_PAD / 2.0;
/// Clearance kept around an edge-label chip when it is placed and when space is
/// reserved for it.
pub const LABEL_CLEAR: f64 = 4.0;
/// Gap between a self-loop and its label.
pub const LOOP_LABEL_GAP: f64 = 4.0;
/// Container fit reduces the wrap width in these steps, down to [`MIN_WRAP_WIDTH`].
pub const WRAP_STEP_PX: f64 = 20.0;
pub const MIN_WRAP_WIDTH: f64 = 120.0;
/// Rounds of layer splitting (`TB`/`BT`) and of wrapping (`LR`/`RL`) per candidate.
const MAX_FIT_ROUNDS: usize = 8;
/// Label placement tries at most this many positions along an edge.
const MAX_LABEL_SAMPLES: usize = 400;

fn too_large_fuel(_: OutOfFuel) -> LayoutError {
    LayoutError::TooLarge { what: "fuel" }
}

fn too_large_build(e: BuildError) -> LayoutError {
    match e {
        BuildError::TooManyNodes => LayoutError::TooLarge {
            what: "layered nodes",
        },
        BuildError::TooManyLayers => LayoutError::TooLarge { what: "layers" },
    }
}

fn cmp_f(a: f64, b: f64) -> core::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(core::cmp::Ordering::Equal)
}

/// Render options with every number made finite and bounded, so hostile values cannot
/// produce NaN coordinates or unbounded work.
#[derive(Clone, Debug)]
struct Opts {
    target_width: f64,
    max_aspect: f64,
    auto: bool,
    style: EdgeStyle,
    node_spacing: f64,
    rank_spacing: f64,
    stability: usize,
    font_size: f64,
    wrap_width: f64,
    /// Label-box width factor of the font mode (`text::width_tolerance`).
    tolerance: f64,
}

fn finite_or(v: f64, default: f64) -> f64 {
    if v.is_nan() {
        default
    } else {
        v
    }
}

impl Opts {
    fn new(o: &RenderOptions) -> Self {
        Opts {
            // +∞ stays: "never wrap". NaN falls back to the default.
            target_width: max(finite_or(o.target_width, 720.0), 0.0),
            max_aspect: clamp(finite_or(o.max_aspect, 1.6), 0.0, 1e9),
            auto: o.direction == DirectionOption::Auto,
            style: o.edge_style,
            node_spacing: clamp(finite_or(o.node_spacing, 24.0), 0.0, 1_000.0),
            rank_spacing: clamp(finite_or(o.rank_spacing, 48.0), 0.0, 1_000.0),
            stability: usize::try_from(o.stability).unwrap_or(usize::MAX),
            font_size: clamp(finite_or(o.font_size, 14.0), 1.0, 1_000.0),
            wrap_width: clamp(finite_or(o.wrap_width, 200.0), 1.0, 100_000.0),
            tolerance: text::width_tolerance(o.font),
        }
    }
}

/// Measured labels and node sizes (screen frame).
#[derive(Clone, Debug, PartialEq)]
struct Meas {
    node_label: Vec<LabelLayout>,
    /// Outer `(w, h)` of every node.
    size: Vec<(f64, f64)>,
    edge_label: Vec<Option<LabelLayout>>,
    title: Vec<LabelLayout>,
}

impl Meas {
    fn widest_label(&self) -> f64 {
        self.node_label
            .iter()
            .chain(self.edge_label.iter().flatten())
            .map(|l| l.width)
            .fold(0.0, max)
    }
}

fn clean_label(mut l: LabelLayout) -> LabelLayout {
    let fix = |v: f64| if v.is_finite() { max(v, 0.0) } else { 0.0 };
    l.width = fix(l.width);
    l.height = fix(l.height);
    l
}

fn measure_all(chart: &Flowchart, o: &Opts, wrap: f64, diags: &mut Diagnostics) -> Meas {
    // The box widens by the font mode's tolerance; lines keep their measured widths and
    // stay centred, so the extra room splits evenly on both sides.
    let clean_label = |l: LabelLayout| {
        let mut l = clean_label(l);
        l.width *= o.tolerance;
        l
    };
    let mut node_label = Vec::with_capacity(chart.nodes.len());
    let mut size = Vec::with_capacity(chart.nodes.len());
    for node in &chart.nodes {
        let style = measure::text_style(chart, node, o.font_size);
        let l = if node.shape.draws_label() {
            clean_label(text::layout_label(&node.label, &style, wrap, diags))
        } else {
            LabelLayout::default()
        };
        size.push(measure::node_size(node.shape, l.width, l.height));
        node_label.push(l);
    }
    let base = TextStyle {
        font_size: o.font_size,
        weight: Weight::Regular,
        italic: false,
    };
    let edge_label = chart
        .edges
        .iter()
        .map(|e| {
            e.label.as_ref().filter(|t| !t.is_empty()).map(|t| {
                let style = measure::style_to_text(&e.style, o.font_size);
                clean_label(text::layout_label(t, &style, wrap, diags))
            })
        })
        .collect();
    let title = chart
        .subgraphs
        .iter()
        .map(|s| {
            if s.title.is_empty() {
                LabelLayout::default()
            } else {
                clean_label(text::layout_label(&s.title, &base, wrap, diags))
            }
        })
        .collect();
    Meas {
        node_label,
        size,
        edge_label,
        title,
    }
}

/// Everything about the chart that does not depend on direction or label size.
struct Base<'a> {
    chart: &'a Flowchart,
    o: Opts,
    cl: Clusters,
    /// Per model edge: whether phase 1 reversed it.
    reversed: Vec<bool>,
    /// Model edges that take part in layering (both ends valid, not a self-loop).
    normal: Vec<usize>,
    /// Self-loop edges of every node, in edge order.
    loops: Vec<Vec<usize>>,
    rank: Vec<u8>,
    /// Hint rank of every surviving node.
    stable_rank: Option<Vec<Option<usize>>>,
    max_nodes: usize,
    max_layers: usize,
}

impl Base<'_> {
    fn endpoints(&self, e: usize) -> Option<(usize, usize)> {
        let edge = self.chart.edges.get(e)?;
        if self.reversed.get(e).copied().unwrap_or(false) {
            Some((edge.to, edge.from))
        } else {
            Some((edge.from, edge.to))
        }
    }

    fn stable_for(&self, g: &LGraph) -> Option<Stable> {
        let ranks = self.stable_rank.as_ref()?;
        let fixed = g
            .nodes
            .iter()
            .map(|v| match v.kind {
                Kind::Real(m) => ranks.get(m).copied().flatten(),
                _ => None,
            })
            .collect();
        Some(Stable {
            fixed,
            stability: self.o.stability,
        })
    }
}

/// Screen size → layout-frame (order extent, layer extent).
fn axes(dir: Direction, w: f64, h: f64) -> (f64, f64) {
    if dir.is_horizontal() {
        (h, w)
    } else {
        (w, h)
    }
}

fn node_extent(base: &Base, m: &Meas, dir: Direction, v: usize) -> Extent {
    let (w, h) = m.size.get(v).copied().unwrap_or((0.0, 0.0));
    let (o, t) = axes(dir, w, h);
    let mut e = Extent {
        left: o / 2.0,
        right: o / 2.0,
        thick: t,
    };
    // Self-loops sit on the "after" side of the order axis (right in TB/BT, below in
    // LR/RL); reserve room for the loops and their labels there.
    if let Some(loops) = base.loops.get(v).filter(|l| !l.is_empty()) {
        e.right += route::LOOP_OUT + route::LOOP_STEP * (loops.len() - 1) as f64;
        let mut lab_o: f64 = 0.0;
        for &le in loops {
            if let Some(Some(l)) = m.edge_label.get(le) {
                let (cw, ch) = chip_size(l);
                let (lo, lt) = axes(dir, cw, ch);
                lab_o = max(lab_o, lo);
                e.thick = max(e.thick, lt + 2.0 * LABEL_CLEAR);
            }
        }
        if lab_o > 0.0 {
            e.right += LOOP_LABEL_GAP + lab_o + LABEL_CLEAR;
        }
    }
    e
}

fn label_extent(m: &Meas, dir: Direction, e: usize) -> Option<Extent> {
    let l = m.edge_label.get(e)?.as_ref()?;
    let (cw, ch) = chip_size(l);
    let (o, t) = axes(dir, cw, ch);
    Some(Extent {
        left: o / 2.0 + LABEL_CLEAR / 2.0,
        right: o / 2.0 + LABEL_CLEAR / 2.0,
        thick: t + LABEL_CLEAR,
    })
}

/// The title filler keeps the cluster at least as wide (on screen) as its title.
fn title_extent(m: &Meas, dir: Direction, c: usize) -> Extent {
    let w = m.title.get(c).map_or(0.0, |t| t.width);
    if dir.is_horizontal() {
        Extent {
            left: 0.0,
            right: 0.0,
            thick: w,
        }
    } else {
        Extent {
            left: w / 2.0,
            right: w / 2.0,
            thick: 0.0,
        }
    }
}

fn title_band(m: &Meas, c: usize) -> f64 {
    let h = m.title.get(c).map_or(0.0, |t| t.height);
    CLUSTER_PAD + h
}

/// A cluster without nodes is drawn as a box around its title.
fn empty_extent(m: &Meas, dir: Direction, c: usize) -> Extent {
    let t = m.title.get(c);
    let w = t.map_or(0.0, |t| t.width) + 2.0 * CLUSTER_PAD;
    let h = t.map_or(0.0, |t| t.height) + 2.0 * CLUSTER_PAD;
    let (o, th) = axes(dir, w, h);
    Extent {
        left: o / 2.0,
        right: o / 2.0,
        thick: th,
    }
}

/// Cluster padding in the layout frame: [`CLUSTER_PAD`] on every side plus the title
/// band on the side that is the top of the screen.
fn pads(m: &Meas, dir: Direction, k: usize) -> Vec<Pad> {
    (0..k)
        .map(|c| {
            let band = title_band(m, c);
            let mut p = Pad {
                order_before: CLUSTER_PAD,
                order_after: CLUSTER_PAD,
                layer_before: CLUSTER_PAD,
                layer_after: CLUSTER_PAD,
            };
            match dir {
                Direction::TB => p.layer_before = band,
                Direction::BT => p.layer_after = band,
                Direction::LR | Direction::RL => p.order_before = band,
            }
            p
        })
        .collect()
}

/// The layered graph plus bookkeeping that survives direction changes.
#[derive(Clone)]
struct Layered {
    g: LGraph,
    /// Layer of every model node.
    layer: Vec<usize>,
    /// Layered nodes that are title fillers.
    title_filler: Vec<bool>,
    /// First layer of every cluster that has layered nodes.
    cluster_lo: Vec<Option<usize>>,
}

fn build_layered(
    base: &Base,
    m: &Meas,
    dir: Direction,
    layer: &[usize],
) -> Result<Layered, BuildError> {
    let n = base.chart.nodes.len();
    let k = base.cl.len();
    let real: Vec<Extent> = (0..n).map(|v| node_extent(base, m, dir, v)).collect();
    let edges: Vec<EdgeIn> = base
        .normal
        .iter()
        .filter_map(|&e| {
            let (u, v) = base.endpoints(e)?;
            Some(EdgeIn {
                edge: e,
                upper: u,
                lower: v,
                reversed: base.reversed.get(e).copied().unwrap_or(false),
                label: label_extent(m, dir, e),
            })
        })
        .collect();
    let titles: Vec<Extent> = (0..k).map(|c| title_extent(m, dir, c)).collect();
    let empty: Vec<Extent> = (0..k).map(|c| empty_extent(m, dir, c)).collect();
    let g = lgraph::build(&BuildIn {
        real: &real,
        layer,
        edges: &edges,
        clusters: &base.cl,
        titles: &titles,
        empty_size: &empty,
        max_nodes: base.max_nodes,
        max_layers: base.max_layers,
    })?;
    // The first filler of a cluster with a non-empty title extent is its title filler
    // (lgraph::build pushes it before the gap fillers, which have zero extent).
    let mut seen = vec![false; k];
    let title_filler = g
        .nodes
        .iter()
        .map(|v| match v.kind {
            Kind::Filler(c) if c < k && !seen[c] => {
                seen[c] = true;
                v.left + v.right > 0.0 || v.thick > 0.0
            }
            _ => false,
        })
        .collect();
    let mut cluster_lo: Vec<Option<usize>> = vec![None; k];
    for v in &g.nodes {
        for c in base.cl.chain(v.cluster) {
            cluster_lo[c] = Some(cluster_lo[c].map_or(v.layer, |l: usize| l.min(v.layer)));
        }
    }
    Ok(Layered {
        g,
        layer: layer.to_vec(),
        title_filler,
        cluster_lo,
    })
}

/// Re-sizes every layered node for `dir` and the measurements `m` (the order and the
/// graph stay as they are).
fn apply_extents(lay: &mut Layered, base: &Base, m: &Meas, dir: Direction) {
    for (i, v) in lay.g.nodes.iter_mut().enumerate() {
        let e = match v.kind {
            Kind::Real(x) => node_extent(base, m, dir, x),
            Kind::Dummy(_) => Extent::default(),
            Kind::Label(e) => label_extent(m, dir, e).unwrap_or_default(),
            Kind::Filler(c) => {
                if lay.title_filler.get(i).copied().unwrap_or(false) {
                    title_extent(m, dir, c)
                } else {
                    Extent::default()
                }
            }
            Kind::EmptyCluster(c) => empty_extent(m, dir, c),
        };
        v.left = e.left;
        v.right = e.right;
        v.thick = e.thick;
    }
}

/// How a dummy node gets its initial-order key.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DummyKey {
    /// Interpolated between the chain's ends.
    Lerp,
    /// The key of the chain's lower end (the dummy sits above its target).
    Lower,
}

/// Initial-order keys for every layered node from a value per model node; cluster
/// placeholders take their cluster's smallest member value.
fn keys_from(g: &LGraph, cl: &Clusters, val: &[f64], dummy: DummyKey) -> Vec<f64> {
    let big = val.iter().copied().fold(0.0, max) + 1.0;
    let mut keys = vec![big; g.nodes.len()];
    for (m, &v) in g.real.iter().enumerate() {
        if let Some(k) = keys.get_mut(v) {
            *k = val.get(m).copied().unwrap_or(big);
        }
    }
    for ch in &g.chains {
        let (Some(&a), Some(&z)) = (ch.nodes.first(), ch.nodes.last()) else {
            continue;
        };
        let (ka, kz) = (keys[a], keys[z]);
        let n = ch.nodes.len();
        for (i, &d) in ch
            .nodes
            .iter()
            .enumerate()
            .take(n.saturating_sub(1))
            .skip(1)
        {
            keys[d] = match dummy {
                DummyKey::Lerp => ka + (kz - ka) * i as f64 / (n - 1) as f64,
                DummyKey::Lower => kz,
            };
        }
    }
    let mut cluster_min = vec![big; cl.len()];
    for (v, node) in g.nodes.iter().enumerate() {
        if let Kind::Real(_) = node.kind {
            for c in cl.chain(node.cluster) {
                cluster_min[c] = min(cluster_min[c], keys[v]);
            }
        }
    }
    for (v, node) in g.nodes.iter().enumerate() {
        match node.kind {
            Kind::Filler(c) | Kind::EmptyCluster(c) => {
                let from = if let Kind::EmptyCluster(_) = node.kind {
                    node.cluster
                } else {
                    Some(c)
                };
                keys[v] = from
                    .and_then(|c| cluster_min.get(c).copied())
                    .unwrap_or(big);
            }
            _ => {}
        }
    }
    keys
}

/// Mandatory work after phase 3 (coordinates, fit, routing, labels), held back from the
/// optional passes (ADR-0008).
fn reserve_for(g: &LGraph) -> u64 {
    let segs: usize = g.down.iter().map(Vec::len).sum();
    let size = (g.nodes.len() + segs + g.chains.len()) as u64;
    size.saturating_mul(64).saturating_add(4_096)
}

/// Phase 3: initial order, mandatory sweeps, then the optional refinement passes.
fn order_layers(
    base: &Base,
    lay: &mut Layered,
    keys: &[f64],
    refine: bool,
    fuel: &mut Fuel,
) -> Result<(), OutOfFuel> {
    let stable = base.stable_for(&lay.g);
    order::initial_order(&mut lay.g, &base.cl, keys, stable.as_ref());
    fuel.set_reserve(reserve_for(&lay.g));
    order::minimise(&mut lay.g, &base.cl, stable.as_ref(), fuel)?;
    if refine {
        order::refine(&mut lay.g, &base.cl, stable.as_ref(), fuel);
    }
    fuel.set_reserve(0);
    Ok(())
}

/// Phase 4 output in the layout frame.
struct Coords {
    x: Vec<f64>,
    y: Vec<f64>,
    thick: Vec<f64>,
    boxes: Vec<Option<Rect>>,
}

fn coordinates(
    base: &Base,
    m: &Meas,
    lay: &Layered,
    dir: Direction,
    fuel: &mut Fuel,
) -> Result<Coords, OutOfFuel> {
    let g = &lay.g;
    let o = &base.o;
    let p = pads(m, dir, base.cl.len());
    let mut x = coords::assign_x(g, o.node_spacing, fuel)?;
    coords::fit_clusters(g, &base.cl, &p, o.node_spacing, &mut x, fuel)?;
    // A labelled edge between neighbouring layers has no label dummy; its label goes in
    // the gap, which must be wide enough for it. Labels whose edges run side by side
    // are stacked across the gap ([`gap_label_groups`]), so it holds all of them.
    let mut min_gap = vec![0.0f64; g.layers.len()];
    for ch in &g.chains {
        if ch.nodes.len() != 2 {
            continue;
        }
        let Some(Some(l)) = m.edge_label.get(ch.edge) else {
            continue;
        };
        let (cw, chh) = chip_size(l);
        let (_, t) = axes(dir, cw, chh);
        let gap = g.nodes.get(ch.nodes[0]).map_or(0, |v| v.layer);
        if let Some(mg) = min_gap.get_mut(gap) {
            *mg = max(*mg, t + 2.0 * LABEL_CLEAR);
        }
    }
    for (gap, group) in gap_label_groups(g, m, dir, &x) {
        let need: f64 = group.iter().map(|&(_, t)| t + 2.0 * LABEL_CLEAR).sum();
        if let Some(mg) = min_gap.get_mut(gap) {
            *mg = max(*mg, need);
        }
    }
    let (y, thick) = coords::layer_y(g, &base.cl, &p, o.rank_spacing, &min_gap);
    let boxes = coords::cluster_boxes(g, &base.cl, &p, &x, &y);
    Ok(Coords { x, y, thick, boxes })
}

/// Chains of one gap whose labels share it: `(chain, chip thickness)` each.
type LabelGroup = Vec<(usize, f64)>;

/// Labels of edges between neighbouring layers whose chips could collide: per gap,
/// groups of two or more chains whose chips, centred between the chain's ends on the
/// order axis, would overlap, each with its chip's layer-axis thickness. Groups keep the
/// chains in order of their range's start.
fn gap_label_groups(g: &LGraph, m: &Meas, dir: Direction, x: &[f64]) -> Vec<(usize, LabelGroup)> {
    // (gap, lo, hi, chain, thickness)
    let mut items: Vec<(usize, f64, f64, usize, f64)> = Vec::new();
    for (ci, ch) in g.chains.iter().enumerate() {
        let &[a, b] = ch.nodes.as_slice() else {
            continue;
        };
        let Some(Some(l)) = m.edge_label.get(ch.edge) else {
            continue;
        };
        let (cw, chh) = chip_size(l);
        let (o, t) = axes(dir, cw, chh);
        let (xa, xb) = (
            x.get(a).copied().unwrap_or(0.0),
            x.get(b).copied().unwrap_or(0.0),
        );
        let gap = g.nodes.get(a).map_or(0, |v| v.layer);
        let mid = (xa + xb) / 2.0;
        items.push((
            gap,
            mid - o / 2.0 - LABEL_CLEAR,
            mid + o / 2.0 + LABEL_CLEAR,
            ci,
            t,
        ));
    }
    items.sort_by(|p, q| p.0.cmp(&q.0).then(cmp_f(p.1, q.1)).then(p.3.cmp(&q.3)));
    let mut out: Vec<(usize, LabelGroup)> = Vec::new();
    let mut cur: Option<(usize, f64, LabelGroup)> = None;
    for (gap, lo, hi, ci, t) in items {
        match cur.as_mut() {
            Some((cg, reach, list)) if *cg == gap && lo < *reach => {
                *reach = max(*reach, hi);
                list.push((ci, t));
            }
            _ => {
                if let Some((cg, _, list)) = cur.take() {
                    if list.len() >= 2 {
                        out.push((cg, list));
                    }
                }
                cur = Some((gap, hi, vec![(ci, t)]));
            }
        }
    }
    if let Some((cg, _, list)) = cur {
        if list.len() >= 2 {
            out.push((cg, list));
        }
    }
    out
}

/// The point where a route (layout frame) first reaches layer-axis coordinate `y`.
fn at_layer_y(points: &[(f64, f64)], y: f64) -> Option<(f64, f64)> {
    points.windows(2).find_map(|s| {
        let (a, b) = (s[0], s[1]);
        let (lo, hi) = (min(a.1, b.1), max(a.1, b.1));
        if y < lo || y > hi {
            return None;
        }
        let t = if hi - lo > 1e-9 {
            (y - a.1) / (b.1 - a.1)
        } else {
            0.0
        };
        Some((a.0 + (b.0 - a.0) * t, y))
    })
}

type BoxF = (f64, f64, f64, f64);

fn boxes_overlap(a: BoxF, b: BoxF) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

fn centred(x: f64, y: f64, w: f64, h: f64, pad: f64) -> BoxF {
    (
        x - w / 2.0 - pad,
        y - h / 2.0 - pad,
        x + w / 2.0 + pad,
        y + h / 2.0 + pad,
    )
}

/// Label position on `points` (screen frame): the midpoint of the longest segment,
/// moved along the edge in both directions until the chip (with [`LABEL_CLEAR`]/2
/// around it) overlaps no node and no placed label (specs/layout.md#6-edge-routing).
/// Falls back to the first spot clear of nodes, then to the midpoint. Each tried spot
/// draws optional fuel; without fuel the midpoint is used.
fn place_label(
    points: &[(f64, f64)],
    w: f64,
    h: f64,
    nodes: &[BoxF],
    placed: &[BoxF],
    fuel: &mut Fuel,
) -> Option<(f64, f64)> {
    let mid = route::longest_segment_mid(points)?;
    let lens: Vec<f64> = points
        .windows(2)
        .map(|s| hypot(s[1].0 - s[0].0, s[1].1 - s[0].1))
        .collect();
    let total: f64 = lens.iter().sum();
    // Arc length of the midpoint.
    let mut best_i = 0;
    for (i, &l) in lens.iter().enumerate() {
        if l > lens[best_i] {
            best_i = i;
        }
    }
    let s_mid: f64 =
        lens[..best_i].iter().sum::<f64>() + lens.get(best_i).copied().unwrap_or(0.0) / 2.0;
    let at = |s: f64| -> (f64, f64) {
        let mut rest = clamp(s, 0.0, total);
        for (i, &l) in lens.iter().enumerate() {
            if rest <= l || i + 1 == lens.len() {
                let t = if l > 0.0 {
                    clamp(rest / l, 0.0, 1.0)
                } else {
                    0.0
                };
                let (a, b) = (points[i], points[i + 1]);
                return (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            }
            rest -= l;
        }
        mid
    };
    // Only boxes near the edge can collide.
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in points {
        x0 = min(x0, p.0);
        y0 = min(y0, p.1);
        x1 = max(x1, p.0);
        y1 = max(y1, p.1);
    }
    let area = (x0 - w, y0 - h, x1 + w, y1 + h);
    let near: Vec<BoxF> = nodes
        .iter()
        .copied()
        .filter(|&b| boxes_overlap(b, area))
        .collect();
    let near_labels: Vec<BoxF> = placed
        .iter()
        .copied()
        .filter(|&b| boxes_overlap(b, area))
        .collect();
    let step = max(2.0, total / MAX_LABEL_SAMPLES as f64);
    let mut node_free: Option<(f64, f64)> = None;
    let mut k = 0usize;
    loop {
        let off = step * k.div_ceil(2) as f64;
        if off > total + step {
            break;
        }
        let s = if k % 2 == 1 { s_mid + off } else { s_mid - off };
        k += 1;
        if s < 0.0 || s > total {
            continue;
        }
        if fuel
            .burn_optional((near.len() + near_labels.len()) as u64 + 1)
            .is_err()
        {
            break;
        }
        let p = at(s);
        let b = centred(p.0, p.1, w, h, LABEL_CLEAR / 2.0);
        if near.iter().any(|&n| boxes_overlap(n, b)) {
            continue;
        }
        if near_labels.iter().all(|&l| !boxes_overlap(l, b)) {
            return Some(p);
        }
        if node_free.is_none() {
            node_free = Some(p);
        }
    }
    Some(node_free.unwrap_or(mid))
}

/// Phases 5 (wrap translation), 6 and 7 for one set of coordinates: node, edge,
/// label and cluster geometry in the screen frame. `part_of[l]` is the wrap part of
/// layer `l` (all zero without wrapping).
fn finish(
    base: &Base,
    m: &Meas,
    lay: &Layered,
    dir: Direction,
    co: &Coords,
    part_of: &[usize],
    fuel: &mut Fuel,
) -> Result<Geometry, OutOfFuel> {
    let g = &lay.g;
    let o = &base.o;
    let chart = base.chart;
    let nl = g.layers.len();
    let part = |l: usize| part_of.get(l).copied().unwrap_or(0);
    let np = (0..nl).map(part).max().map_or(1, |p| p + 1);
    let ly = |l: usize| co.y.get(l).copied().unwrap_or(0.0);
    let lt = |l: usize| co.thick.get(l).copied().unwrap_or(0.0);
    let top: Vec<f64> = (0..nl).map(|l| ly(l) - lt(l) / 2.0).collect();
    let bot: Vec<f64> = (0..nl).map(|l| ly(l) + lt(l) / 2.0).collect();
    let cluster_part = |c: usize| lay.cluster_lo.get(c).copied().flatten().map_or(0, part);
    fuel.burn((g.nodes.len() + g.chains.len()) as u64 + 1)?;

    // Wrap translation: part p moves back to the start of the layer axis and after
    // part p − 1 along the order axis, leaving room for the wrap channels.
    let mut shift = vec![(0.0f64, 0.0f64); np];
    let mut frame: Option<WrapFrame> = None;
    if np > 1 {
        let mut o0 = vec![f64::MAX; np];
        let mut o1 = vec![f64::MIN; np];
        let mut l0 = vec![f64::MAX; np];
        let mut l1 = vec![f64::MIN; np];
        for (v, node) in g.nodes.iter().enumerate() {
            let p = part(node.layer);
            let xv = co.x.get(v).copied().unwrap_or(0.0);
            o0[p] = min(o0[p], xv - node.left);
            o1[p] = max(o1[p], xv + node.right);
            l0[p] = min(l0[p], top.get(node.layer).copied().unwrap_or(0.0));
            l1[p] = max(l1[p], bot.get(node.layer).copied().unwrap_or(0.0));
        }
        for (c, b) in co.boxes.iter().enumerate() {
            if let Some(b) = b {
                let p = cluster_part(c).min(np - 1);
                o0[p] = min(o0[p], b.x0);
                o1[p] = max(o1[p], b.x1);
                l0[p] = min(l0[p], b.y0);
                l1[p] = max(l1[p], b.y1);
            }
        }
        for p in 0..np {
            if o0[p] > o1[p] {
                (o0[p], o1[p], l0[p], l1[p]) = (0.0, 0.0, 0.0, 0.0);
            }
        }
        // Wrap crossings per boundary, and the label chips (order axis, clearance
        // included) of edges between neighbouring layers that cross it: such a label
        // sits on the detour's run through the gap, which grows to stack them.
        let mut cnt = vec![0usize; np];
        let mut chips = vec![0.0f64; np];
        let mut chip = vec![0.0f64; g.chains.len()];
        for (ci, ch) in g.chains.iter().enumerate() {
            for w in ch.nodes.windows(2) {
                let (pa, pb) = (part(g.nodes[w[0]].layer), part(g.nodes[w[1]].layer));
                if pa != pb {
                    cnt[pa.min(np - 1)] += 1;
                    if let (2, Some(Some(l))) = (ch.nodes.len(), m.edge_label.get(ch.edge)) {
                        let (cw, chh) = chip_size(l);
                        let (t, _) = axes(dir, cw, chh);
                        chip[ci] = t + LABEL_CLEAR;
                        chips[pa.min(np - 1)] += chip[ci];
                    }
                }
            }
        }
        let mut gap_x = vec![0.0; np];
        for p in 0..np {
            let dy = l0[0] - l0[p];
            let dx = if p == 0 {
                0.0
            } else {
                let prev_end = o1[p - 1] + shift[p - 1].0;
                gap_x[p - 1] = prev_end + o.node_spacing / 2.0 + route::WRAP_STEP / 2.0;
                let channels = route::WRAP_STEP * cnt[p - 1] as f64 + chips[p - 1];
                prev_end + o.node_spacing + channels - o0[p]
            };
            shift[p] = (dx, dy);
        }
        let far = (0..np).map(|p| l1[p] + shift[p].1).fold(f64::MIN, max);
        let out = max(o.rank_spacing / 2.0, route::WRAP_STEP);
        frame = Some(WrapFrame {
            chan_y: far + out,
            gap_x,
            entry_y: vec![l0[0] - out; np],
            chip,
        });
    }
    let sh = |l: usize| shift.get(part(l)).copied().unwrap_or((0.0, 0.0));
    let pos: Vec<(f64, f64)> = g
        .nodes
        .iter()
        .enumerate()
        .map(|(v, node)| {
            let (dx, dy) = sh(node.layer);
            (
                co.x.get(v).copied().unwrap_or(0.0) + dx,
                ly(node.layer) + dy,
            )
        })
        .collect();
    // Jogs between layer l and l + 1 stay clear of cluster boxes that end at l or start
    // at l + 1 (their padding and title bands lie in that gap).
    let mut top_s: Vec<f64> = (0..nl).map(|l| top[l] + sh(l).1).collect();
    let mut bot_s: Vec<f64> = (0..nl).map(|l| bot[l] + sh(l).1).collect();
    let mut cluster_hi: Vec<Option<usize>> = vec![None; base.cl.len()];
    for v in &g.nodes {
        for c in base.cl.chain(v.cluster) {
            cluster_hi[c] = Some(cluster_hi[c].map_or(v.layer, |h: usize| h.max(v.layer)));
        }
    }
    for (c, b) in co.boxes.iter().enumerate() {
        let (Some(b), Some(lo), Some(hi)) = (b, lay.cluster_lo[c], cluster_hi[c]) else {
            continue;
        };
        let dy = sh(lo).1;
        if lo > 0 && part(lo - 1) == part(lo) {
            if let Some(t) = top_s.get_mut(lo) {
                *t = min(*t, b.y0 + dy);
            }
        }
        if hi + 1 < nl && part(hi + 1) == part(hi) {
            if let Some(t) = bot_s.get_mut(hi) {
                *t = max(*t, b.y1 + dy);
            }
        }
    }
    for l in 0..nl.saturating_sub(1) {
        // A band squeezed shut falls back to the plain gap.
        if bot_s[l] >= top_s[l + 1] {
            bot_s[l] = bot[l] + sh(l).1;
            top_s[l + 1] = top[l + 1] + sh(l + 1).1;
        }
    }
    let shapes: Vec<NodeShape> = chart
        .nodes
        .iter()
        .zip(&m.size)
        .map(|(n, &(w, h))| NodeShape {
            shape: n.shape,
            w,
            h,
        })
        .collect();

    let routed = route::route(&RouteIn {
        dir,
        style: o.style,
        g,
        pos: &pos,
        layer_top: &top_s,
        layer_bot: &bot_s,
        part: part_of,
        shapes: &shapes,
        wrap: frame.as_ref(),
    });

    // Stacked labels: each group of side-by-side labelled edges between neighbouring
    // layers splits the gap into equal slots, one chip per slot, on its edge.
    let mut stacked: Vec<Option<(f64, f64)>> = vec![None; g.chains.len()];
    for (gap, group) in gap_label_groups(g, m, dir, &co.x) {
        if part(gap) != part(gap + 1) {
            continue;
        }
        let lo = bot.get(gap).copied().unwrap_or(0.0) + sh(gap).1;
        let hi = top.get(gap + 1).copied().unwrap_or(lo) + sh(gap).1;
        let k = group.len() as f64;
        for (i, &(ci, _)) in group.iter().enumerate() {
            let yi = lo + (hi - lo) * (i as f64 + 0.5) / k;
            if let (Some(slot), Some(r)) = (stacked.get_mut(ci), routed.get(ci)) {
                *slot = at_layer_y(&r.points, yi);
            }
        }
    }

    let fin = |p: (f64, f64)| route::to_final(dir, p.0, p.1);
    let ne = chart.edges.len();
    let mut edge_pts: Vec<Vec<(f64, f64)>> = vec![Vec::new(); ne];
    let mut wrap_flag = vec![false; ne];
    let mut label_at: Vec<Option<(f64, f64)>> = vec![None; ne];
    for ((ch, r), fixed) in g.chains.iter().zip(routed).zip(stacked) {
        let Some(slot) = edge_pts.get_mut(ch.edge) else {
            continue;
        };
        let mut pts: Vec<(f64, f64)> = r.points.into_iter().map(fin).collect();
        if ch.reversed {
            pts.reverse();
        }
        *slot = pts;
        wrap_flag[ch.edge] = r.wrap;
        label_at[ch.edge] = r.label.or(fixed).map(fin);
    }
    // Self-loops, with their labels beyond the outermost loop.
    for (v, loops) in base.loops.iter().enumerate() {
        let (Some(&lv), Some(shape)) = (g.real.get(v), shapes.get(v)) else {
            continue;
        };
        let (cx, cy) = pos[lv];
        let mut outer = cx;
        for (i, &e) in loops.iter().enumerate() {
            let (pts, out) = route::self_loop(dir, shape, cx, cy, i);
            outer = max(outer, out);
            if let Some(slot) = edge_pts.get_mut(e) {
                *slot = pts.into_iter().map(fin).collect();
            }
        }
        for &e in loops {
            if let Some(Some(l)) = m.edge_label.get(e) {
                let (cw, ch) = chip_size(l);
                let (lo, _) = axes(dir, cw, ch);
                label_at[e] = Some(fin((outer + LOOP_LABEL_GAP + lo / 2.0, cy)));
            }
        }
    }

    // Nodes.
    let mut nodes: Vec<NodeGeom> = Vec::with_capacity(chart.nodes.len());
    for (v, label) in m.node_label.iter().enumerate() {
        let (w, h) = m.size.get(v).copied().unwrap_or((0.0, 0.0));
        let (x, y) = g.real.get(v).map_or((0.0, 0.0), |&lv| fin(pos[lv]));
        nodes.push(NodeGeom {
            x,
            y,
            w,
            h,
            label: label.clone(),
            rank: base.rank.get(v).copied().unwrap_or(0),
        });
    }
    let node_boxes: Vec<BoxF> = nodes
        .iter()
        .map(|n| centred(n.x, n.y, n.w, n.h, 0.0))
        .collect();

    // Edge labels: fixed positions first (label dummies, self-loops), then the others
    // moved along their edge until clear.
    let mut placed: Vec<BoxF> = Vec::new();
    let mut labels: Vec<Option<EdgeLabelGeom>> = vec![None; ne];
    for e in 0..ne {
        if let (Some(p), Some(Some(l))) = (label_at[e], m.edge_label.get(e)) {
            let (cw, ch) = chip_size(l);
            placed.push(centred(p.0, p.1, cw, ch, LABEL_CLEAR / 2.0));
            labels[e] = Some(EdgeLabelGeom {
                x: p.0,
                y: p.1,
                label: l.clone(),
            });
        }
    }
    for e in 0..ne {
        if labels[e].is_some() || edge_pts[e].len() < 2 {
            continue;
        }
        let Some(Some(l)) = m.edge_label.get(e) else {
            continue;
        };
        let (cw, ch) = chip_size(l);
        if let Some(p) = place_label(&edge_pts[e], cw, ch, &node_boxes, &placed, fuel) {
            placed.push(centred(p.0, p.1, cw, ch, LABEL_CLEAR / 2.0));
            labels[e] = Some(EdgeLabelGeom {
                x: p.0,
                y: p.1,
                label: l.clone(),
            });
        }
    }

    // Clusters: boxes from phase 4, empty clusters from their placeholder (nested empty
    // clusters share the box of their outermost empty ancestor).
    let k = base.cl.len();
    let mut rects: Vec<Option<Rect>> = (0..k)
        .map(|c| {
            co.boxes.get(c).copied().flatten().map(|b| {
                let (dx, dy) = shift.get(cluster_part(c)).copied().unwrap_or((0.0, 0.0));
                Rect {
                    x0: b.x0 + dx,
                    y0: b.y0 + dy,
                    x1: b.x1 + dx,
                    y1: b.y1 + dy,
                }
            })
        })
        .collect();
    for (v, node) in g.nodes.iter().enumerate() {
        if let Kind::EmptyCluster(c) = node.kind {
            if let Some(r) = rects.get_mut(c) {
                let (x, y) = pos[v];
                *r = Some(Rect {
                    x0: x - node.left,
                    y0: y - node.thick / 2.0,
                    x1: x + node.right,
                    y1: y + node.thick / 2.0,
                });
            }
        }
    }
    let mut clusters: Vec<ClusterGeom> = Vec::with_capacity(k);
    for c in 0..k {
        let mut r = rects[c];
        let mut cur = base.cl.parent.get(c).copied().flatten();
        let mut guard = 0;
        while r.is_none() && guard <= lgraph::MAX_CLUSTER_DEPTH {
            let Some(p) = cur else { break };
            r = rects.get(p).copied().flatten();
            cur = base.cl.parent.get(p).copied().flatten();
            guard += 1;
        }
        let r = r.unwrap_or_default();
        let (ax, ay) = fin((r.x0, r.y0));
        let (bx, by) = fin((r.x1, r.y1));
        let (x, y) = (min(ax, bx), min(ay, by));
        let (w, h) = (abs(bx - ax), abs(by - ay));
        let label = m.title.get(c).cloned().unwrap_or_default();
        clusters.push(ClusterGeom {
            x,
            y,
            w,
            h,
            label_x: x + w / 2.0,
            label_y: y + TITLE_TOP + label.height / 2.0,
            label,
        });
    }

    // Translate so the drawing starts at the margin.
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut grow = |b: BoxF| {
        x0 = min(x0, b.0);
        y0 = min(y0, b.1);
        x1 = max(x1, b.2);
        y1 = max(y1, b.3);
    };
    node_boxes.iter().for_each(|&b| grow(b));
    for pts in &edge_pts {
        for &(x, y) in pts {
            grow((x, y, x, y));
        }
    }
    for l in labels.iter().flatten() {
        let (cw, ch) = chip_size(&l.label);
        grow(centred(l.x, l.y, cw, ch, 0.0));
    }
    for c in &clusters {
        grow((c.x, c.y, c.x + c.w, c.y + c.h));
    }
    if x0 > x1 {
        (x0, y0, x1, y1) = (0.0, 0.0, 0.0, 0.0);
    }
    let (dx, dy) = (MARGIN - x0, MARGIN - y0);
    for n in nodes.iter_mut() {
        n.x += dx;
        n.y += dy;
    }
    for c in clusters.iter_mut() {
        c.x += dx;
        c.y += dy;
        c.label_x += dx;
        c.label_y += dy;
    }
    let back = &base.reversed;
    let edges: Vec<EdgeGeom> = edge_pts
        .into_iter()
        .zip(labels)
        .enumerate()
        .map(|(e, (pts, label))| EdgeGeom {
            points: pts
                .into_iter()
                .map(|(x, y)| Point::new(x + dx, y + dy))
                .collect(),
            label: label.map(|mut l| {
                l.x += dx;
                l.y += dy;
                l
            }),
            back: back.get(e).copied().unwrap_or(false),
            wrap: wrap_flag[e],
        })
        .collect();
    Ok(Geometry {
        width: x1 - x0 + 2.0 * MARGIN,
        height: y1 - y0 + 2.0 * MARGIN,
        direction: dir,
        nodes,
        edges,
        clusters,
        layers: Vec::new(),
        fuel_used: 0,
    })
}

fn fits(g: &Geometry, o: &Opts) -> bool {
    g.width <= o.target_width
}

fn aspect_ok(g: &Geometry, o: &Opts) -> bool {
    g.height <= o.max_aspect * g.width
}

/// Container-fit attempts are skipped once the fuel left would not cover another
/// candidate of the size of the first one; a candidate that runs out of fuel is
/// dropped and ends the search.
struct Budget {
    unit: u64,
    done: bool,
}

impl Budget {
    fn attempt<T>(
        &mut self,
        fuel: &mut Fuel,
        f: impl FnOnce(&mut Fuel) -> Result<T, OutOfFuel>,
    ) -> Option<T> {
        if self.done || fuel.remaining() < self.unit {
            self.done = true;
            return None;
        }
        match f(fuel) {
            Ok(v) => Some(v),
            Err(OutOfFuel) => {
                self.done = true;
                None
            }
        }
    }
}

/// A laid-out candidate and what container fit needs to refine it.
struct Cand {
    geom: Geometry,
    lay: Layered,
    co: Coords,
    dir: Direction,
}

fn plain(
    base: &Base,
    m: &Meas,
    mut lay: Layered,
    dir: Direction,
    fuel: &mut Fuel,
) -> Result<Cand, OutOfFuel> {
    apply_extents(&mut lay, base, m, dir);
    let co = coordinates(base, m, &lay, dir, fuel)?;
    let zeros = vec![0usize; lay.g.layers.len()];
    let geom = finish(base, m, &lay, dir, &co, &zeros, fuel)?;
    Ok(Cand { geom, lay, co, dir })
}

/// Step 2 (`LR`/`RL`): wrap the layer sequence while the drawing is too wide and the
/// aspect ratio allows.
fn wrap_layers(
    base: &Base,
    m: &Meas,
    c: &Cand,
    budget: &mut Budget,
    fuel: &mut Fuel,
) -> Option<Geometry> {
    let g = &c.lay.g;
    let nl = g.layers.len();
    let start: Vec<f64> = (0..nl).map(|l| c.co.y[l] - c.co.thick[l] / 2.0).collect();
    let end: Vec<f64> = (0..nl).map(|l| c.co.y[l] + c.co.thick[l] / 2.0).collect();
    let mut crossing = vec![0usize; nl];
    for (s, layer) in g.layers.iter().enumerate() {
        crossing[s] = layer.iter().map(|&v| g.up[v].len()).sum();
    }
    let mut blocked = vec![false; nl];
    let mut hi = vec![0usize; base.cl.len()];
    for v in &g.nodes {
        for cc in base.cl.chain(v.cluster) {
            hi[cc] = hi[cc].max(v.layer);
        }
    }
    for (cc, lo) in c.lay.cluster_lo.iter().enumerate() {
        if let Some(lo) = *lo {
            for b in blocked.iter_mut().take(hi[cc] + 1).skip(lo + 1) {
                *b = true;
            }
        }
    }
    let mut parts: Vec<(usize, usize)> = vec![(0, nl)];
    let mut best: Option<Geometry> = None;
    for _ in 0..MAX_FIT_ROUNDS {
        // Split the part that is longest along the layer axis.
        let (i, &(a, b)) = parts.iter().enumerate().max_by(|x, y| {
            cmp_f(
                end[x.1 .1 - 1] - start[x.1 .0],
                end[y.1 .1 - 1] - start[y.1 .0],
            )
            .then(y.0.cmp(&x.0))
        })?;
        let s = fit::wrap_split(a, b, &start, &end, &crossing, &blocked)?;
        let mut next = parts.clone();
        next[i] = (a, s);
        next.insert(i + 1, (s, b));
        let mut part_of = vec![0usize; nl];
        for (p, &(a, b)) in next.iter().enumerate() {
            for po in part_of.iter_mut().take(b).skip(a) {
                *po = p;
            }
        }
        let geom = budget.attempt(fuel, |f| finish(base, m, &c.lay, c.dir, &c.co, &part_of, f))?;
        if !aspect_ok(&geom, &base.o) {
            break;
        }
        parts = next;
        let done = fits(&geom, &base.o);
        best = Some(geom);
        if done {
            break;
        }
    }
    best
}

/// Step 3 (`TB`/`BT`): split every layer wider than the target into sub-rows, each
/// row a pseudo-layer below the previous one so edges keep pointing down. Round `i`
/// splits a layer of width `w` into `ceil(w / target) + i` rows of the original
/// candidate; the first round that fits wins, and a round beyond `max_aspect` ends the
/// search (the previous result stands).
fn split_layers(base: &Base, m: &Meas, c: Cand, budget: &mut Budget, fuel: &mut Fuel) -> Cand {
    let inner = base.o.target_width - 2.0 * MARGIN;
    let g = &c.lay.g;
    let wide: Vec<(usize, f64)> = g
        .layers
        .iter()
        .enumerate()
        .filter_map(|(l, layer)| {
            // A layer's width includes the boxes of the clusters around its nodes.
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for &v in layer {
                lo = min(lo, c.co.x[v] - g.nodes[v].left);
                hi = max(hi, c.co.x[v] + g.nodes[v].right);
                for cc in base.cl.chain(g.nodes[v].cluster) {
                    if let Some(Some(b)) = c.co.boxes.get(cc) {
                        lo = min(lo, b.x0);
                        hi = max(hi, b.x1);
                    }
                }
            }
            (hi - lo > inner).then_some((l, hi - lo))
        })
        .collect();
    let prev_x: Vec<f64> = g.real.iter().map(|&v| c.co.x[v]).collect();
    let mut best: Option<Cand> = None;
    for round in 0..MAX_FIT_ROUNDS {
        let splits: Vec<(usize, Vec<Vec<usize>>)> = wide
            .iter()
            .filter_map(|&(l, w)| {
                let need = if inner > 1.0 {
                    crate::math::ceil(w / inner)
                } else {
                    f64::MAX
                };
                // Saturating float-to-int conversion; layer_rows caps it at the node count.
                let rows = (need as usize).saturating_add(round).max(2);
                fit::layer_rows(g, l, &c.co.x, rows).map(|r| (l, r))
            })
            .collect();
        if splits.is_empty() {
            break;
        }
        let new_layer = fit::apply_splits(&c.lay.layer, &splits);
        // Every moved row starts its initial order under row 0 (its keys shifted by
        // the distance between the rows' first nodes), so the dummies of edges into
        // lower rows interleave with row 0 instead of widening it to one side.
        let mut key_x = prev_x.clone();
        for (l, rows) in &splits {
            let row0 = g.layers[*l].iter().find_map(|&v| match g.nodes[v].kind {
                Kind::Real(m) => Some(m),
                _ => None,
            });
            let Some(x0) = row0.and_then(|m| prev_x.get(m).copied()) else {
                continue;
            };
            for (i, row) in rows.iter().enumerate() {
                let Some(first) = row.first().and_then(|&m| prev_x.get(m).copied()) else {
                    continue;
                };
                // Row i + 1 sorts just after row i at equal offsets.
                let tie = 1e-3 * (i + 1) as f64;
                for &m in row {
                    if let (Some(k), Some(&xm)) = (key_x.get_mut(m), prev_x.get(m)) {
                        *k = x0 + (xm - first) + tie;
                    }
                }
            }
        }
        let attempt = budget.attempt(fuel, |f| {
            let Ok(mut lay) = build_layered(base, m, c.dir, &new_layer) else {
                return Ok(None);
            };
            let keys = keys_from(&lay.g, &base.cl, &key_x, DummyKey::Lower);
            order_layers(base, &mut lay, &keys, false, f)?;
            plain(base, m, lay, c.dir, f).map(Some)
        });
        let Some(Some(next)) = attempt else { break };
        if !aspect_ok(&next.geom, &base.o) {
            break;
        }
        // More rows only help while they make the drawing narrower.
        let narrowest = best.as_ref().map_or(c.geom.width, |b| b.geom.width);
        if next.geom.width >= narrowest {
            break;
        }
        let done = fits(&next.geom, &base.o);
        best = Some(next);
        if done {
            break;
        }
    }
    best.unwrap_or(c)
}

/// Steps 1–3 of container fit for one measurement.
fn fit_steps(base: &Base, m: &Meas, first: Cand, budget: &mut Budget, fuel: &mut Fuel) -> Cand {
    if fits(&first.geom, &base.o) {
        return first;
    }
    let mut best = first;
    if base.o.auto {
        let other = if best.dir.is_horizontal() {
            Direction::TB
        } else {
            Direction::LR
        };
        let lay = best.lay.clone();
        if let Some(c) = budget.attempt(fuel, |f| plain(base, m, lay, other, f)) {
            if fits(&c.geom, &base.o) {
                return c;
            }
            if c.geom.width < best.geom.width {
                best = c;
            }
        }
    }
    if best.dir.is_horizontal() {
        if let Some(geom) = wrap_layers(base, m, &best, budget, fuel) {
            best.geom = geom;
        }
        best
    } else {
        split_layers(base, m, best, budget, fuel)
    }
}

/// Stable layout: survivors and the `I020`/`I021`/`I022` diagnostics
/// (specs/layout.md#stable-layout).
fn read_hint(
    chart: &Flowchart,
    opts: &RenderOptions,
    layer: &[usize],
    diags: &mut Diagnostics,
) -> Option<(Vec<Option<usize>>, Direction)> {
    let text = opts.hint.as_deref()?;
    let lim = opts.limits;
    let Some(h) = hint::parse(text, lim.nodes, lim.layers, lim.input_bytes) else {
        diags.emit(
            Severity::Info,
            "I022",
            Span::default(),
            "layout hint is malformed, of an unknown version, or too large; laid out afresh",
        );
        return None;
    };
    let ranks: Vec<Option<usize>> = chart
        .nodes
        .iter()
        .enumerate()
        .map(|(v, node)| {
            h.place
                .get(&node.id)
                .filter(|&&(l, _)| Some(&l) == layer.get(v))
                .map(|&(_, i)| i)
        })
        .collect();
    let n = chart.nodes.len();
    let survivors = ranks.iter().filter(|r| r.is_some()).count();
    if survivors * 2 < n {
        diags.emit(
            Severity::Info,
            "I020",
            Span::default(),
            format!(
                "layout hint discarded: {} of {} nodes survive, fewer than half",
                survivors, n
            ),
        );
        return None;
    }
    if survivors < n {
        diags.emit(
            Severity::Info,
            "I021",
            Span::default(),
            format!(
                "layout hint partial: {} of {} nodes treated as new",
                n - survivors,
                n
            ),
        );
    }
    Some((ranks, h.direction))
}

pub fn run(
    chart: &Flowchart,
    opts: &RenderOptions,
    fuel: &mut Fuel,
    diags: &mut Diagnostics,
) -> Result<Geometry, LayoutError> {
    let lim = opts.limits;
    if chart.nodes.len() > lim.nodes {
        return Err(LayoutError::TooLarge { what: "nodes" });
    }
    if chart.edges.len() > lim.edges {
        return Err(LayoutError::TooLarge { what: "edges" });
    }
    let o = Opts::new(opts);
    let n = chart.nodes.len();
    let meas = measure_all(chart, &o, o.wrap_width, diags);

    // Phase 1: cycle removal over the edges between distinct, valid nodes.
    let mut normal: Vec<usize> = Vec::new();
    let mut loops: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (e, edge) in chart.edges.iter().enumerate() {
        if edge.from >= n || edge.to >= n {
            continue;
        }
        if edge.from == edge.to {
            loops[edge.from].push(e);
        } else {
            normal.push(e);
        }
    }
    let pairs: Vec<(usize, usize)> = normal
        .iter()
        .map(|&e| (chart.edges[e].from, chart.edges[e].to))
        .collect();
    let info = acyclic::remove_cycles(n, &pairs, fuel).map_err(too_large_fuel)?;
    let mut reversed = vec![false; chart.edges.len()];
    for (k, &e) in normal.iter().enumerate() {
        reversed[e] = info.reversed.get(k).copied().unwrap_or(false);
    }

    // Phase 2: longest-path layering honouring `min_len`.
    let cap = lim.layers.saturating_add(1);
    let lp_edges: Vec<(usize, usize, usize)> = normal
        .iter()
        .map(|&e| {
            let edge = &chart.edges[e];
            let len = usize::try_from(edge.min_len).unwrap_or(cap).clamp(1, cap);
            if reversed[e] {
                (edge.to, edge.from, len)
            } else {
                (edge.from, edge.to, len)
            }
        })
        .collect();
    let layer = layering::longest_path(n, &lp_edges, fuel).map_err(too_large_fuel)?;
    if layer.iter().any(|&l| l >= lim.layers) {
        return Err(LayoutError::TooLarge { what: "layers" });
    }

    let hinted = read_hint(chart, opts, &layer, diags);
    let base_dir = match (&hinted, o.auto) {
        (Some((_, d)), true) => *d,
        _ => chart.direction,
    };
    let base = Base {
        chart,
        o,
        cl: Clusters::from_chart(chart),
        reversed,
        normal,
        loops,
        rank: info.rank.clone(),
        stable_rank: hinted.map(|(r, _)| r),
        max_nodes: lim.layered_nodes,
        max_layers: lim.layers,
    };

    // Phase 3, from a depth-first order of the dominator tree.
    let mut lay = build_layered(&base, &meas, base_dir, &layer).map_err(too_large_build)?;
    let dfs: Vec<f64> = info.dfs_key.iter().map(|&k| k as f64).collect();
    let keys = keys_from(&lay.g, &base.cl, &dfs, DummyKey::Lerp);
    order_layers(&base, &mut lay, &keys, true, fuel).map_err(too_large_fuel)?;
    let hint_layers: Vec<Vec<usize>> = lay
        .g
        .layers
        .iter()
        .map(|l| {
            l.iter()
                .filter_map(|&v| match lay.g.nodes[v].kind {
                    Kind::Real(m) => Some(m),
                    _ => None,
                })
                .collect()
        })
        .collect();

    // Phases 4–7 (mandatory), then container fit (phase 5) by trying alternatives.
    let before = fuel.used();
    let first = plain(&base, &meas, lay, base_dir, fuel).map_err(too_large_fuel)?;
    let mut budget = Budget {
        unit: fuel.used() - before,
        done: false,
    };
    let chosen = if fits(&first.geom, &base.o) {
        first
    } else {
        let lay0 = first.lay.clone();
        let fitted = fit_steps(&base, &meas, first, &mut budget, fuel);
        if fits(&fitted.geom, &base.o) {
            fitted
        } else {
            reduce_wrap(&base, &meas, lay0, base_dir, &mut budget, fuel).unwrap_or(fitted)
        }
    };
    let mut geom = chosen.geom;
    geom.layers = hint_layers;
    geom.fuel_used = fuel.used();
    Ok(geom)
}

/// Step 4: re-measure with a narrower wrap width (20 px steps, down to 120 px) and run
/// steps 1–3 again; the first candidate that fits wins.
fn reduce_wrap(
    base: &Base,
    m0: &Meas,
    lay: Layered,
    dir: Direction,
    budget: &mut Budget,
    fuel: &mut Fuel,
) -> Option<Cand> {
    let mut wrap = base.o.wrap_width;
    let mut prev = m0.clone();
    let mut scratch = Diagnostics::new(false);
    for _ in 0..MAX_FIT_ROUNDS {
        let widest = prev.widest_label();
        // Wrapping at a width no label reaches changes nothing: jump to the first step
        // below the widest label.
        let mut next = wrap - WRAP_STEP_PX;
        while next >= widest && next >= MIN_WRAP_WIDTH {
            next -= WRAP_STEP_PX;
        }
        if next < MIN_WRAP_WIDTH {
            return None;
        }
        wrap = next;
        let m = measure_all(base.chart, &base.o, wrap, &mut scratch);
        if m == prev {
            continue;
        }
        let lay = lay.clone();
        let c = budget.attempt(fuel, |f| plain(base, &m, lay, dir, f))?;
        let c = fit_steps(base, &m, c, budget, fuel);
        if fits(&c.geom, &base.o) {
            return Some(c);
        }
        prev = m;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_options_are_bounded() {
        let o = Opts::new(&RenderOptions {
            node_spacing: f64::NAN,
            rank_spacing: -5.0,
            max_aspect: f64::NAN,
            font_size: 0.0,
            wrap_width: f64::INFINITY,
            target_width: f64::NAN,
            ..RenderOptions::default()
        });
        assert_eq!(o.node_spacing, 24.0);
        assert_eq!(o.rank_spacing, 0.0);
        assert_eq!(o.max_aspect, 1.6);
        assert_eq!(o.font_size, 1.0);
        assert_eq!(o.wrap_width, 100_000.0);
        assert_eq!(o.target_width, 720.0);
    }

    #[test]
    fn label_moves_along_the_edge_off_a_node() {
        // A vertical edge from y = 0 to 100 whose midpoint is covered by a node.
        let pts = [(0.0, 0.0), (0.0, 100.0)];
        let node = (-20.0, 40.0, 20.0, 60.0);
        let mut fuel = Fuel::new(1_000_000);
        let (x, y) = place_label(&pts, 10.0, 10.0, &[node], &[], &mut fuel).unwrap();
        assert_eq!(x, 0.0);
        assert!(!boxes_overlap(
            centred(x, y, 10.0, 10.0, LABEL_CLEAR / 2.0),
            node
        ));
        // Without fuel the midpoint is used.
        let mut empty = Fuel::new(0);
        assert_eq!(
            place_label(&pts, 10.0, 10.0, &[node], &[], &mut empty),
            Some((0.0, 50.0))
        );
    }

    #[test]
    fn keys_interpolate_along_chains() {
        use super::super::lgraph::build;
        let real = [Extent::default(); 2];
        let edges = [EdgeIn {
            edge: 0,
            upper: 0,
            lower: 1,
            reversed: false,
            label: None,
        }];
        let g = build(&BuildIn {
            real: &real,
            layer: &[0, 4],
            edges: &edges,
            clusters: &Clusters::default(),
            titles: &[],
            empty_size: &[],
            max_nodes: 100,
            max_layers: 100,
        })
        .unwrap();
        let keys = keys_from(&g, &Clusters::default(), &[0.0, 8.0], DummyKey::Lerp);
        let chain: Vec<f64> = g.chains[0].nodes.iter().map(|&v| keys[v]).collect();
        assert_eq!(chain, vec![0.0, 2.0, 4.0, 6.0, 8.0]);
        let keys = keys_from(&g, &Clusters::default(), &[0.0, 8.0], DummyKey::Lower);
        let chain: Vec<f64> = g.chains[0].nodes.iter().map(|&v| keys[v]).collect();
        assert_eq!(chain, vec![0.0, 8.0, 8.0, 8.0, 8.0]);
    }
}
