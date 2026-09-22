//! State-diagram lowering (specs/state.md#lowering): a `StateMachine` becomes the same
//! graph a flowchart is, and [`layout_state`] runs [`crate::layout::layout_flowchart`]
//! over it. None of the seven phases of specs/layout.md changes for a state machine,
//! and this module adds none of its own.
//!
//! | Model | Lowered as |
//! |---|---|
//! | `Simple` state | Node, `Shape::Rect` |
//! | `Composite` state with members | Cluster, its members inside |
//! | `Composite` state with no members | Node, `Shape::Rect`, so a transition naming it lands |
//! | `Choice` | Node, `Shape::Rhombus`, empty label |
//! | `Fork`, `Join` | Node, `Shape::Fork`: the 70 × 10 bar |
//! | `Start` | Node, `Shape::FilledCircle` |
//! | `End` | Node, `Shape::FramedCircle` |
//! | `Region` | Cluster nested in its composite's cluster |
//! | `Transition` | Edge, `Stroke::Normal`, `Arrow::Arrow` at the target |
//! | `Note` | Not a graph element: reserved in its state's extent, placed after layout |
//!
//! A note reaches the layout as [`crate::model::Reserve`] on its state's node: the
//! engine keeps that room beside the node like any other extent, and [`place_notes`]
//! carves the box back out of it once the coordinates are final. So a note box overlaps
//! no node, no cluster title and no routed edge, and the layered graph is the same with
//! and without notes.
//!
//! A note on a composite state that became a cluster is reserved on, and drawn beside,
//! that cluster's first member: a cluster box has no extent of its own to grow, and the
//! reserved room is the only place a note is guaranteed to overlap nothing.

use alloc::vec::Vec;

use crate::diag::Diagnostics;
use crate::fuel::Fuel;
use crate::geometry::state::{StateGeometry, StateNote};
use crate::geometry::{chip_size, Geometry, Point};
use crate::math::{clamp, max};
use crate::model::state::{NotePlacement, StateKind, StateMachine};
use crate::model::{Arrow, Edge, Flowchart, Node, Reserve, Shape, Stroke, Style, Subgraph};
use crate::options::{Direction, DirectionOption, RenderOptions};
use crate::text::{self, LabelLayout, TextStyle, Weight};

use super::LayoutError;

/// Padding inside a note box, per side (specs/state.md#notes-2).
pub const NOTE_PAD: (f64, f64) = (10.0, 8.0);
/// Distance from a state's rect to the note box beside it.
pub const NOTE_GAP: f64 = 16.0;
/// Wrap width of note text.
pub const NOTE_WRAP: f64 = 180.0;
/// Gap between two notes on the same state.
pub const NOTE_STACK: f64 = 8.0;

/// The shape a state kind lowers to. `Composite` has none: it becomes a cluster unless
/// it holds no member, and an empty composite lowers as `Simple` does.
pub fn node_shape(kind: StateKind) -> Option<Shape> {
    match kind {
        StateKind::Simple => Some(Shape::Rect),
        StateKind::Composite => None,
        StateKind::Choice => Some(Shape::Rhombus),
        StateKind::Fork | StateKind::Join => Some(Shape::Fork),
        StateKind::Start => Some(Shape::FilledCircle),
        StateKind::End => Some(Shape::FramedCircle),
    }
}

/// What a lowered cluster stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterOrigin {
    /// `StateMachine::states[i]`, a `Composite`.
    Composite(usize),
    /// `StateMachine::regions[i]`.
    Region(usize),
}

/// The lowered graph and the maps back to the model. Every map is total over its side,
/// so the draw stage never searches for the state a lowered element came from.
#[derive(Clone, Debug, Default)]
pub struct Lowering {
    /// The graph the flowchart engine lays out.
    pub graph: Flowchart,
    /// `graph.nodes[i]` stands for `StateMachine::states[node_of[i]]`.
    pub node_of: Vec<usize>,
    /// `graph.subgraphs[i]` stands for `cluster_of[i]`.
    pub cluster_of: Vec<ClusterOrigin>,
    /// `graph.edges[i]` is `StateMachine::transitions[edge_of[i]]`.
    pub edge_of: Vec<usize>,
    /// The lowered node of every state; `None` for a composite that became a cluster.
    pub node_for: Vec<Option<usize>>,
    /// The lowered cluster of every state; `None` for everything but a composite.
    pub cluster_for: Vec<Option<usize>>,
}

impl Lowering {
    /// The node a transition, a note or the draw stage reaches `state` through: the
    /// state's own node, or the first member node, in declaration order, of the cluster
    /// it became. It scans the lowered nodes, so a caller resolving many states builds
    /// [`representatives`] once instead.
    pub fn node_at(&self, state: usize) -> Option<usize> {
        if let Some(v) = self.node_for.get(state).copied().flatten() {
            return Some(v);
        }
        let c = self.cluster_for.get(state).copied().flatten()?;
        (0..self.graph.nodes.len()).find(|&v| self.inside(v, c))
    }

    /// Whether lowered node `v` sits in cluster `c` or one nested in it.
    fn inside(&self, v: usize, c: usize) -> bool {
        let mut cur = self.graph.nodes.get(v).and_then(|n| n.subgraph);
        let mut steps = 0usize;
        while let Some(x) = cur {
            if x == c {
                return true;
            }
            steps += 1;
            if steps > MAX_DEPTH {
                return false;
            }
            cur = self.graph.subgraphs.get(x).and_then(|s| s.parent);
        }
        false
    }
}

/// [`Lowering::node_at`] for every state at once: each state's own node, or the first
/// node declared inside the cluster it became. Each composite is filled by the first
/// member that reaches it, and the walk up stops at the first ancestor already filled,
/// so the pass costs one step per state plus one per composite.
fn representatives(sm: &StateMachine, l: &Lowering) -> Vec<Option<usize>> {
    let mut rep = l.node_for.clone();
    for (v, &s) in l.node_of.iter().enumerate() {
        let mut cur = sm.states.get(s).and_then(|st| st.parent);
        let mut steps = 0usize;
        while let Some(a) = cur {
            match rep.get_mut(a) {
                Some(slot) if slot.is_none() => *slot = Some(v),
                _ => break,
            }
            steps += 1;
            if steps > MAX_DEPTH {
                break;
            }
            cur = sm.states.get(a).and_then(|st| st.parent);
        }
    }
    rep
}

/// Deepest cluster chain walked, matching the layout's own cluster limit.
const MAX_DEPTH: usize = 64;

/// A laid-out state diagram: the lowered graph, the maps back to the model, and the
/// geometry the draw stage reads.
#[derive(Clone, Debug)]
pub struct StateLayout {
    pub lowering: Lowering,
    pub geometry: StateGeometry,
}

/// Lowers `sm` onto the graph the layered engine takes. Charges one mandatory fuel unit
/// per state, transition, note and region before it allocates (specs/state.md#fuel).
pub fn lower(
    sm: &StateMachine,
    opts: &RenderOptions,
    fuel: &mut Fuel,
) -> Result<Lowering, LayoutError> {
    let units = sm
        .states
        .len()
        .saturating_add(sm.transitions.len())
        .saturating_add(sm.notes.len())
        .saturating_add(sm.regions.len()) as u64;
    if fuel.burn(units).is_err() {
        return Err(LayoutError::TooLarge { what: "fuel" });
    }
    // `nodes` bounds states and `edges` transitions, counted on the model rather than on
    // the lowered graph, where a composite state is a cluster (specs/state.md#diagnostics).
    if sm.states.len() > opts.limits.nodes {
        return Err(LayoutError::TooLarge { what: "nodes" });
    }
    if sm.transitions.len() > opts.limits.edges {
        return Err(LayoutError::TooLarge { what: "edges" });
    }

    let n = sm.states.len();
    let mut l = Lowering {
        graph: Flowchart {
            meta: sm.meta.clone(),
            direction: sm.direction,
            class_defs: sm.class_defs.clone(),
            ..Flowchart::default()
        },
        node_for: alloc::vec![None; n],
        cluster_for: alloc::vec![None; n],
        ..Lowering::default()
    };

    // A composite state holding members becomes a cluster; every other state a node.
    let mut has_member = alloc::vec![false; n];
    for s in &sm.states {
        if let Some(p) = s.parent.filter(|&p| p < n) {
            has_member[p] = true;
        }
    }
    let is_cluster =
        |i: usize| sm.states[i].kind == StateKind::Composite && has_member.get(i) == Some(&true);

    // Clusters first, a composite immediately followed by its regions, so a parent
    // always precedes its children (specs/state.md#what-the-lowering-guarantees).
    let mut region_cluster: Vec<Option<usize>> = alloc::vec![None; sm.regions.len()];
    for i in 0..n {
        if !is_cluster(i) {
            continue;
        }
        let state = &sm.states[i];
        let parent = scope_cluster(sm, &l, &region_cluster, i);
        l.cluster_for[i] = Some(l.graph.subgraphs.len());
        l.cluster_of.push(ClusterOrigin::Composite(i));
        l.graph.subgraphs.push(Subgraph {
            id: state.id.clone(),
            title: state.label.clone(),
            parent,
            nodes: Vec::new(),
            direction: state.direction,
            classes: state.classes.clone(),
            style: state.style.clone(),
            span: state.span,
        });
        let own = l.cluster_for[i];
        for (r, region) in sm.regions.iter().enumerate() {
            if region.parent != i {
                continue;
            }
            region_cluster[r] = Some(l.graph.subgraphs.len());
            l.cluster_of.push(ClusterOrigin::Region(r));
            l.graph.subgraphs.push(Subgraph {
                id: alloc::format!("{}-r{}", state.id, region.index),
                title: alloc::string::String::new(),
                parent: own,
                nodes: Vec::new(),
                direction: None,
                classes: Vec::new(),
                style: Style::default(),
                span: region.span,
            });
        }
    }

    // Nodes, in declaration order, each naming its innermost cluster.
    for i in 0..n {
        if is_cluster(i) {
            continue;
        }
        let state = &sm.states[i];
        let cluster = scope_cluster(sm, &l, &region_cluster, i);
        let v = l.graph.nodes.len();
        l.node_for[i] = Some(v);
        l.node_of.push(i);
        l.graph.nodes.push(Node {
            id: state.id.clone(),
            label: if state.kind.draws_label() {
                state.label.clone()
            } else {
                alloc::string::String::new()
            },
            shape: node_shape(state.kind).unwrap_or(Shape::Rect),
            classes: state.classes.clone(),
            style: state.style.clone(),
            link: state.link.clone(),
            subgraph: cluster,
            span: state.span,
            reserve: Reserve::default(),
        });
        if let Some(sub) = cluster.and_then(|c| l.graph.subgraphs.get_mut(c)) {
            sub.nodes.push(v);
        }
    }

    // Transitions, in source order. Both ends resolve to a node, so a transition naming
    // a composite state reaches the cluster's first member; one between a composite and
    // a state inside it has no two endpoints and is dropped.
    let rep = representatives(sm, &l);
    for (t, tr) in sm.transitions.iter().enumerate() {
        if tr.from >= n || tr.to >= n {
            continue;
        }
        if tr.from != tr.to && (nested_in(sm, tr.from, tr.to) || nested_in(sm, tr.to, tr.from)) {
            continue;
        }
        let (Some(from), Some(to)) = (
            rep.get(tr.from).copied().flatten(),
            rep.get(tr.to).copied().flatten(),
        ) else {
            continue;
        };
        if from == to && tr.from != tr.to {
            continue;
        }
        l.edge_of.push(t);
        l.graph.edges.push(Edge {
            from,
            to,
            label: tr.label.clone(),
            stroke: Stroke::Normal,
            arrow_start: Arrow::None,
            arrow_end: Arrow::Arrow,
            min_len: 1,
            style: Style::default(),
            span: tr.span,
            id: None,
            classes: Vec::new(),
        });
    }

    Ok(l)
}

/// The cluster state `i` sits directly in: its concurrency region when it has one, else
/// its parent composite's cluster, else the top level.
fn scope_cluster(
    sm: &StateMachine,
    l: &Lowering,
    region_cluster: &[Option<usize>],
    i: usize,
) -> Option<usize> {
    let state = &sm.states[i];
    let parent = state.parent?;
    match state
        .region
        .and_then(|r| region_cluster.get(r).copied().flatten())
    {
        Some(c) => Some(c),
        None => l.cluster_for.get(parent).copied().flatten(),
    }
}

/// Whether state `inner` lies inside composite state `outer`, at any depth.
fn nested_in(sm: &StateMachine, inner: usize, outer: usize) -> bool {
    let mut cur = sm.states.get(inner).and_then(|s| s.parent);
    let mut steps = 0usize;
    while let Some(p) = cur {
        if p == outer {
            return true;
        }
        steps += 1;
        if steps > MAX_DEPTH {
            return false;
        }
        cur = sm.states.get(p).and_then(|s| s.parent);
    }
    false
}

/// One measured note box, before it is placed.
#[derive(Clone, Debug)]
struct NoteBox {
    state: usize,
    placement: NotePlacement,
    label: LabelLayout,
    w: f64,
    h: f64,
}

/// Measures every note at [`NOTE_WRAP`], charging one mandatory fuel unit per byte as
/// the flowchart's own label measure does.
fn measure_notes(
    sm: &StateMachine,
    opts: &RenderOptions,
    fuel: &mut Fuel,
    diags: &mut Diagnostics,
) -> Result<Vec<NoteBox>, LayoutError> {
    let cost = sm.notes.iter().fold(0u64, |a, note| {
        a.saturating_add(u64::try_from(note.text.len()).unwrap_or(u64::MAX))
    });
    if fuel.burn(cost).is_err() {
        return Err(LayoutError::TooLarge { what: "fuel" });
    }
    let style = TextStyle {
        font_size: clamp(
            if opts.font_size.is_nan() {
                14.0
            } else {
                opts.font_size
            },
            1.0,
            1_000.0,
        ),
        weight: Weight::Regular,
        italic: false,
    };
    let tolerance = text::width_tolerance(opts.font);
    Ok(sm
        .notes
        .iter()
        .map(|note| {
            let mut label = text::layout_label(&note.text, &style, NOTE_WRAP, diags);
            label.width = max(label.width, 0.0) * tolerance;
            label.height = max(label.height, 0.0);
            NoteBox {
                state: note.state,
                placement: note.placement,
                w: label.width + 2.0 * NOTE_PAD.0,
                h: label.height + 2.0 * NOTE_PAD.1,
                label,
            }
        })
        .collect())
}

/// Extent of a box on the order and layer axes for `dir`: `None` is `direction: auto`,
/// where the engine has yet to choose and both readings are reserved.
fn axes(dir: Option<Direction>, w: f64, h: f64) -> (f64, f64) {
    match dir {
        Some(d) if d.is_horizontal() => (h, w),
        Some(_) => (w, h),
        None => (max(w, h), max(w, h)),
    }
}

/// The notes of one node and side, as indices into the measured boxes, in source order.
/// Sorting by node and side is stable, so every column keeps the order the source wrote.
fn columns(rep: &[Option<usize>], notes: &[NoteBox]) -> Vec<(usize, NotePlacement, usize)> {
    let mut cols: Vec<(usize, NotePlacement, usize)> = notes
        .iter()
        .enumerate()
        .filter_map(|(k, b)| Some((rep.get(b.state).copied().flatten()?, b.placement, k)))
        .collect();
    cols.sort_by_key(|&(v, side, _)| (v, side == NotePlacement::After));
    cols
}

/// Writes the room the notes of each state need into that state's node
/// (specs/state.md#notes-2).
fn reserve_notes(
    l: &mut Lowering,
    rep: &[Option<usize>],
    notes: &[NoteBox],
    dir: Option<Direction>,
) {
    let cols = columns(rep, notes);
    let mut at = 0usize;
    while at < cols.len() {
        let (v, side, _) = cols[at];
        let mut end = at;
        let (mut order, mut layer) = (0.0f64, 0.0f64);
        while end < cols.len() && cols[end].0 == v && cols[end].1 == side {
            let b = &notes[cols[end].2];
            let (o, t) = axes(dir, b.w, b.h);
            order = max(order, o);
            layer += t;
            end += 1;
        }
        layer += NOTE_STACK * (end - at - 1) as f64;
        if let Some(node) = l.graph.nodes.get_mut(v) {
            match side {
                NotePlacement::Before => node.reserve.before += NOTE_GAP + order,
                NotePlacement::After => node.reserve.after += NOTE_GAP + order,
            }
            node.reserve.thick = max(node.reserve.thick, layer);
        }
        at = end;
    }
}

/// How far each node's self-loops and their labels reach past it on the order axis. The
/// loops sit on the `after` side, exactly where an `after` note goes, so the note starts
/// beyond whichever is further out — both inside the room the node reserved.
fn loop_reach(geo: &Geometry, l: &Lowering) -> Vec<Option<f64>> {
    let horizontal = geo.direction.is_horizontal();
    let mut reach: Vec<Option<f64>> = alloc::vec![None; geo.nodes.len()];
    for (e, edge) in geo.edges.iter().enumerate() {
        let Some(chart) = l.graph.edges.get(e).filter(|c| c.from == c.to) else {
            continue;
        };
        let Some(slot) = reach.get_mut(chart.from) else {
            continue;
        };
        let mut outer = slot.unwrap_or(f64::MIN);
        for p in &edge.points {
            outer = max(outer, if horizontal { p.y } else { p.x });
        }
        if let Some(label) = &edge.label {
            let (cw, ch) = chip_size(&label.label);
            let far = if horizontal {
                label.y + ch / 2.0
            } else {
                label.x + cw / 2.0
            };
            outer = max(outer, far);
        }
        *slot = Some(outer);
    }
    reach
}

/// Carves every note box out of the room [`reserve_notes`] asked for: the boxes of one
/// state and side form a column across the state's rect, [`NOTE_GAP`] from whatever the
/// state already occupies and [`NOTE_STACK`] apart, in source order.
fn place_notes(
    geo: &Geometry,
    l: &Lowering,
    rep: &[Option<usize>],
    notes: &[NoteBox],
) -> Vec<StateNote> {
    let horizontal = geo.direction.is_horizontal();
    let reach = loop_reach(geo, l);
    let cols = columns(rep, notes);
    let mut out: Vec<Option<StateNote>> = alloc::vec![None; notes.len()];
    let mut at = 0usize;
    while at < cols.len() {
        let (v, side, _) = cols[at];
        let mut end = at;
        let mut span = 0.0f64;
        while end < cols.len() && cols[end].0 == v && cols[end].1 == side {
            let b = &notes[cols[end].2];
            span += if horizontal { b.w } else { b.h };
            end += 1;
        }
        span += NOTE_STACK * (end - at - 1) as f64;
        let Some(node) = geo.nodes.get(v) else {
            at = end;
            continue;
        };
        // The column runs along the layer axis, centred on the state's rect; the boxes
        // start one gap past the far edge of everything the state already occupies.
        let (centre, half, cross, cross_half) = if horizontal {
            (node.x, node.w / 2.0, node.y, node.h / 2.0)
        } else {
            (node.y, node.h / 2.0, node.x, node.w / 2.0)
        };
        let occupied = match side {
            NotePlacement::Before => cross - cross_half,
            NotePlacement::After => max(
                cross + cross_half,
                reach.get(v).copied().flatten().unwrap_or(f64::MIN),
            ),
        };
        let mut along = centre - span / 2.0;
        for &(_, _, k) in &cols[at..end] {
            let b = &notes[k];
            let (thick, order) = if horizontal { (b.w, b.h) } else { (b.h, b.w) };
            let near = match side {
                NotePlacement::Before => occupied - NOTE_GAP - order,
                NotePlacement::After => occupied + NOTE_GAP,
            };
            let (x, y) = if horizontal {
                (along, near)
            } else {
                (near, along)
            };
            let anchor = anchor_point(
                node,
                side,
                horizontal,
                clamp(along + thick / 2.0, centre - half, centre + half),
            );
            out[k] = Some(StateNote {
                index: k,
                state: b.state,
                placement: b.placement,
                x,
                y,
                w: b.w,
                h: b.h,
                label: b.label.clone(),
                anchor,
            });
            along += thick + NOTE_STACK;
        }
        at = end;
    }
    // A note whose state names nothing in the graph keeps its slot, so the geometry
    // stays one box per model note.
    out.into_iter()
        .enumerate()
        .map(|(k, placed)| {
            placed.unwrap_or_else(|| StateNote {
                index: k,
                state: notes[k].state,
                placement: notes[k].placement,
                x: 0.0,
                y: 0.0,
                w: notes[k].w,
                h: notes[k].h,
                label: notes[k].label.clone(),
                anchor: Point::default(),
            })
        })
        .collect()
}

/// Brings the note boxes inside the drawing. Container fit measures the drawing from
/// what it draws, and a node's reserved room is not drawn, so a note carved out of the
/// room at the edge of the diagram falls outside it. Everything shifts by the shortfall
/// and the extent grows to hold the boxes with [`super::MARGIN`] around them; the note
/// keeps its [`NOTE_GAP`] from its state, because state and note move together.
fn fit_notes(geo: &mut Geometry, notes: &mut [StateNote]) {
    let Some(first) = notes.first() else { return };
    let (mut lo_x, mut lo_y) = (first.x, first.y);
    let (mut hi_x, mut hi_y) = (first.x + first.w, first.y + first.h);
    for n in notes.iter() {
        lo_x = crate::math::min(lo_x, n.x);
        lo_y = crate::math::min(lo_y, n.y);
        hi_x = max(hi_x, n.x + n.w);
        hi_y = max(hi_y, n.y + n.h);
    }
    let dx = max(super::MARGIN - lo_x, 0.0);
    let dy = max(super::MARGIN - lo_y, 0.0);
    if dx > 0.0 || dy > 0.0 {
        for n in geo.nodes.iter_mut() {
            n.x += dx;
            n.y += dy;
        }
        for e in geo.edges.iter_mut() {
            for p in e.points.iter_mut() {
                p.x += dx;
                p.y += dy;
            }
            if let Some(l) = e.label.as_mut() {
                l.x += dx;
                l.y += dy;
            }
        }
        for c in geo.clusters.iter_mut() {
            c.x += dx;
            c.y += dy;
            c.label_x += dx;
            c.label_y += dy;
        }
        for n in notes.iter_mut() {
            n.x += dx;
            n.y += dy;
            n.anchor.x += dx;
            n.anchor.y += dy;
        }
    }
    geo.width = max(geo.width + dx, hi_x + dx + super::MARGIN);
    geo.height = max(geo.height + dy, hi_y + dy + super::MARGIN);
}

/// Where the dashed connector meets the state's own rect.
fn anchor_point(
    node: &crate::geometry::NodeGeom,
    side: NotePlacement,
    horizontal: bool,
    along: f64,
) -> Point {
    let sign = match side {
        NotePlacement::Before => -1.0,
        NotePlacement::After => 1.0,
    };
    if horizontal {
        Point::new(along, node.y + sign * node.h / 2.0)
    } else {
        Point::new(node.x + sign * node.w / 2.0, along)
    }
}

/// Lowers, lays the graph out with the flowchart engine, and places the notes. Hint
/// problems are `I020`/`I021`/`I022` diagnostics, never errors, exactly as for a
/// flowchart (specs/state.md#phases-and-options).
pub fn layout_state(
    sm: &StateMachine,
    opts: &RenderOptions,
    fuel: &mut Fuel,
    diags: &mut Diagnostics,
) -> Result<StateLayout, LayoutError> {
    let mut lowering = lower(sm, opts, fuel)?;
    let notes = measure_notes(sm, opts, fuel, diags)?;
    let rep = representatives(sm, &lowering);
    if !notes.is_empty() {
        // `direction: auto` leaves the engine to choose, so both readings of the order
        // axis are reserved and the placement then uses the direction it settled on.
        let dir = match opts.direction {
            DirectionOption::Auto => None,
            DirectionOption::FromSource => Some(sm.direction),
        };
        reserve_notes(&mut lowering, &rep, &notes, dir);
    }
    let mut graph = super::layout_flowchart(&lowering.graph, opts, fuel, diags)?;
    let mut placed = place_notes(&graph, &lowering, &rep, &notes);
    fit_notes(&mut graph, &mut placed);
    Ok(StateLayout {
        lowering,
        geometry: StateGeometry {
            graph,
            notes: placed,
        },
    })
}
