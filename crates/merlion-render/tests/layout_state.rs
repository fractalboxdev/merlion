//! Lowering a state machine onto the flowchart engine (specs/state.md#lowering).
//!
//! Every model here is hand-built, so these tests hold whatever the state parser does:
//! they pin the six guarantees of specs/state.md#what-the-lowering-guarantees, the
//! shapes each kind takes, the geometry of notes, clusters and fork bars, determinism,
//! container fit and the limits.

use merlion_render::diag::Diagnostics;
use merlion_render::fuel::Fuel;
use merlion_render::geometry::state::StateNote;
use merlion_render::geometry::{chip_size, ClusterGeom, Geometry, NodeGeom};
use merlion_render::layout::state::{
    layout_state, lower, node_shape, ClusterOrigin, Lowering, StateLayout, NOTE_GAP, NOTE_PAD,
    NOTE_STACK,
};
use merlion_render::layout::LayoutError;
use merlion_render::model::state::{
    Note, NotePlacement, Region, State, StateKind, StateMachine, Transition,
};
use merlion_render::model::Shape;
use merlion_render::options::{Direction, DirectionOption, Limits, RenderOptions};

// ---------------------------------------------------------------- model builder

/// Builds a `StateMachine` the way the parser does: states in declaration order, a
/// parent before its children, regions parent-first.
#[derive(Default)]
struct Build {
    sm: StateMachine,
}

impl Build {
    fn new() -> Self {
        Build::default()
    }

    fn direction(mut self, d: Direction) -> Self {
        self.sm.direction = d;
        self
    }

    /// A top-level state.
    fn state(&mut self, id: &str, kind: StateKind) -> usize {
        self.member(id, kind, None, None)
    }

    /// A state inside `parent` (and, when given, inside that parent's region).
    fn member(
        &mut self,
        id: &str,
        kind: StateKind,
        parent: Option<usize>,
        region: Option<usize>,
    ) -> usize {
        let i = self.sm.states.len();
        self.sm.states.push(State {
            id: String::from(id),
            label: if kind.draws_label() {
                String::from(id)
            } else {
                String::new()
            },
            kind,
            parent,
            region,
            ..State::default()
        });
        if let Some(p) = parent {
            self.sm.states[p].children.push(i);
        }
        if let Some(r) = region {
            self.sm.regions[r].states.push(i);
        }
        i
    }

    /// A concurrency region of composite state `parent`.
    fn region(&mut self, parent: usize) -> usize {
        let index = self
            .sm
            .regions
            .iter()
            .filter(|r| r.parent == parent)
            .count();
        self.sm.regions.push(Region {
            parent,
            index,
            states: Vec::new(),
            span: Default::default(),
        });
        self.sm.regions.len() - 1
    }

    fn edge(&mut self, from: usize, to: usize) -> usize {
        self.labelled(from, to, None)
    }

    fn labelled(&mut self, from: usize, to: usize, label: Option<&str>) -> usize {
        self.sm.transitions.push(Transition {
            from,
            to,
            label: label.map(String::from),
            span: Default::default(),
        });
        self.sm.transitions.len() - 1
    }

    fn note(&mut self, state: usize, placement: NotePlacement, text: &str) -> usize {
        self.sm.notes.push(Note {
            state,
            placement,
            text: String::from(text),
            span: Default::default(),
        });
        self.sm.notes.len() - 1
    }

    fn done(self) -> StateMachine {
        self.sm
    }
}

// ------------------------------------------------------------------- utilities

fn opts() -> RenderOptions {
    RenderOptions::default()
}

fn lowered(sm: &StateMachine) -> Lowering {
    let mut fuel = Fuel::new(opts().fuel);
    lower(sm, &opts(), &mut fuel).expect("lower")
}

fn laid_out_with(sm: &StateMachine, o: &RenderOptions) -> StateLayout {
    let mut fuel = Fuel::new(o.fuel);
    let mut diags = Diagnostics::new(false);
    layout_state(sm, o, &mut fuel, &mut diags).expect("layout")
}

fn laid_out(sm: &StateMachine) -> StateLayout {
    laid_out_with(sm, &opts())
}

#[derive(Clone, Copy, Debug)]
struct Box2 {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Box2 {
    fn of_node(n: &NodeGeom) -> Self {
        Box2 {
            x0: n.x - n.w / 2.0,
            y0: n.y - n.h / 2.0,
            x1: n.x + n.w / 2.0,
            y1: n.y + n.h / 2.0,
        }
    }
    fn of_cluster(c: &ClusterGeom) -> Self {
        Box2 {
            x0: c.x,
            y0: c.y,
            x1: c.x + c.w,
            y1: c.y + c.h,
        }
    }
    fn of_note(n: &StateNote) -> Self {
        Box2 {
            x0: n.x,
            y0: n.y,
            x1: n.x + n.w,
            y1: n.y + n.h,
        }
    }
    /// Overlapping by more than `eps` in both axes.
    fn overlaps(&self, other: &Box2, eps: f64) -> bool {
        self.x0 < other.x1 - eps
            && other.x0 < self.x1 - eps
            && self.y0 < other.y1 - eps
            && other.y0 < self.y1 - eps
    }
    fn contains(&self, other: &Box2, eps: f64) -> bool {
        self.x0 <= other.x0 + eps
            && self.y0 <= other.y0 + eps
            && self.x1 >= other.x1 - eps
            && self.y1 >= other.y1 - eps
    }
}

fn node_box(g: &Geometry, v: usize) -> Box2 {
    Box2::of_node(&g.nodes[v])
}

// -------------------------------------------- guarantee 1: ids and total maps

#[test]
fn the_lowered_ids_are_the_source_ids_and_every_map_is_total() {
    let mut b = Build::new();
    let start = b.state("root_start", StateKind::Start);
    let still = b.state("Still", StateKind::Simple);
    let end = b.state("root_end", StateKind::End);
    b.edge(start, still);
    b.edge(still, end);
    let sm = b.done();

    let l = lowered(&sm);
    assert_eq!(l.node_for.len(), sm.states.len());
    assert_eq!(l.cluster_for.len(), sm.states.len());
    assert_eq!(l.node_of.len(), l.graph.nodes.len());
    assert_eq!(l.edge_of.len(), l.graph.edges.len());
    assert_eq!(l.cluster_of.len(), l.graph.subgraphs.len());

    let ids: Vec<&str> = l.graph.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["root_start", "Still", "root_end"]);
    for (v, &s) in l.node_of.iter().enumerate() {
        assert_eq!(l.graph.nodes[v].id, sm.states[s].id);
        assert_eq!(l.node_for[s], Some(v));
    }
    // Injective: no id repeats.
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());

    // The same model lowers to the same graph every time.
    assert_eq!(lowered(&sm).graph, l.graph);
}

// ---------------------------------------------- guarantee 2: the source's order

#[test]
fn nodes_follow_state_order_and_edges_follow_transition_order() {
    let mut b = Build::new();
    let a = b.state("a", StateKind::Simple);
    let c = b.state("c", StateKind::Simple);
    let d = b.state("d", StateKind::Simple);
    b.edge(d, a);
    b.edge(a, c);
    let sm = b.done();

    let l = lowered(&sm);
    assert_eq!(l.node_of, [a, c, d]);
    assert_eq!(l.edge_of, [0, 1]);
    assert_eq!(l.graph.edges[0].from, l.node_for[d].unwrap());
    assert_eq!(l.graph.edges[0].to, l.node_for[a].unwrap());
}

#[test]
fn a_transition_label_becomes_the_edge_label() {
    let mut b = Build::new();
    let a = b.state("a", StateKind::Simple);
    let c = b.state("c", StateKind::Simple);
    b.labelled(a, c, Some("EvPressed"));
    let sm = b.done();
    let l = lowered(&sm);
    assert_eq!(l.graph.edges[0].label.as_deref(), Some("EvPressed"));
    let g = laid_out(&sm);
    assert!(g.geometry.graph.edges[0].label.is_some());
}

// ------------------------------------- guarantee 3: a parent precedes its children

#[test]
fn a_composite_precedes_its_regions_and_every_cluster_precedes_its_children() {
    let mut b = Build::new();
    let active = b.state("Active", StateKind::Composite);
    let r0 = b.region(active);
    let r1 = b.region(active);
    let num = b.member("NumLockOff", StateKind::Simple, Some(active), Some(r0));
    let caps = b.member("CapsLockOff", StateKind::Simple, Some(active), Some(r1));
    // A composite nested inside a region, itself holding a member.
    let inner = b.member("Inner", StateKind::Composite, Some(active), Some(r1));
    let leaf = b.member("Leaf", StateKind::Simple, Some(inner), None);
    b.edge(num, caps);
    b.edge(caps, leaf);
    let sm = b.done();

    let l = lowered(&sm);
    // Active, its two regions, then the composite inside region 1.
    assert_eq!(
        l.cluster_of,
        [
            ClusterOrigin::Composite(active),
            ClusterOrigin::Region(r0),
            ClusterOrigin::Region(r1),
            ClusterOrigin::Composite(inner),
        ]
    );
    for (c, sub) in l.graph.subgraphs.iter().enumerate() {
        if let Some(p) = sub.parent {
            assert!(p < c, "cluster {c} lists its parent {p} after itself");
        }
    }
    assert_eq!(l.graph.subgraphs[0].id, "Active");
    assert_eq!(l.graph.subgraphs[1].id, "Active-r0");
    assert_eq!(l.graph.subgraphs[2].id, "Active-r1");
    assert_eq!(l.graph.subgraphs[3].id, "Inner");
    assert_eq!(l.graph.subgraphs[1].title, "");
}

// -------------------------------- guarantee 4: every node names its innermost cluster

#[test]
fn every_node_names_its_innermost_cluster() {
    let mut b = Build::new();
    let outer = b.state("Outer", StateKind::Composite);
    let r0 = b.region(outer);
    let r1 = b.region(outer);
    let in_r0 = b.member("InR0", StateKind::Simple, Some(outer), Some(r0));
    let in_r1 = b.member("InR1", StateKind::Simple, Some(outer), Some(r1));
    let plain = b.state("Plain", StateKind::Simple);
    b.edge(in_r0, in_r1);
    let sm = b.done();

    let l = lowered(&sm);
    let cluster_of_state = |s: usize| l.graph.nodes[l.node_for[s].unwrap()].subgraph;
    assert_eq!(cluster_of_state(in_r0), Some(1), "the region, not Outer");
    assert_eq!(cluster_of_state(in_r1), Some(2));
    assert_eq!(cluster_of_state(plain), None);
    // Membership lists hold the direct members only.
    assert_eq!(l.graph.subgraphs[0].nodes, Vec::<usize>::new());
    assert_eq!(l.graph.subgraphs[1].nodes, [l.node_for[in_r0].unwrap()]);
    assert_eq!(l.graph.subgraphs[2].nodes, [l.node_for[in_r1].unwrap()]);
    assert_eq!(l.cluster_for[outer], Some(0));
    assert_eq!(l.cluster_for[plain], None);
    assert_eq!(
        l.node_for[outer], None,
        "a composite with members is a cluster"
    );
}

#[test]
fn a_composite_with_no_members_lowers_to_a_node() {
    let mut b = Build::new();
    let empty = b.state("Empty", StateKind::Composite);
    let other = b.state("Other", StateKind::Simple);
    b.edge(other, empty);
    let sm = b.done();

    let l = lowered(&sm);
    assert!(l.graph.subgraphs.is_empty());
    assert_eq!(l.graph.nodes[l.node_for[empty].unwrap()].shape, Shape::Rect);
    assert_eq!(l.graph.nodes[l.node_for[empty].unwrap()].label, "Empty");
    assert_eq!(l.graph.edges.len(), 1);
}

// ------------------------------------- guarantee 5: every edge has two node endpoints

#[test]
fn a_transition_naming_a_composite_connects_to_its_first_member() {
    let mut b = Build::new();
    let outside = b.state("Outside", StateKind::Simple);
    let composite = b.state("Composite", StateKind::Composite);
    let first = b.member("First", StateKind::Simple, Some(composite), None);
    let second = b.member("Second", StateKind::Simple, Some(composite), None);
    b.edge(outside, composite);
    b.edge(first, second);
    let sm = b.done();

    let l = lowered(&sm);
    assert_eq!(l.edge_of, [0, 1]);
    assert_eq!(l.graph.edges[0].to, l.node_for[first].unwrap());
    assert_eq!(l.graph.edges[0].from, l.node_for[outside].unwrap());
}

#[test]
fn a_transition_between_a_composite_and_its_own_member_is_dropped() {
    let mut b = Build::new();
    let composite = b.state("Composite", StateKind::Composite);
    let first = b.member("First", StateKind::Simple, Some(composite), None);
    let second = b.member("Second", StateKind::Simple, Some(composite), None);
    let outside = b.state("Outside", StateKind::Simple);
    b.edge(composite, first);
    b.edge(second, composite);
    b.edge(outside, composite);
    let sm = b.done();

    let l = lowered(&sm);
    assert_eq!(l.edge_of, [2], "only the transition from outside survives");
    for e in &l.graph.edges {
        assert!(e.from < l.graph.nodes.len() && e.to < l.graph.nodes.len());
    }
}

#[test]
fn a_self_transition_stays_a_self_loop() {
    let mut b = Build::new();
    let a = b.state("a", StateKind::Simple);
    b.edge(a, a);
    let sm = b.done();
    let l = lowered(&sm);
    assert_eq!(l.edge_of, [0]);
    assert_eq!(l.graph.edges[0].from, l.graph.edges[0].to);
    let g = laid_out(&sm);
    assert!(g.geometry.graph.edges[0].points.len() > 1);
}

// ------------------------------------------- guarantee 6: a note is not a graph element

#[test]
fn notes_leave_the_layered_graph_untouched() {
    let mut b = Build::new();
    let a = b.state("a", StateKind::Simple);
    let c = b.state("c", StateKind::Simple);
    let d = b.state("d", StateKind::Simple);
    b.edge(a, c);
    b.edge(a, d);
    let bare = b.done();

    let mut noted = bare.clone();
    noted.notes.push(Note {
        state: c,
        placement: NotePlacement::After,
        text: String::from("Waits for the author"),
        span: Default::default(),
    });

    let (lb, ln) = (lowered(&bare), lowered(&noted));
    assert_eq!(lb.graph, ln.graph, "lowering ignores notes entirely");

    let (gb, gn) = (laid_out(&bare), laid_out(&noted));
    assert_eq!(gb.geometry.graph.nodes.len(), gn.geometry.graph.nodes.len());
    assert_eq!(gb.geometry.graph.layers, gn.geometry.graph.layers);
    assert_eq!(gb.geometry.notes.len(), 0);
    assert_eq!(gn.geometry.notes.len(), 1);
    let _ = (a, d);
}

// ------------------------------------------------------------------- shapes

#[test]
fn each_kind_takes_its_shape_and_the_pseudo_states_draw_no_label() {
    let mut b = Build::new();
    let kinds = [
        (StateKind::Simple, Shape::Rect),
        (StateKind::Choice, Shape::Rhombus),
        (StateKind::Fork, Shape::Fork),
        (StateKind::Join, Shape::Fork),
        (StateKind::Start, Shape::FilledCircle),
        (StateKind::End, Shape::FramedCircle),
    ];
    let mut made = Vec::new();
    for (i, (kind, _)) in kinds.iter().enumerate() {
        made.push(b.state(&format!("s{i}"), *kind));
    }
    let sm = b.done();
    let l = lowered(&sm);
    for (s, (kind, shape)) in made.iter().zip(kinds.iter()) {
        let n = &l.graph.nodes[l.node_for[*s].unwrap()];
        assert_eq!(n.shape, *shape, "{kind:?}");
        assert_eq!(node_shape(*kind), Some(*shape));
        if !kind.draws_label() {
            assert_eq!(n.label, "", "{kind:?} draws a bare symbol");
        }
    }

    let g = laid_out(&sm);
    let size = |s: usize| {
        let n = &g.geometry.graph.nodes[g.lowering.node_for[s].unwrap()];
        (n.w, n.h)
    };
    assert_eq!(size(made[2]), (70.0, 10.0), "the fork bar");
    assert_eq!(size(made[3]), (70.0, 10.0), "the join bar");
    assert_eq!(size(made[4]), (14.0, 14.0), "the start disc");
    assert_eq!(size(made[5]), (20.0, 20.0), "the end ring");
}

// ------------------------------------------------------------------ clusters

#[test]
fn a_cluster_box_encloses_every_member_and_nested_cluster() {
    let mut b = Build::new();
    let start = b.state("root_start", StateKind::Start);
    let first = b.state("First", StateKind::Composite);
    let inner_start = b.member("First_start", StateKind::Start, Some(first), None);
    let second = b.member("second", StateKind::Simple, Some(first), None);
    let deep = b.member("Deep", StateKind::Composite, Some(first), None);
    let leaf = b.member("leaf", StateKind::Simple, Some(deep), None);
    b.edge(start, first);
    b.edge(inner_start, second);
    b.edge(second, leaf);
    let sm = b.done();

    let g = laid_out(&sm);
    let geo = &g.geometry.graph;
    for (v, &s) in g.lowering.node_of.iter().enumerate() {
        let mut cluster = g.lowering.graph.nodes[v].subgraph;
        let mut guard = 0;
        while let Some(c) = cluster {
            assert!(
                Box2::of_cluster(&geo.clusters[c]).contains(&node_box(geo, v), 0.01),
                "{} escapes cluster {}",
                sm.states[s].id,
                g.lowering.graph.subgraphs[c].id
            );
            cluster = g.lowering.graph.subgraphs[c].parent;
            guard += 1;
            assert!(guard < 64);
        }
    }
    // A nested cluster sits inside its parent.
    let outer = Box2::of_cluster(&geo.clusters[g.lowering.cluster_for[first].unwrap()]);
    let nested = Box2::of_cluster(&geo.clusters[g.lowering.cluster_for[deep].unwrap()]);
    assert!(outer.contains(&nested, 0.01));
}

// ------------------------------------------------------------- fork and join

#[test]
fn a_fork_bar_spans_the_branches_it_opens() {
    let mut b = Build::new();
    let start = b.state("root_start", StateKind::Start);
    let fork = b.state("fork_state", StateKind::Fork);
    let s2 = b.state("State2", StateKind::Simple);
    let s3 = b.state("State3", StateKind::Simple);
    let join = b.state("join_state", StateKind::Join);
    let s4 = b.state("State4", StateKind::Simple);
    b.edge(start, fork);
    b.edge(fork, s2);
    b.edge(fork, s3);
    b.edge(s2, join);
    b.edge(s3, join);
    b.edge(join, s4);
    let sm = b.done();

    let g = laid_out(&sm);
    let geo = &g.geometry.graph;
    let at = |s: usize| &geo.nodes[g.lowering.node_for[s].unwrap()];
    let (lo, hi) = (at(s2).x.min(at(s3).x), at(s2).x.max(at(s3).x));
    for bar in [fork, join] {
        let n = at(bar);
        assert!(
            n.x >= lo - 0.01 && n.x <= hi + 0.01,
            "the bar sits off its branches: {} not in [{lo}, {hi}]",
            n.x
        );
    }
    // The branches share the layer after the fork and before the join.
    let layer_of = |s: usize| {
        geo.layers
            .iter()
            .position(|l| l.contains(&g.lowering.node_for[s].unwrap()))
            .unwrap()
    };
    assert_eq!(layer_of(s2), layer_of(s3));
    assert!(layer_of(fork) < layer_of(s2));
    assert!(layer_of(join) > layer_of(s2));
    let _ = s4;
}

// ---------------------------------------------------------------------- notes

fn note_of(g: &StateLayout, i: usize) -> &StateNote {
    &g.geometry.notes[i]
}

#[test]
fn a_note_sits_beside_its_state_on_the_side_it_names() {
    let mut b = Build::new();
    let draft = b.state("Draft", StateKind::Simple);
    let review = b.state("Review", StateKind::Simple);
    b.edge(draft, review);
    b.note(draft, NotePlacement::Before, "Waits for the author");
    b.note(review, NotePlacement::After, "Two reviewers sign off");
    let sm = b.done();

    let g = laid_out(&sm);
    let geo = &g.geometry.graph;
    assert_eq!(g.geometry.notes.len(), 2);

    let left = note_of(&g, 0);
    let d = &geo.nodes[g.lowering.node_for[draft].unwrap()];
    assert_eq!(left.state, draft);
    assert_eq!(left.placement, NotePlacement::Before);
    assert!(
        ((d.x - d.w / 2.0) - (left.x + left.w) - NOTE_GAP).abs() < 0.01,
        "a `before` note is NOTE_GAP left of the state"
    );
    assert!(
        (left.y + left.h / 2.0 - d.y).abs() < 0.01,
        "centred on the state"
    );
    assert!(left.w >= 2.0 * NOTE_PAD.0 && left.h >= 2.0 * NOTE_PAD.1);
    assert!((left.anchor.x - (d.x - d.w / 2.0)).abs() < 0.01);

    let right = note_of(&g, 1);
    let r = &geo.nodes[g.lowering.node_for[review].unwrap()];
    assert!(
        (right.x - (r.x + r.w / 2.0) - NOTE_GAP).abs() < 0.01,
        "an `after` note is NOTE_GAP right of the state"
    );
}

#[test]
fn a_note_turns_with_the_direction() {
    let mut b = Build::new().direction(Direction::LR);
    let draft = b.state("Draft", StateKind::Simple);
    let done = b.state("Done", StateKind::Simple);
    b.edge(draft, done);
    b.note(draft, NotePlacement::Before, "Above in LR");
    let sm = b.done();

    let g = laid_out(&sm);
    assert_eq!(g.geometry.graph.direction, Direction::LR);
    let n = note_of(&g, 0);
    let d = &g.geometry.graph.nodes[g.lowering.node_for[draft].unwrap()];
    assert!(
        ((d.y - d.h / 2.0) - (n.y + n.h) - NOTE_GAP).abs() < 0.01,
        "`before` is above the state in LR"
    );
    assert!((n.x + n.w / 2.0 - d.x).abs() < 0.01);
}

#[test]
fn notes_on_one_state_stack_in_source_order_without_touching() {
    let mut b = Build::new();
    let s = b.state("Draft", StateKind::Simple);
    let t = b.state("Done", StateKind::Simple);
    b.edge(s, t);
    b.note(s, NotePlacement::After, "first");
    b.note(s, NotePlacement::After, "second");
    let sm = b.done();

    let g = laid_out(&sm);
    let (a, c) = (note_of(&g, 0), note_of(&g, 1));
    assert!((a.x - c.x).abs() < 0.01, "one column");
    assert!(
        (c.y - (a.y + a.h) - NOTE_STACK).abs() < 0.01,
        "NOTE_STACK apart, in source order"
    );
}

#[test]
fn a_note_overlaps_no_node_no_cluster_title_and_no_note() {
    let mut b = Build::new();
    let start = b.state("root_start", StateKind::Start);
    let draft = b.state("Draft", StateKind::Simple);
    let review = b.state("Review", StateKind::Composite);
    let screening = b.member("Screening", StateKind::Simple, Some(review), None);
    let decision = b.member("Decision", StateKind::Simple, Some(review), None);
    let published = b.state("Published", StateKind::Simple);
    b.edge(start, draft);
    b.edge(draft, review);
    b.edge(screening, decision);
    b.edge(decision, published);
    b.note(
        draft,
        NotePlacement::After,
        "Waits for the author to submit",
    );
    b.note(published, NotePlacement::Before, "Visible to everyone");
    b.note(screening, NotePlacement::After, "Automated");
    let sm = b.done();

    let g = laid_out(&sm);
    let geo = &g.geometry.graph;
    let boxes: Vec<Box2> = g.geometry.notes.iter().map(Box2::of_note).collect();
    for (i, nb) in boxes.iter().enumerate() {
        for v in 0..geo.nodes.len() {
            assert!(
                !nb.overlaps(&node_box(geo, v), 0.01),
                "note {i} overlaps node {}",
                g.lowering.graph.nodes[v].id
            );
        }
        for (c, cg) in geo.clusters.iter().enumerate() {
            let title = Box2 {
                x0: cg.label_x - cg.label.width / 2.0,
                y0: cg.label_y - cg.label.height / 2.0,
                x1: cg.label_x + cg.label.width / 2.0,
                y1: cg.label_y + cg.label.height / 2.0,
            };
            assert!(
                !nb.overlaps(&title, 0.01),
                "note {i} overlaps the title of cluster {}",
                g.lowering.graph.subgraphs[c].id
            );
        }
        for (j, other) in boxes.iter().enumerate() {
            if i != j {
                assert!(!nb.overlaps(other, 0.01), "notes {i} and {j} overlap");
            }
        }
        // Inside the drawing.
        assert!(nb.x0 >= -0.01 && nb.y0 >= -0.01);
        assert!(nb.x1 <= geo.width + 0.01 && nb.y1 <= geo.height + 0.01);
    }
}

#[test]
fn a_note_overlaps_no_node_of_another_component() {
    // Four components, each carrying a note that reaches well past its own states:
    // packing must keep the room the notes reserved, whichever side they take.
    let mut b = Build::new();
    let a = b.state("A", StateKind::Simple);
    let a2 = b.state("A2", StateKind::Simple);
    let c = b.state("C", StateKind::Simple);
    let d = b.state("D", StateKind::Simple);
    let e = b.state("E", StateKind::Simple);
    let f = b.state("F", StateKind::Simple);
    let g1 = b.state("G", StateKind::Simple);
    let h = b.state("H", StateKind::Simple);
    b.edge(a, a2);
    b.edge(c, d);
    b.edge(e, f);
    b.edge(g1, h);
    b.note(
        c,
        NotePlacement::Before,
        "this note is deliberately long so it reaches far left",
    );
    b.note(
        e,
        NotePlacement::After,
        "and this one reaches just as far to the right of its own state",
    );
    b.note(
        h,
        NotePlacement::Before,
        "a third note, on the second layer, left of the state it names",
    );
    let sm = b.done();

    let g = laid_out(&sm);
    let geo = &g.geometry.graph;
    let boxes: Vec<Box2> = g.geometry.notes.iter().map(Box2::of_note).collect();
    for (i, nb) in boxes.iter().enumerate() {
        for v in 0..geo.nodes.len() {
            assert!(
                !nb.overlaps(&node_box(geo, v), 0.01),
                "note {i} overlaps node {}",
                g.lowering.graph.nodes[v].id
            );
        }
        for (j, other) in boxes.iter().enumerate() {
            if i != j {
                assert!(!nb.overlaps(other, 0.01), "notes {i} and {j} overlap");
            }
        }
        assert!(nb.x0 >= -0.01 && nb.y0 >= -0.01);
        assert!(nb.x1 <= geo.width + 0.01 && nb.y1 <= geo.height + 0.01);
    }
}

#[test]
fn a_note_overlaps_no_cluster_of_another_component() {
    // The note is on a state of its own component; the other component is a composite,
    // so what the note must not cover is a cluster box rather than a bare node.
    let mut b = Build::new();
    let outer = b.state("Outer", StateKind::Composite);
    let inner = b.member("Inner", StateKind::Simple, Some(outer), None);
    let inner2 = b.member("Inner2", StateKind::Simple, Some(outer), None);
    let lone = b.state("Lone", StateKind::Simple);
    let after = b.state("After", StateKind::Simple);
    b.edge(inner, inner2);
    b.edge(lone, after);
    b.note(
        lone,
        NotePlacement::Before,
        "this note is deliberately long so it reaches far to the left",
    );
    let sm = b.done();

    let g = laid_out(&sm);
    let geo = &g.geometry.graph;
    let nb = Box2::of_note(note_of(&g, 0));
    for (c, cg) in geo.clusters.iter().enumerate() {
        assert!(
            !nb.overlaps(&Box2::of_cluster(cg), 0.01),
            "the note overlaps cluster {}",
            g.lowering.graph.subgraphs[c].id
        );
    }
    for v in 0..geo.nodes.len() {
        assert!(
            !nb.overlaps(&node_box(geo, v), 0.01),
            "the note overlaps node {}",
            g.lowering.graph.nodes[v].id
        );
    }
}

#[test]
fn a_note_clears_the_self_loop_on_its_own_state() {
    let mut b = Build::new();
    let a = b.state("Retry", StateKind::Simple);
    let c = b.state("Done", StateKind::Simple);
    b.labelled(a, a, Some("again"));
    b.edge(a, c);
    b.note(a, NotePlacement::After, "Retries until the budget runs out");
    let sm = b.done();

    let g = laid_out(&sm);
    let geo = &g.geometry.graph;
    let nb = Box2::of_note(note_of(&g, 0));
    let loop_edge = &geo.edges[0];
    for p in &loop_edge.points {
        assert!(
            p.x <= nb.x0 + 0.01,
            "the note starts left of the self-loop at {}",
            p.x
        );
    }
    if let Some(l) = &loop_edge.label {
        let (cw, _) = chip_size(&l.label);
        assert!(l.x + cw / 2.0 <= nb.x0 + 0.01, "and left of its label");
    }
}

// ------------------------------------------------------------------ no overlaps

#[test]
fn no_two_state_boxes_overlap() {
    let sm = workflow();
    let g = laid_out(&sm);
    let geo = &g.geometry.graph;
    for v in 0..geo.nodes.len() {
        for w in (v + 1)..geo.nodes.len() {
            assert!(
                !node_box(geo, v).overlaps(&node_box(geo, w), 0.01),
                "{} overlaps {}",
                g.lowering.graph.nodes[v].id,
                g.lowering.graph.nodes[w].id
            );
        }
    }
}

/// The spec's worked example: nine states, eight transitions, a composite and a note.
fn workflow() -> StateMachine {
    let mut b = Build::new();
    let start = b.state("root_start", StateKind::Start);
    let draft = b.state("Draft", StateKind::Simple);
    let submitted = b.state("Submitted", StateKind::Simple);
    let review = b.state("Review", StateKind::Composite);
    let screening = b.member("Screening", StateKind::Simple, Some(review), None);
    let decision = b.member("Decision", StateKind::Simple, Some(review), None);
    let published = b.state("Published", StateKind::Simple);
    let end = b.state("root_end", StateKind::End);
    b.edge(start, draft);
    b.labelled(draft, submitted, Some("submit"));
    b.edge(submitted, review);
    b.labelled(review, published, Some("approved"));
    b.labelled(review, draft, Some("rejected"));
    b.edge(screening, decision);
    b.edge(published, end);
    b.note(draft, NotePlacement::After, "Waits for the author");
    b.done()
}

// ---------------------------------------------------------------- determinism

#[test]
fn the_same_model_lays_out_identically_every_time() {
    let sm = workflow();
    let first = laid_out(&sm);
    let second = laid_out(&sm);
    assert_eq!(first.geometry, second.geometry);
    assert_eq!(first.lowering.graph, second.lowering.graph);

    // Fuel spent earlier does not move anything. `Geometry::fuel_used` counts the whole
    // budget handed in, so a second layout on the same `Fuel` differs there and nowhere
    // else.
    let o = opts();
    let mut fuel = Fuel::new(o.fuel);
    let mut diags = Diagnostics::new(false);
    let _ = layout_state(&sm, &o, &mut fuel, &mut diags).expect("layout");
    let mut third = layout_state(&sm, &o, &mut fuel, &mut diags).expect("layout");
    assert!(third.geometry.graph.fuel_used > first.geometry.graph.fuel_used);
    third.geometry.graph.fuel_used = first.geometry.graph.fuel_used;
    assert_eq!(third.geometry, first.geometry);
}

// -------------------------------------------------------------- container fit

#[test]
fn a_wide_machine_fits_the_target_width() {
    let mut b = Build::new().direction(Direction::LR);
    let mut prev = b.state("s0", StateKind::Simple);
    for i in 1..24 {
        let next = b.state(&format!("state{i}"), StateKind::Simple);
        b.edge(prev, next);
        prev = next;
    }
    let sm = b.done();
    let g = laid_out(&sm);
    assert!(
        g.geometry.graph.width <= opts().target_width + 0.01,
        "{} px wide",
        g.geometry.graph.width
    );
}

#[test]
fn a_narrower_target_width_is_honoured() {
    let mut b = Build::new().direction(Direction::LR);
    let mut prev = b.state("s0", StateKind::Simple);
    for i in 1..12 {
        let next = b.state(&format!("state{i}"), StateKind::Simple);
        b.edge(prev, next);
        prev = next;
    }
    let sm = b.done();
    let o = RenderOptions {
        target_width: 360.0,
        ..opts()
    };
    let g = laid_out_with(&sm, &o);
    assert!(
        g.geometry.graph.width <= 360.01,
        "{}",
        g.geometry.graph.width
    );
}

#[test]
fn direction_auto_still_lays_the_notes_out_on_the_order_axis() {
    let sm = workflow();
    let o = RenderOptions {
        direction: DirectionOption::Auto,
        ..opts()
    };
    let g = laid_out_with(&sm, &o);
    let geo = &g.geometry.graph;
    let nb = Box2::of_note(note_of(&g, 0));
    for v in 0..geo.nodes.len() {
        assert!(!nb.overlaps(&node_box(geo, v), 0.01));
    }
}

// -------------------------------------------------------------------- limits

#[test]
fn too_many_states_or_transitions_is_too_large() {
    let mut b = Build::new();
    for i in 0..8 {
        b.state(&format!("s{i}"), StateKind::Simple);
    }
    for i in 0..7 {
        b.edge(i, i + 1);
    }
    let sm = b.done();

    let mut fuel = Fuel::new(opts().fuel);
    let o = RenderOptions {
        limits: Limits {
            nodes: 4,
            ..Limits::default()
        },
        ..opts()
    };
    assert_eq!(
        lower(&sm, &o, &mut fuel).err(),
        Some(LayoutError::TooLarge { what: "nodes" })
    );

    let o = RenderOptions {
        limits: Limits {
            edges: 3,
            ..Limits::default()
        },
        ..opts()
    };
    assert_eq!(
        lower(&sm, &o, &mut fuel).err(),
        Some(LayoutError::TooLarge { what: "edges" })
    );
}

#[test]
fn lowering_runs_out_of_fuel_rather_than_allocating() {
    let mut b = Build::new();
    for i in 0..64 {
        b.state(&format!("s{i}"), StateKind::Simple);
    }
    let sm = b.done();
    let mut fuel = Fuel::new(8);
    assert_eq!(
        lower(&sm, &opts(), &mut fuel).err(),
        Some(LayoutError::TooLarge { what: "fuel" })
    );
}

#[test]
fn a_starved_layout_is_too_large_and_never_panics() {
    let sm = workflow();
    for limit in [0u64, 1, 16, 64, 256, 1024] {
        let mut fuel = Fuel::new(limit);
        let mut diags = Diagnostics::new(false);
        let r = layout_state(&sm, &opts(), &mut fuel, &mut diags);
        assert!(matches!(r, Err(LayoutError::TooLarge { .. })) || r.is_ok());
    }
}

#[test]
fn an_empty_machine_lays_out() {
    let sm = StateMachine::default();
    let g = laid_out(&sm);
    assert!(g.geometry.graph.nodes.is_empty());
    assert!(g.geometry.notes.is_empty());
    assert!(g.geometry.graph.width >= 0.0);
}

// ----------------------------------------------------------- styles and links

#[test]
fn classes_styles_and_links_reach_the_lowered_node_and_cluster() {
    let mut b = Build::new();
    let composite = b.state("Composite", StateKind::Composite);
    let member = b.member("member", StateKind::Simple, Some(composite), None);
    let other = b.state("other", StateKind::Simple);
    b.edge(member, other);
    let mut sm = b.done();
    sm.states[composite].classes.push(String::from("hot"));
    sm.states[member].classes.push(String::from("cold"));
    sm.states[member].link = Some(merlion_render::model::Link {
        url: String::from("https://example.com/"),
        target_blank: false,
    });

    let l = lowered(&sm);
    assert_eq!(l.graph.subgraphs[0].classes, ["hot"]);
    assert_eq!(l.graph.nodes[l.node_for[member].unwrap()].classes, ["cold"]);
    assert!(l.graph.nodes[l.node_for[member].unwrap()].link.is_some());
    assert_eq!(l.graph.direction, sm.direction);
}

// -------------------------------------------------------------- concurrency

#[test]
fn concurrency_regions_hold_their_own_members_and_stay_apart() {
    let mut b = Build::new();
    let active = b.state("Active", StateKind::Composite);
    let r0 = b.region(active);
    let r1 = b.region(active);
    let num_off = b.member("NumLockOff", StateKind::Simple, Some(active), Some(r0));
    let num_on = b.member("NumLockOn", StateKind::Simple, Some(active), Some(r0));
    let caps_off = b.member("CapsLockOff", StateKind::Simple, Some(active), Some(r1));
    let caps_on = b.member("CapsLockOn", StateKind::Simple, Some(active), Some(r1));
    b.labelled(num_off, num_on, Some("EvNumLockPressed"));
    b.labelled(caps_off, caps_on, Some("EvCapsLockPressed"));
    let sm = b.done();

    let g = laid_out(&sm);
    let geo = &g.geometry.graph;
    let cluster = |s: usize| {
        let c = g.lowering.graph.nodes[g.lowering.node_for[s].unwrap()]
            .subgraph
            .unwrap();
        (c, Box2::of_cluster(&geo.clusters[c]))
    };
    let (c0, box0) = cluster(num_off);
    let (c1, box1) = cluster(caps_off);
    assert_ne!(c0, c1, "each region is its own cluster");
    assert_eq!(g.lowering.cluster_of[c0], ClusterOrigin::Region(r0));
    assert_eq!(g.lowering.cluster_of[c1], ClusterOrigin::Region(r1));
    for s in [num_off, num_on] {
        assert!(box0.contains(&node_box(geo, g.lowering.node_for[s].unwrap()), 0.01));
    }
    for s in [caps_off, caps_on] {
        assert!(box1.contains(&node_box(geo, g.lowering.node_for[s].unwrap()), 0.01));
    }
    assert!(!box0.overlaps(&box1, 0.01), "the regions are drawn apart");
    let outer = Box2::of_cluster(&geo.clusters[g.lowering.cluster_for[active].unwrap()]);
    assert!(outer.contains(&box0, 0.01) && outer.contains(&box1, 0.01));
}

/// A composite that splits into regions costs two cluster levels, not one, so a source
/// at the parser's nesting limit reaches twice that depth in the layout. Every composite
/// box stays inside the one above it right up to that limit
/// (specs/state.md#concurrency, specs/architecture.md#boundaries).
#[test]
fn composites_holding_regions_nest_to_the_parser_limit_without_escaping() {
    let depth = Limits::default().nesting;
    let mut b = Build::new();
    let mut composites = Vec::new();
    let mut parent = None;
    let mut region = None;
    for i in 0..depth {
        let c = b.member(&format!("C{i}"), StateKind::Composite, parent, region);
        composites.push(c);
        let r0 = b.region(c);
        let r1 = b.region(c);
        b.member(&format!("a{i}"), StateKind::Simple, Some(c), Some(r0));
        b.member(&format!("b{i}"), StateKind::Simple, Some(c), Some(r1));
        parent = Some(c);
        region = Some(r0);
    }
    let sm = b.done();

    let g = laid_out(&sm);
    let geo = &g.geometry.graph;
    let box_of = |s: usize| Box2::of_cluster(&geo.clusters[g.lowering.cluster_for[s].unwrap()]);
    for w in composites.windows(2) {
        let (outer, inner) = (box_of(w[0]), box_of(w[1]));
        assert!(
            outer.contains(&inner, 0.01),
            "C{} {outer:?} does not hold C{} {inner:?}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn a_transition_across_a_composite_boundary_is_drawn() {
    // mermaid refuses this; Merlion routes it through the cluster boundary
    // (specs/state.md#composite-states).
    let mut b = Build::new();
    let left = b.state("Left", StateKind::Composite);
    let inner_left = b.member("a", StateKind::Simple, Some(left), None);
    let right = b.state("Right", StateKind::Composite);
    let inner_right = b.member("b", StateKind::Simple, Some(right), None);
    b.edge(inner_left, inner_right);
    let sm = b.done();

    let g = laid_out(&sm);
    assert_eq!(g.lowering.edge_of, [0]);
    assert!(g.geometry.graph.edges[0].points.len() >= 2);
    assert_eq!(
        g.lowering.graph.edges[0].from,
        g.lowering.node_for[inner_left].unwrap()
    );
    assert_eq!(
        g.lowering.graph.edges[0].to,
        g.lowering.node_for[inner_right].unwrap()
    );
}

// ------------------------------------------------------------- stable layout

#[test]
fn the_layout_hint_is_read_as_well_as_written() {
    use merlion_render::layout::hint;

    let sm = workflow();
    let first = laid_out(&sm);
    let ids: Vec<&str> = first
        .lowering
        .graph
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    let text = hint::format(
        first.geometry.graph.direction,
        &first.geometry.graph.layers,
        &ids,
    );

    let o = RenderOptions {
        hint: Some(text),
        ..opts()
    };
    let mut fuel = Fuel::new(o.fuel);
    let mut diags = Diagnostics::new(false);
    let again = layout_state(&sm, &o, &mut fuel, &mut diags).expect("layout");
    assert!(
        !diags.items.iter().any(|d| d.code == "I022"),
        "the hint the lowering writes parses: {:?}",
        diags.items
    );
    assert_eq!(again.geometry.graph.layers, first.geometry.graph.layers);

    // A hint naming states this machine does not have is reported, not fatal.
    let o = RenderOptions {
        hint: Some(String::from("v1;TB;0:nowhere,0:elsewhere")),
        ..opts()
    };
    let mut fuel = Fuel::new(o.fuel);
    let mut diags = Diagnostics::new(false);
    let fresh = layout_state(&sm, &o, &mut fuel, &mut diags).expect("layout");
    assert!(
        diags.items.iter().any(|d| d.code.starts_with("I02")),
        "{:?}",
        diags.items
    );
    assert_eq!(fresh.geometry.graph.layers, first.geometry.graph.layers);
}
