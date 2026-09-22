//! Drawing state diagrams (specs/state.md#svg-output, #roles-and-automatic-tones,
//! #text-alternative).
//!
//! The model, the lowering and the geometry are hand-built, so these assertions hold
//! whatever the parser and the layout solver produce.

mod svg_support;

use merlion_render::diag::{Diagnostics, Span};
use merlion_render::geometry::state::{StateGeometry, StateNote};
use merlion_render::geometry::{ClusterGeom, EdgeGeom, EdgeLabelGeom, Geometry, NodeGeom, Point};
use merlion_render::layout::state::{ClusterOrigin, Lowering, StateLayout};
use merlion_render::model::state::{
    Note, NotePlacement, Region, State, StateKind, StateMachine, Transition,
};
use merlion_render::model::{Arrow, Edge, Flowchart, Node, Shape, Stroke, Subgraph};
use merlion_render::svg::{draw_state, outline_state};
use merlion_render::text::{LabelLayout, Line, Run, Weight};
use merlion_render::{Direction, RenderOptions};

use svg_support::{all_attrs, assert_safe, assert_well_formed, style_text};

/// The default light theme's `--merlion-fg`, which the pseudo-states draw in.
const INK: &str = "#1f2328";

// ------------------------------------------------------------------ tiny builders

fn label(text: &str) -> LabelLayout {
    if text.is_empty() {
        return LabelLayout::default();
    }
    let width = 7.0 * text.chars().count() as f64;
    LabelLayout {
        lines: vec![Line {
            runs: vec![Run {
                text: String::from(text),
                weight: Weight::Regular,
                italic: false,
                code: false,
                width,
            }],
            width,
            size: 14.0,
            detail: false,
            height: 20.0,
            ascent: 15.0,
        }],
        width,
        height: 20.0,
        line_height: 20.0,
        ascent: 15.0,
    }
}

fn state(id: &str, kind: StateKind) -> State {
    State {
        id: String::from(id),
        label: if kind.draws_label() {
            String::from(id)
        } else {
            String::new()
        },
        kind,
        ..State::default()
    }
}

fn child(id: &str, kind: StateKind, parent: usize, region: Option<usize>) -> State {
    State {
        parent: Some(parent),
        region,
        ..state(id, kind)
    }
}

fn transition(from: usize, to: usize, text: Option<&str>) -> Transition {
    Transition {
        from,
        to,
        label: text.map(String::from),
        span: Span::default(),
    }
}

fn note(st: usize, placement: NotePlacement, text: &str) -> Note {
    Note {
        state: st,
        placement,
        text: String::from(text),
        span: Span::default(),
    }
}

fn node(id: &str, shape: Shape, text: &str) -> Node {
    Node {
        id: String::from(id),
        label: String::from(text),
        shape,
        classes: Vec::new(),
        style: Default::default(),
        link: None,
        subgraph: None,
        span: Span::default(),
        reserve: Default::default(),
    }
}

fn in_cluster(mut n: Node, sg: usize) -> Node {
    n.subgraph = Some(sg);
    n
}

fn edge(from: usize, to: usize, text: Option<&str>) -> Edge {
    Edge {
        from,
        to,
        label: text.map(String::from),
        stroke: Stroke::Normal,
        arrow_start: Arrow::None,
        arrow_end: Arrow::Arrow,
        min_len: 1,
        style: Default::default(),
        span: Span::default(),
        id: None,
        classes: Vec::new(),
    }
}

fn subgraph(id: &str, title: &str, parent: Option<usize>, nodes: Vec<usize>) -> Subgraph {
    Subgraph {
        id: String::from(id),
        title: String::from(title),
        parent,
        nodes,
        direction: None,
        classes: Vec::new(),
        style: Default::default(),
        span: Span::default(),
    }
}

fn node_geom(x: f64, y: f64, w: f64, h: f64, text: &str, rank: u8) -> NodeGeom {
    NodeGeom {
        x,
        y,
        w,
        h,
        label: label(text),
        rank,
    }
}

fn cluster_geom(x: f64, y: f64, w: f64, h: f64, title: &str) -> ClusterGeom {
    ClusterGeom {
        x,
        y,
        w,
        h,
        label: label(title),
        label_x: x + w / 2.0,
        label_y: y + 10.0,
    }
}

fn edge_geom(a: (f64, f64), b: (f64, f64)) -> EdgeGeom {
    EdgeGeom {
        points: vec![Point::new(a.0, a.1), Point::new(b.0, b.1)],
        label: None,
        back: false,
        wrap: false,
    }
}

fn labelled(mut g: EdgeGeom, text: &str) -> EdgeGeom {
    let mid = (
        (g.points[0].x + g.points[1].x) / 2.0,
        (g.points[0].y + g.points[1].y) / 2.0,
    );
    g.label = Some(EdgeLabelGeom {
        x: mid.0,
        y: mid.1,
        label: label(text),
    });
    g
}

// --------------------------------------------------------------------- the fixture
//
// ```
// stateDiagram-v2
//   [*] --> Draft
//   Draft --> if_state : check
//   state if_state <<choice>>
//   if_state --> Machine : yes
//   state Machine {
//     On
//     --
//     Off
//   }
//   fork_state --> join_state
//   Machine --> [*]
//   note right of Draft : Waits for the author
//   note left of Draft : Second
// ```
//
// States 0 `root_start`, 1 `Draft`, 2 `if_state`, 3 `Machine` (composite, two regions),
// 4 `On`, 5 `Off`, 6 `fork_state`, 7 `join_state`, 8 `root_end`.

const MACHINE: usize = 3;

fn machine() -> StateMachine {
    StateMachine {
        direction: Direction::TB,
        states: vec![
            state("root_start", StateKind::Start),
            state("Draft", StateKind::Simple),
            state("if_state", StateKind::Choice),
            State {
                children: vec![4, 5],
                ..state("Machine", StateKind::Composite)
            },
            child("On", StateKind::Simple, MACHINE, Some(0)),
            child("Off", StateKind::Simple, MACHINE, Some(1)),
            state("fork_state", StateKind::Fork),
            state("join_state", StateKind::Join),
            state("root_end", StateKind::End),
        ],
        transitions: vec![
            transition(0, 1, None),
            transition(1, 2, Some("check")),
            transition(2, MACHINE, Some("yes")),
            transition(6, 7, None),
            transition(MACHINE, 8, None),
        ],
        notes: vec![
            note(1, NotePlacement::After, "Waits for the author"),
            note(1, NotePlacement::Before, "Second"),
        ],
        regions: vec![
            Region {
                parent: MACHINE,
                index: 0,
                states: vec![4],
                span: Span::default(),
            },
            Region {
                parent: MACHINE,
                index: 1,
                states: vec![5],
                span: Span::default(),
            },
        ],
        ..StateMachine::default()
    }
}

/// The lowering the layout workstream produces for [`machine`]: the composite is
/// cluster 0, its two regions clusters 1 and 2, and every other state a node in
/// declaration order (specs/state.md#lowering).
fn machine_lowering() -> Lowering {
    let graph = Flowchart {
        direction: Direction::TB,
        nodes: vec![
            node("root_start", Shape::FilledCircle, ""),
            node("Draft", Shape::Rect, "Draft"),
            node("if_state", Shape::Rhombus, ""),
            in_cluster(node("On", Shape::Rect, "On"), 1),
            in_cluster(node("Off", Shape::Rect, "Off"), 2),
            node("fork_state", Shape::Fork, ""),
            node("join_state", Shape::Fork, ""),
            node("root_end", Shape::FramedCircle, ""),
        ],
        edges: vec![
            edge(0, 1, None),
            edge(1, 2, Some("check")),
            // A transition naming a composite connects to the cluster's first member.
            edge(2, 3, Some("yes")),
            edge(5, 6, None),
            edge(3, 7, None),
        ],
        subgraphs: vec![
            subgraph("Machine", "Machine", None, vec![]),
            subgraph("Machine-r0", "", Some(0), vec![3]),
            subgraph("Machine-r1", "", Some(0), vec![4]),
        ],
        ..Flowchart::default()
    };
    Lowering {
        graph,
        node_of: vec![0, 1, 2, 4, 5, 6, 7, 8],
        cluster_of: vec![
            ClusterOrigin::Composite(MACHINE),
            ClusterOrigin::Region(0),
            ClusterOrigin::Region(1),
        ],
        edge_of: vec![0, 1, 2, 3, 4],
        node_for: vec![
            Some(0),
            Some(1),
            Some(2),
            None,
            Some(3),
            Some(4),
            Some(5),
            Some(6),
            Some(7),
        ],
        cluster_for: vec![None, None, None, Some(0), None, None, None, None, None],
    }
}

fn machine_geometry() -> StateGeometry {
    let graph = Geometry {
        width: 420.0,
        height: 420.0,
        direction: Direction::TB,
        nodes: vec![
            node_geom(60.0, 30.0, 14.0, 14.0, "", 0),
            node_geom(60.0, 90.0, 80.0, 40.0, "Draft", 1),
            node_geom(60.0, 160.0, 60.0, 40.0, "", 2),
            node_geom(150.0, 242.0, 70.0, 40.0, "On", 3),
            node_geom(150.0, 302.0, 70.0, 40.0, "Off", 4),
            node_geom(320.0, 90.0, 70.0, 10.0, "", 1),
            node_geom(320.0, 160.0, 70.0, 10.0, "", 2),
            node_geom(60.0, 390.0, 20.0, 20.0, "", 5),
        ],
        edges: vec![
            edge_geom((60.0, 37.0), (60.0, 70.0)),
            labelled(edge_geom((60.0, 110.0), (60.0, 140.0)), "check"),
            labelled(edge_geom((60.0, 180.0), (150.0, 222.0)), "yes"),
            edge_geom((320.0, 95.0), (320.0, 155.0)),
            edge_geom((150.0, 322.0), (60.0, 380.0)),
        ],
        clusters: vec![
            cluster_geom(100.0, 205.0, 120.0, 130.0, "Machine"),
            cluster_geom(110.0, 222.0, 100.0, 45.0, ""),
            cluster_geom(110.0, 282.0, 100.0, 45.0, ""),
        ],
        layers: vec![vec![0], vec![1, 5], vec![2, 6], vec![3], vec![4], vec![7]],
        fuel_used: 0,
    };
    StateGeometry {
        graph,
        notes: vec![
            StateNote {
                index: 0,
                state: 1,
                placement: NotePlacement::After,
                x: 160.0,
                y: 70.0,
                w: 140.0,
                h: 40.0,
                label: label("Waits for the author"),
                anchor: Point::new(100.0, 90.0),
            },
            StateNote {
                index: 1,
                state: 1,
                placement: NotePlacement::Before,
                x: 160.0,
                y: 118.0,
                w: 80.0,
                h: 36.0,
                label: label("Second"),
                anchor: Point::new(100.0, 96.0),
            },
        ],
    }
}

fn machine_layout() -> StateLayout {
    StateLayout {
        lowering: machine_lowering(),
        geometry: machine_geometry(),
    }
}

fn draw(sm: &StateMachine, layout: &StateLayout, opts: &RenderOptions) -> String {
    let mut diags = Diagnostics::new(false);
    draw_state(sm, layout, opts, "mstate", &mut diags).svg
}

fn drawn() -> String {
    draw(&machine(), &machine_layout(), &RenderOptions::default())
}

/// The `<g …>` opening tag whose attributes contain `needle`.
fn group_with<'a>(svg: &'a str, needle: &str) -> &'a str {
    let mut rest = svg;
    while let Some(i) = rest.find("<g ") {
        let end = rest[i..].find('>').expect("unterminated <g") + i;
        let tag = &rest[i..=end];
        if tag.contains(needle) {
            return tag;
        }
        rest = &rest[end + 1..];
    }
    panic!("no <g> containing {needle:?} in\n{svg}");
}

// ------------------------------------------------------------------ output contract

#[test]
fn the_drawing_is_well_formed_and_safe() {
    let svg = drawn();
    assert_well_formed(&svg);
    assert_safe(&svg, "mstate");
    assert!(svg.contains("class=\"merlion merlion-state\""), "{svg}");
}

#[test]
fn hostile_labels_titles_and_ids_are_escaped() {
    let hostile = "</text><script>alert(1)</script>\" onload=\"x\u{202E}\u{0}]]>";
    let mut sm = machine();
    sm.meta.acc_title = Some(String::from(hostile));
    sm.states[1].id = String::from(hostile);
    sm.states[1].label = String::from(hostile);
    sm.notes[0].text = String::from(hostile);

    let mut layout = machine_layout();
    layout.lowering.graph.nodes[1].id = String::from(hostile);
    layout.lowering.graph.nodes[1].label = String::from(hostile);
    layout.lowering.graph.subgraphs[0].id = String::from(hostile);
    layout.lowering.graph.subgraphs[0].title = String::from(hostile);
    layout.geometry.graph.nodes[1].label = label(hostile);
    layout.geometry.graph.clusters[0].label = label(hostile);
    layout.geometry.notes[0].label = label(hostile);

    let svg = draw(&sm, &layout, &RenderOptions::default());
    assert_well_formed(&svg);
    assert_safe(&svg, "mstate");
    assert!(!svg.contains("<script"), "{svg}");
    assert!(!svg.contains("onload=\"x"), "{svg}");
    assert!(!svg.contains('\u{202E}') && !svg.contains('\u{0}'), "{svg}");
}

#[test]
fn a_hostile_model_never_panics() {
    // Indices out of range, a cyclic cluster tree, non-finite geometry and maps shorter
    // than the graph: every one is dropped rather than drawn (specs/security.md).
    let mut sm = machine();
    sm.notes.push(note(99, NotePlacement::After, "dangling"));
    sm.regions[1].parent = 99;

    let mut layout = machine_layout();
    layout.lowering.node_of = vec![99];
    layout.lowering.edge_of = vec![];
    layout.lowering.cluster_of = vec![ClusterOrigin::Region(99)];
    layout.lowering.graph.subgraphs[0].parent = Some(2);
    layout.lowering.graph.edges[0].from = 400;
    layout.geometry.graph.width = f64::NAN;
    layout.geometry.graph.nodes[0].x = f64::INFINITY;
    layout.geometry.notes[0].w = f64::NAN;
    layout.geometry.notes.push(StateNote {
        index: 9,
        state: 400,
        placement: NotePlacement::Before,
        x: f64::NAN,
        y: f64::NEG_INFINITY,
        w: -10.0,
        h: -10.0,
        label: label("x"),
        anchor: Point::new(f64::NAN, 0.0),
    });

    let svg = draw(&sm, &layout, &RenderOptions::default());
    assert_well_formed(&svg);
    assert_safe(&svg, "mstate");
}

#[test]
fn drawing_is_deterministic() {
    let (sm, layout) = (machine(), machine_layout());
    let opts = RenderOptions::default();
    assert_eq!(draw(&sm, &layout, &opts), draw(&sm, &layout, &opts));
}

// ------------------------------------------------------- groups and data attributes

#[test]
fn a_state_group_carries_the_node_classes_its_kind_and_its_id() {
    let svg = drawn();
    let g = group_with(&svg, "data-merlion-id=\"Draft\"");
    assert!(
        g.contains("class=\"merlion-node merlion-state-node\""),
        "{g}"
    );
    assert!(g.contains("data-merlion-kind=\"simple\""), "{g}");
    assert!(g.contains("data-merlion-rank=\"1\""), "{g}");
    // `{id}-n{k}` is the lowered node index (specs/state.md#groups-and-data-attributes).
    assert!(g.contains("id=\"mstate-n1\""), "{g}");
}

#[test]
fn every_pseudo_state_carries_its_own_class_and_kind() {
    let svg = drawn();
    for (id, class, kind) in [
        ("root_start", "merlion-state-start", "start"),
        ("root_end", "merlion-state-end", "end"),
        ("fork_state", "merlion-state-bar", "fork"),
        ("join_state", "merlion-state-bar", "join"),
        ("if_state", "merlion-state-choice", "choice"),
    ] {
        let g = group_with(&svg, &format!("data-merlion-id=\"{id}\""));
        assert!(
            g.contains(&format!("merlion-node merlion-state-node {class}")),
            "{id}: {g}"
        );
        assert!(g.contains(&format!("data-merlion-kind=\"{kind}\"")), "{g}");
    }
}

#[test]
fn a_transition_carries_the_edge_classes_its_ends_and_its_index() {
    let svg = drawn();
    let g = group_with(&svg, "id=\"mstate-e1\"");
    assert!(
        g.contains("class=\"merlion-edge merlion-transition\""),
        "{g}"
    );
    assert!(g.contains("data-merlion-index=\"1\""), "{g}");
    assert!(g.contains("data-merlion-from=\"Draft\""), "{g}");
    assert!(g.contains("data-merlion-to=\"if_state\""), "{g}");
    assert!(g.contains("id=\"mstate-e1\""), "{g}");
    assert!(svg.contains("class=\"merlion-edge-path\""), "{svg}");
    // The transition text takes the edge-label chip.
    assert!(svg.contains("class=\"merlion-edge-label-bg\""), "{svg}");
}

#[test]
fn a_composite_state_is_a_cluster_and_a_region_is_not() {
    let svg = drawn();
    let c = group_with(&svg, "data-merlion-id=\"Machine\"");
    assert!(
        c.contains("class=\"merlion-cluster merlion-composite"),
        "{c}"
    );
    assert!(svg.contains("class=\"merlion-cluster-box\""), "{svg}");
    assert!(svg.contains("class=\"merlion-cluster-title\""), "{svg}");

    // A region carries neither a box nor a title, so the viewer's cluster gestures
    // never target it (specs/state.md#groups-and-data-attributes).
    let r = group_with(&svg, "data-merlion-id=\"Machine-r1\"");
    assert!(r.contains("class=\"merlion-region\""), "{r}");
    assert!(!r.contains("merlion-cluster"), "{r}");
    assert!(r.contains("data-merlion-index=\"1\""), "{r}");
    assert_eq!(svg.matches("class=\"merlion-cluster-box\"").count(), 1);
}

/// The `d` of the one `.merlion-region-divider` in `svg`.
fn divider_d(svg: &str) -> &str {
    let i = svg
        .find("class=\"merlion-region-divider\"")
        .unwrap_or_else(|| panic!("no divider in\n{svg}"));
    svg[i..]
        .split(" d=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("d")
}

#[test]
fn a_divider_marks_every_region_after_the_first() {
    let svg = drawn();
    assert_eq!(svg.matches("class=\"merlion-region-divider\"").count(), 1);
    assert!(svg.contains("stroke-dasharray=\"4 4\""), "{svg}");
    // Every drawn `d` is a finite path: the divider never writes `NaN`.
    for (tag, name, value) in all_attrs(&svg) {
        if tag == "path" && name == "d" {
            assert!(!value.contains("NaN") && !value.contains("inf"), "{value}");
        }
    }
}

/// The divider crosses the gap the two region boxes leave, on whichever axis they are
/// apart on, and spans the composite's inner extent on the other one. The layered engine
/// puts sibling clusters beside one another on the order axis, so `machine_geometry`'s
/// regions sit side by side; stacked regions are the other arrangement the same rule
/// covers (specs/state.md#groups-and-data-attributes).
#[test]
fn the_divider_crosses_the_gap_between_the_two_region_boxes() {
    // Regions side by side: the divider is vertical, spanning the composite's height.
    let mut layout = machine_layout();
    layout.geometry.graph.clusters[1] = cluster_geom(110.0, 222.0, 45.0, 105.0, "");
    layout.geometry.graph.clusters[2] = cluster_geom(165.0, 222.0, 45.0, 105.0, "");
    let svg = draw(&machine(), &layout, &RenderOptions::default());
    assert_eq!(divider_d(&svg), "M160 217L160 323");

    // Regions stacked: the divider is horizontal, spanning the composite's width.
    let mut layout = machine_layout();
    layout.geometry.graph.clusters[1] = cluster_geom(110.0, 222.0, 100.0, 45.0, "");
    layout.geometry.graph.clusters[2] = cluster_geom(110.0, 282.0, 100.0, 45.0, "");
    let svg = draw(&machine(), &layout, &RenderOptions::default());
    assert_eq!(divider_d(&svg), "M112 274.5L208 274.5");

    // Two boxes that overlap on both axes have no boundary: nothing is drawn rather
    // than a line through the states.
    let mut layout = machine_layout();
    layout.geometry.graph.clusters[1] = cluster_geom(110.0, 222.0, 100.0, 100.0, "");
    layout.geometry.graph.clusters[2] = cluster_geom(120.0, 232.0, 100.0, 100.0, "");
    let svg = draw(&machine(), &layout, &RenderOptions::default());
    assert_eq!(svg.matches("class=\"merlion-region-divider\"").count(), 0);
}

#[test]
fn notes_draw_last_with_a_dashed_connector() {
    let svg = drawn();
    let g = group_with(&svg, "data-merlion-placement=\"after\"");
    assert!(g.contains("class=\"merlion-note\""), "{g}");
    assert!(g.contains("data-merlion-id=\"Draft\""), "{g}");
    assert!(svg.contains("class=\"merlion-note-link\""), "{svg}");
    assert!(svg.contains("class=\"merlion-note-box\""), "{svg}");
    assert_eq!(svg.matches("class=\"merlion-note\"").count(), 2);

    // Draw order: within each level clusters come first, then its edges, then its
    // nodes; the notes come after everything (specs/state.md#groups-and-data-attributes).
    let at = |s: &str| svg.find(s).unwrap_or_else(|| panic!("{s}\n{svg}"));
    assert!(at("class=\"merlion-cluster-box\"") < at("data-merlion-id=\"On\""));
    assert!(at("id=\"mstate-e0\"") < at("id=\"mstate-n0\""));
    let last_node = svg.rfind("class=\"merlion-node").expect("node");
    assert!(last_node < at("class=\"merlion-note\""), "{svg}");
    // The connector is dashed and starts at the anchor on the state's rect.
    let i = at("class=\"merlion-note-link\"");
    let tag = &svg[i..i + svg[i..].find("/>").expect("close")];
    assert!(tag.contains("stroke-dasharray=\"4 4\""), "{tag}");
    assert!(tag.contains("d=\"M100 90L160 90\""), "{tag}");
}

// -------------------------------------------------------- tones, roles and the style

#[test]
fn the_ink_marks_are_solid_and_take_no_tone() {
    let svg = drawn();
    // specs/state.md#theme-tokens: `--merlion-fg` fill through the element class.
    let css = style_text(&svg);
    assert!(
        css.contains(".merlion-state-start>.merlion-shape") && css.contains("--merlion-fg"),
        "{css}"
    );
    // The presentation attribute carries the same ink, so a CSS-less renderer agrees.
    let start = group_with(&svg, "data-merlion-id=\"root_start\"");
    let after = &svg[svg.find(start).unwrap() + start.len()..];
    let shape = &after[..after.find("/>").expect("shape")];
    assert!(shape.contains(&format!("fill=\"{INK}\"")), "{shape}");
}

#[test]
fn the_state_rules_are_scoped_to_the_root_id() {
    let svg = drawn();
    let css = style_text(&svg);
    for sel in [
        ".merlion-state-start>.merlion-shape",
        ".merlion-note>.merlion-note-box",
        ".merlion-note>.merlion-note-link",
        ".merlion-region>.merlion-region-divider",
    ] {
        assert!(
            css.contains(&format!("#mstate {sel}")),
            "missing {sel} in {css}"
        );
    }
}

#[test]
fn the_flowchart_tone_table_tones_a_choice_and_a_top_level_composite() {
    // specs/state.md#roles-and-automatic-tones: no rule of its own — the shapes the
    // lowering picks already tone through the flowchart table.
    let svg = drawn();
    let choice = group_with(&svg, "data-merlion-id=\"if_state\"");
    assert!(choice.contains("merlion-c-warn"), "{choice}");
    assert!(choice.contains("merlion-auto"), "{choice}");
    let composite = group_with(&svg, "data-merlion-id=\"Machine\"");
    assert!(composite.contains("merlion-cc-series-1"), "{composite}");
    let region = group_with(&svg, "data-merlion-id=\"Machine-r1\"");
    assert!(!region.contains("merlion-cc-series"), "{region}");
    // A simple state and the ink marks take none.
    let draft = group_with(&svg, "data-merlion-id=\"Draft\"");
    assert!(!draft.contains("merlion-c-"), "{draft}");
}

#[test]
fn auto_tone_off_removes_every_automatic_role() {
    let opts = RenderOptions {
        auto_tone: false,
        ..RenderOptions::default()
    };
    let svg = draw(&machine(), &machine_layout(), &opts);
    assert!(!svg.contains("merlion-auto"), "{svg}");
    assert!(!svg.contains("merlion-c-warn"), "{svg}");
}

// --------------------------------------------------------------------- the outline

#[test]
fn the_outline_is_the_spec_example() {
    // specs/state.md#text-alternative, verbatim.
    let sm = review_machine();
    assert_eq!(
        outline_state(&sm),
        "State diagram, top to bottom. 9 states, 8 transitions.\n\
         start → Draft\n\
         Draft → Submitted [submit]\n\
         Submitted → Review\n\
         Review → Published [approved]; → Draft [rejected]\n\
         Review: start → Screening\n\
         Review: Screening → Decision\n\
         Review: Decision\n\
         Published → end\n\
         Note right of Draft: Waits for the author"
    );
}

#[test]
fn the_outline_names_a_choice_fork_and_join_by_their_kind() {
    let sm = machine();
    let outline = outline_state(&sm);
    assert!(
        outline.contains("if_state (choice) → Machine [yes]"),
        "{outline}"
    );
    assert!(
        outline.contains("fork_state (fork) → join_state (join)"),
        "{outline}"
    );
    assert!(
        outline.starts_with("State diagram, top to bottom. 9 states, 5 transitions.\n"),
        "{outline}"
    );
    // A note prints its placement, its state and its text on one line.
    assert!(
        outline.contains("\nNote left of Draft: Second"),
        "{outline}"
    );
}

#[test]
fn the_outline_reads_the_model_not_the_lowered_graph() {
    // A transition naming a composite prints that state, not its first member.
    let outline = outline_state(&machine());
    assert!(outline.contains("→ Machine [yes]"), "{outline}");
    assert!(!outline.contains("→ On"), "{outline}");
    // The drawn `<desc>` carries the same text.
    let svg = drawn();
    let head = "<desc id=\"mstate-desc\">";
    let a = svg.find(head).expect("desc") + head.len();
    let b = svg[a..].find("</desc>").expect("/desc") + a;
    assert!(
        svg[a..b].starts_with("State diagram, top to bottom."),
        "{}",
        &svg[a..b]
    );
}

#[test]
fn acc_descr_replaces_the_outline_in_desc_only() {
    let mut sm = machine();
    sm.meta.acc_descr = Some(String::from("Two machines"));
    let svg = draw(&sm, &machine_layout(), &RenderOptions::default());
    assert!(
        svg.contains("<desc id=\"mstate-desc\">Two machines</desc>"),
        "{svg}"
    );
    assert!(outline_state(&sm).starts_with("State diagram,"));
}

/// The state machine of the spec's text-alternative example.
fn review_machine() -> StateMachine {
    StateMachine {
        direction: Direction::TB,
        states: vec![
            state("root_start", StateKind::Start),
            state("Draft", StateKind::Simple),
            state("Submitted", StateKind::Simple),
            State {
                children: vec![4, 5, 6],
                ..state("Review", StateKind::Composite)
            },
            child("Review_start", StateKind::Start, 3, None),
            child("Screening", StateKind::Simple, 3, None),
            child("Decision", StateKind::Simple, 3, None),
            state("Published", StateKind::Simple),
            state("root_end", StateKind::End),
        ],
        transitions: vec![
            transition(0, 1, None),
            transition(1, 2, Some("submit")),
            transition(2, 3, None),
            transition(4, 5, None),
            transition(5, 6, None),
            transition(3, 7, Some("approved")),
            transition(3, 1, Some("rejected")),
            transition(7, 8, None),
        ],
        notes: vec![note(1, NotePlacement::After, "Waits for the author")],
        ..StateMachine::default()
    }
}
