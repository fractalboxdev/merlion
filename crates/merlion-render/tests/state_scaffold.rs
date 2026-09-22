//! The interfaces the state-diagram workstreams build on (specs/state.md): both headers
//! dispatch to the state parser, the model, lowering and draw stages are wired end to
//! end over the flowchart layout engine, and the SVG the pipeline produces already meets
//! the output guarantees of specs/svg-output.md.

mod svg_support;

use merlion_render::diag::Diagnostics;
use merlion_render::fuel::Fuel;
use merlion_render::layout::{layout_state, StateLayout};
use merlion_render::model::state::{NotePlacement, StateKind, StateMachine};
use merlion_render::model::{Diagram, Shape};
use merlion_render::parse::{parse, ParseOptions};
use merlion_render::svg::{draw_state, outline_state};
use merlion_render::{render, Direction, RenderOptions};

use svg_support::{assert_safe, assert_well_formed};

const SRC: &str = "stateDiagram-v2\n    [*] --> Still\n    Still --> Moving\n    Moving --> [*]\n";

fn parse_state(src: &str) -> StateMachine {
    let opts = ParseOptions {
        strict: false,
        limits: Default::default(),
    };
    let mut diags = Diagnostics::new(false);
    match parse(src, &opts, &mut diags) {
        Ok(Diagram::State(s)) => s,
        other => panic!(
            "expected a state diagram, got {other:?}\n{:#?}",
            diags.items
        ),
    }
}

#[test]
fn both_headers_dispatch_to_the_state_parser() {
    for src in [SRC, "stateDiagram\n    [*] --> Still\n"] {
        let s = parse_state(src);
        assert!(s.states.is_empty(), "the scaffold parses no statements");
        assert!(s.transitions.is_empty());
    }
}

#[test]
fn the_diagram_names_its_type() {
    let opts = ParseOptions {
        strict: false,
        limits: Default::default(),
    };
    let mut diags = Diagnostics::new(false);
    let d = parse(SRC, &opts, &mut diags).expect("parse");
    assert_eq!(d.type_name(), "state");
}

#[test]
fn front_matter_reaches_the_state_model() {
    let s = parse_state("---\ntitle: Traffic light\n---\nstateDiagram-v2\n    [*] --> Red\n");
    assert_eq!(s.meta.title.as_deref(), Some("Traffic light"));
    assert_eq!(s.direction, Direction::TB);
}

#[test]
fn the_model_types_cover_the_spec() {
    // specs/state.md#model: the shapes the parser workstream fills in.
    assert_eq!(StateKind::ALL.len(), 7);
    assert_eq!(StateKind::Choice.as_str(), "choice");
    assert!(StateKind::Simple.draws_label());
    for k in [
        StateKind::Start,
        StateKind::End,
        StateKind::Choice,
        StateKind::Fork,
        StateKind::Join,
    ] {
        assert!(!k.draws_label(), "{k:?} draws a bare symbol");
    }
    assert_ne!(NotePlacement::Before, NotePlacement::After);
}

#[test]
fn every_state_kind_lowers_to_a_shape_the_flowchart_engine_draws() {
    // specs/state.md#lowering: a composite becomes a cluster, everything else a node.
    use merlion_render::layout::state::node_shape;
    assert_eq!(node_shape(StateKind::Composite), None);
    assert_eq!(node_shape(StateKind::Simple), Some(Shape::Rect));
    assert_eq!(node_shape(StateKind::Choice), Some(Shape::Rhombus));
    assert_eq!(node_shape(StateKind::Fork), Some(Shape::Fork));
    assert_eq!(node_shape(StateKind::Join), Some(Shape::Fork));
    assert_eq!(node_shape(StateKind::Start), Some(Shape::FilledCircle));
    assert_eq!(node_shape(StateKind::End), Some(Shape::FramedCircle));
    for k in StateKind::ALL {
        if let Some(shape) = node_shape(k) {
            assert!(
                Shape::ALL.contains(&shape),
                "{k:?} lowers to an unknown shape"
            );
        }
    }
}

#[test]
fn lowering_and_layout_are_wired_end_to_end() {
    let s = parse_state(SRC);
    let opts = RenderOptions::default();
    let mut fuel = Fuel::new(opts.fuel);
    let mut diags = Diagnostics::new(false);
    let layout: StateLayout = layout_state(&s, &opts, &mut fuel, &mut diags).expect("layout");
    // The lowered graph is what the flowchart engine laid out, and the maps back to the
    // model are total over the model's side (specs/state.md#what-the-lowering-guarantees).
    assert_eq!(layout.lowering.node_for.len(), s.states.len());
    assert_eq!(layout.lowering.cluster_for.len(), s.states.len());
    assert_eq!(
        layout.lowering.node_of.len(),
        layout.lowering.graph.nodes.len()
    );
    assert_eq!(
        layout.lowering.edge_of.len(),
        layout.lowering.graph.edges.len()
    );
    assert_eq!(
        layout.lowering.cluster_of.len(),
        layout.lowering.graph.subgraphs.len()
    );
    assert_eq!(layout.geometry.notes.len(), s.notes.len());
    let out = draw_state(&s, &layout, &opts, "mstate", &mut diags);
    assert!(out
        .svg
        .starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert_eq!(out.outline, outline_state(&s));
}

#[test]
fn the_rendered_state_diagram_carries_the_root_contract() {
    let opts = RenderOptions {
        id_prefix: Some(String::from("mstate")),
        ..RenderOptions::default()
    };
    let r = render(SRC, &opts);
    let svg = r.svg.expect("svg");
    assert!(svg.contains("class=\"merlion merlion-state\""), "{svg}");
    // The hint is the flowchart's, so a state diagram reads one as well as writes it
    // (specs/state.md#phases-and-options).
    assert!(svg.contains("data-merlion-layout=\"v1;TB"), "{svg}");
    assert!(
        svg.contains("<title id=\"mstate-title\">State diagram</title>"),
        "{svg}"
    );
    assert_well_formed(&svg);
    assert_safe(&svg, "mstate");
}

#[test]
fn the_outline_is_returned_as_plain_text() {
    let r = render(SRC, &RenderOptions::default());
    let outline = r.outline.expect("outline");
    assert!(
        outline.starts_with("State diagram, top to bottom."),
        "{outline}"
    );
    assert_eq!(merlion_render::outline(SRC).expect("outline"), outline);
}
