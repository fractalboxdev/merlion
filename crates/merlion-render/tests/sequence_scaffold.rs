//! The interfaces the sequence-diagram workstreams build on (specs/sequence.md): the
//! header dispatches to the sequence parser, the model, geometry and draw stages are
//! wired end to end, and the SVG the pipeline produces already meets the output
//! guarantees of specs/svg-output.md.

mod svg_support;

use merlion_render::diag::Diagnostics;
use merlion_render::fuel::Fuel;
use merlion_render::geometry::sequence::SequenceGeometry;
use merlion_render::layout::layout_sequence;
use merlion_render::model::sequence::{FragmentKind, Head, MessageLine, ParticipantKind, Sequence};
use merlion_render::model::Diagram;
use merlion_render::parse::{parse, ParseOptions};
use merlion_render::svg::{draw_sequence, outline_sequence};
use merlion_render::{render, RenderOptions};

use svg_support::{assert_safe, assert_well_formed};

const SRC: &str =
    "sequenceDiagram\n    Alice->>John: Hello John, how are you?\n    John-->>Alice: Great!\n";

fn parse_sequence(src: &str) -> Sequence {
    let opts = ParseOptions {
        strict: false,
        limits: Default::default(),
    };
    let mut diags = Diagnostics::new(false);
    match parse(src, &opts, &mut diags) {
        Ok(Diagram::Sequence(s)) => s,
        other => panic!("expected a sequence, got {other:?}\n{:#?}", diags.items),
    }
}

#[test]
fn the_header_dispatches_to_the_sequence_parser() {
    let s = parse_sequence(SRC);
    assert_eq!(
        s.participants
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        ["Alice", "John"]
    );
    assert_eq!(s.items.len(), 2);
    assert_eq!(s.messages, 2);
}

#[test]
fn the_diagram_names_its_type() {
    let opts = ParseOptions {
        strict: false,
        limits: Default::default(),
    };
    let mut diags = Diagnostics::new(false);
    let d = parse(SRC, &opts, &mut diags).expect("parse");
    assert_eq!(d.type_name(), "sequence");
}

#[test]
fn front_matter_reaches_the_sequence_model() {
    let s = parse_sequence("---\ntitle: Ordering\n---\nsequenceDiagram\n    A->>B: hi\n");
    assert_eq!(s.meta.title.as_deref(), Some("Ordering"));
}

#[test]
fn the_model_types_cover_the_spec() {
    // specs/sequence.md#model: the shapes the parser workstream fills in.
    assert_ne!(ParticipantKind::Actor, ParticipantKind::Database);
    assert_ne!(MessageLine::Solid, MessageLine::Dotted);
    assert_ne!(Head::Filled, Head::Cross);
    assert_ne!(FragmentKind::Loop, FragmentKind::Break);
}

#[test]
fn layout_and_draw_are_wired_end_to_end() {
    let s = parse_sequence(SRC);
    let opts = RenderOptions::default();
    let mut fuel = Fuel::new(opts.fuel);
    let mut diags = Diagnostics::new(false);
    let geom: SequenceGeometry = layout_sequence(&s, &opts, &mut fuel, &mut diags).expect("layout");
    assert!(geom.width > 0.0 && geom.height > 0.0);
    let out = draw_sequence(&s, &geom, &opts, "mseq", &mut diags);
    assert!(out
        .svg
        .starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert_eq!(out.outline, outline_sequence(&s));
}

#[test]
fn the_rendered_sequence_carries_the_root_contract() {
    let opts = RenderOptions {
        id_prefix: Some(String::from("mseq")),
        ..RenderOptions::default()
    };
    let r = render(SRC, &opts);
    let svg = r.svg.expect("svg");
    assert!(svg.contains("class=\"merlion merlion-sequence\""), "{svg}");
    assert!(svg.contains("data-merlion-layout=\"v1;SEQ"), "{svg}");
    assert!(
        svg.contains("<title id=\"mseq-title\">Sequence diagram</title>"),
        "{svg}"
    );
    assert_well_formed(&svg);
    assert_safe(&svg, "mseq");
}

#[test]
fn the_outline_is_returned_as_plain_text() {
    let r = render(SRC, &RenderOptions::default());
    let outline = r.outline.expect("outline");
    assert!(outline.starts_with("Sequence diagram."), "{outline}");
    assert_eq!(merlion_render::outline(SRC).expect("outline"), outline);
}
