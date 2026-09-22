//! Drawing sequence diagrams (specs/sequence.md#svg-output, #roles-and-automatic-tones,
//! #text-alternative).
//!
//! The model and the geometry are hand-built, so these assertions hold whatever the
//! parser and the layout solver produce.

mod svg_support;

use merlion_render::diag::{Diagnostics, Severity, Span};
use merlion_render::geometry::sequence::{
    ActivationGeom, BoxGeom, FragmentGeom, MessageGeom, NoteGeom, ParticipantGeom, Rect,
    SectionGeom, SequenceGeometry,
};
use merlion_render::model::sequence::{
    Autonumber, Central, Fragment, FragmentKind, Head, Item, Message, MessageLine, Note,
    Participant, ParticipantBox, ParticipantKind, Placement, Section, Sequence,
};
use merlion_render::model::Color;
use merlion_render::svg::{draw_sequence, outline_sequence};
use merlion_render::text::{LabelLayout, Line, Run, Weight};
use merlion_render::RenderOptions;

use svg_support::{all_attrs, assert_safe, assert_well_formed, style_text};

// ---------------------------------------------------------------- model builders

fn label(text: &str) -> LabelLayout {
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

fn participant(id: &str, kind: ParticipantKind) -> Participant {
    Participant {
        id: String::from(id),
        label: String::from(id),
        kind,
        ..Participant::default()
    }
}

fn message(index: u32, from: usize, to: usize, text: &str) -> Message {
    Message {
        index,
        from,
        to,
        label: String::from(text),
        ..Message::default()
    }
}

fn note(placement: Placement, from: usize, to: usize, text: &str) -> Note {
    Note {
        placement,
        from,
        to,
        text: String::from(text),
        span: Span::default(),
    }
}

fn section(label: &str, items: Vec<Item>) -> Section {
    Section {
        label: String::from(label),
        items,
        span: Span::default(),
    }
}

fn fragment(kind: FragmentKind, sections: Vec<Section>) -> Fragment {
    Fragment {
        kind,
        sections,
        span: Span::default(),
    }
}

// ------------------------------------------------------------- geometry builders

fn column(x: f64, label_text: &str) -> ParticipantGeom {
    ParticipantGeom {
        x,
        head: Rect {
            x: x - 40.0,
            y: 8.0,
            w: 80.0,
            h: 32.0,
        },
        foot: Some(Rect {
            x: x - 40.0,
            y: 300.0,
            w: 80.0,
            h: 32.0,
        }),
        lifeline: (40.0, 300.0),
        label: label(label_text),
    }
}

fn row(y: f64, x0: f64, x1: f64, text: &str) -> MessageGeom {
    MessageGeom {
        from: (x0, y),
        to: (x1, y),
        self_loop: false,
        label: Some(((x0 + x1) / 2.0, y - 12.0, label(text))),
        number: None,
    }
}

/// A two-participant diagram with one message: the smallest complete drawing.
fn simple() -> (Sequence, SequenceGeometry) {
    let seq = Sequence {
        participants: vec![
            participant("Alice", ParticipantKind::Participant),
            participant("Bob", ParticipantKind::Participant),
        ],
        items: vec![Item::Message(message(0, 0, 1, "Hello"))],
        messages: 1,
        ..Sequence::default()
    };
    let geom = SequenceGeometry {
        width: 300.0,
        height: 340.0,
        participants: vec![column(50.0, "Alice"), column(220.0, "Bob")],
        messages: vec![row(120.0, 50.0, 220.0, "Hello")],
        ..SequenceGeometry::default()
    };
    (seq, geom)
}

/// Every element the SVG contract names, in one diagram.
fn rich() -> (Sequence, SequenceGeometry) {
    let inner = fragment(
        FragmentKind::Alt,
        vec![
            section("is sick", vec![Item::Message(message(2, 1, 0, "Declined"))]),
            section("is well", vec![Item::Message(message(3, 0, 1, "Approved"))]),
        ],
    );
    let seq = Sequence {
        participants: vec![
            participant("Alice", ParticipantKind::Actor),
            participant("API", ParticipantKind::Participant),
            participant("DB", ParticipantKind::Database),
        ],
        boxes: vec![ParticipantBox {
            label: String::from("Service"),
            color: Some(Color::Named("aqua")),
            participants: vec![1, 2],
            span: Span::default(),
        }],
        items: vec![
            Item::Message(message(0, 0, 1, "Place order")),
            Item::Note(note(Placement::Over, 0, 1, "One order")),
            Item::Fragment(fragment(
                FragmentKind::Loop,
                vec![section(
                    "every minute",
                    vec![
                        Item::Message(message(1, 1, 2, "read")),
                        Item::Fragment(inner),
                    ],
                )],
            )),
        ],
        autonumber: Some(Autonumber::default()),
        messages: 4,
        ..Sequence::default()
    };
    let geom = SequenceGeometry {
        width: 460.0,
        height: 360.0,
        participants: vec![
            column(50.0, "Alice"),
            column(220.0, "API"),
            column(390.0, "DB"),
        ],
        messages: vec![
            MessageGeom {
                number: Some((60.0, 108.0)),
                ..row(120.0, 50.0, 220.0, "Place order")
            },
            MessageGeom {
                number: Some((230.0, 168.0)),
                ..row(180.0, 220.0, 390.0, "read")
            },
            MessageGeom {
                number: Some((60.0, 228.0)),
                ..row(240.0, 220.0, 50.0, "Declined")
            },
            MessageGeom {
                number: Some((60.0, 288.0)),
                ..row(280.0, 50.0, 220.0, "Approved")
            },
        ],
        notes: vec![NoteGeom {
            index: 0,
            placement: Placement::Over,
            box_: Rect {
                x: 40.0,
                y: 132.0,
                w: 190.0,
                h: 36.0,
            },
            label: label("One order"),
        }],
        fragments: vec![
            FragmentGeom {
                kind: FragmentKind::Loop,
                box_: Rect {
                    x: 30.0,
                    y: 150.0,
                    w: 400.0,
                    h: 160.0,
                },
                depth: 0,
                tab: Rect {
                    x: 30.0,
                    y: 150.0,
                    w: 46.0,
                    h: 18.0,
                },
                label: label("every minute"),
                label_x: 82.0,
                label_y: 160.0,
                sections: vec![],
            },
            FragmentGeom {
                kind: FragmentKind::Alt,
                box_: Rect {
                    x: 38.0,
                    y: 200.0,
                    w: 384.0,
                    h: 100.0,
                },
                depth: 1,
                tab: Rect {
                    x: 38.0,
                    y: 200.0,
                    w: 40.0,
                    h: 18.0,
                },
                label: label("is sick"),
                label_x: 84.0,
                label_y: 210.0,
                sections: vec![SectionGeom {
                    y: 260.0,
                    label: label("is well"),
                    label_x: 44.0,
                    label_y: 270.0,
                }],
            },
        ],
        activations: vec![ActivationGeom {
            participant: 1,
            depth: 0,
            bar: Rect {
                x: 215.0,
                y: 120.0,
                w: 10.0,
                h: 160.0,
            },
        }],
        boxes: vec![BoxGeom {
            box_: Rect {
                x: 172.0,
                y: 0.0,
                w: 266.0,
                h: 48.0,
            },
            label: label("Service"),
            label_x: 180.0,
            label_y: 10.0,
        }],
        fuel_used: 0,
    };
    (seq, geom)
}

fn draw(seq: &Sequence, geom: &SequenceGeometry) -> String {
    let mut diags = Diagnostics::new(false);
    draw_sequence(seq, geom, &RenderOptions::default(), "mseq", &mut diags).svg
}

fn draw_with(
    seq: &Sequence,
    geom: &SequenceGeometry,
    opts: &RenderOptions,
    diags: &mut Diagnostics,
) -> String {
    draw_sequence(seq, geom, opts, "mseq", diags).svg
}

/// The body of `.merlion-diagram`, where the draw order is observable.
fn body(svg: &str) -> &str {
    let a = svg
        .find("<g class=\"merlion-diagram\"")
        .expect("diagram group");
    &svg[a..]
}

fn at(svg: &str, needle: &str) -> usize {
    body(svg)
        .find(needle)
        .unwrap_or_else(|| panic!("missing {needle:?} in\n{svg}"))
}

// ------------------------------------------------------------------------ tests

#[test]
fn the_output_is_well_formed_and_safe() {
    for (seq, geom) in [simple(), rich()] {
        let svg = draw(&seq, &geom);
        assert_well_formed(&svg);
        assert_safe(&svg, "mseq");
    }
}

#[test]
fn hostile_labels_reach_no_markup() {
    let mut seq = Sequence {
        participants: vec![
            Participant {
                id: String::from("</g><script>alert(1)</script>"),
                label: String::from("\" onload=\"x"),
                kind: ParticipantKind::Actor,
                ..Participant::default()
            },
            participant("B", ParticipantKind::Participant),
        ],
        items: vec![
            Item::Message(message(0, 0, 1, "</text><script>alert(1)</script>")),
            Item::Note(note(Placement::RightOf, 1, 1, "]]><!--")),
        ],
        boxes: vec![ParticipantBox {
            label: String::from("<svg onload=alert(1)>"),
            color: None,
            participants: vec![0],
            span: Span::default(),
        }],
        messages: 1,
        ..Sequence::default()
    };
    seq.items.push(Item::Fragment(fragment(
        FragmentKind::Alt,
        vec![section("</title><style>*{}", vec![])],
    )));
    let geom = SequenceGeometry {
        width: 300.0,
        height: 340.0,
        participants: vec![column(50.0, "\" onload=\"x"), column(220.0, "B")],
        messages: vec![row(120.0, 50.0, 220.0, "</text><script>")],
        notes: vec![NoteGeom {
            index: 0,
            placement: Placement::RightOf,
            box_: Rect {
                x: 240.0,
                y: 140.0,
                w: 60.0,
                h: 36.0,
            },
            label: label("]]><!--"),
        }],
        fragments: vec![FragmentGeom {
            kind: FragmentKind::Alt,
            box_: Rect {
                x: 20.0,
                y: 200.0,
                w: 260.0,
                h: 60.0,
            },
            depth: 0,
            tab: Rect {
                x: 20.0,
                y: 200.0,
                w: 40.0,
                h: 18.0,
            },
            label: label("</title><style>*{}"),
            label_x: 64.0,
            label_y: 210.0,
            sections: vec![],
        }],
        boxes: vec![BoxGeom {
            box_: Rect {
                x: 8.0,
                y: 0.0,
                w: 96.0,
                h: 48.0,
            },
            label: label("<svg onload=alert(1)>"),
            label_x: 12.0,
            label_y: 10.0,
        }],
        ..SequenceGeometry::default()
    };
    let svg = draw(&seq, &geom);
    assert_well_formed(&svg);
    assert_safe(&svg, "mseq");
    assert!(!svg.contains("<script"), "{svg}");
    // The hostile text survives as inert content: no attribute is named `onload`.
    assert!(!svg.contains(" onload=\""), "{svg}");
    assert!(svg.contains("&quot; onload=&quot;x"), "{svg}");
    assert!(svg.contains("&lt;/text&gt;&lt;script&gt;"), "{svg}");
    // The source id reaches `data-merlion-id` escaped and the XML id encoded.
    assert!(
        svg.contains("data-merlion-id=\"&lt;/g&gt;&lt;script&gt;"),
        "{svg}"
    );
    for (_, name, value) in all_attrs(&svg) {
        if name == "id" {
            assert!(
                value
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "id {value:?}"
            );
        }
    }
}

#[test]
fn the_same_input_draws_the_same_bytes() {
    let (seq, geom) = rich();
    assert_eq!(draw(&seq, &geom), draw(&seq, &geom));
    let (again, geom2) = rich();
    assert_eq!(draw(&seq, &geom), draw(&again, &geom2));
}

#[test]
fn elements_carry_their_groups_and_data_attributes() {
    let (seq, geom) = rich();
    let svg = draw(&seq, &geom);
    for needle in [
        "<g class=\"merlion-cluster merlion-box merlion-bs-0\" data-merlion-index=\"0\">",
        "data-merlion-id=\"Alice\" data-merlion-kind=\"actor\" data-merlion-rank=\"0\" \
         id=\"mseq-n0\"",
        "data-merlion-id=\"DB\" data-merlion-kind=\"database\" data-merlion-rank=\"0\" \
         id=\"mseq-n2\"",
        "data-merlion-kind=\"loop\" data-merlion-index=\"0\"",
        "data-merlion-kind=\"alt\" data-merlion-index=\"1\"",
        "<g class=\"merlion-activation\" data-merlion-id=\"API\" data-merlion-depth=\"0\">",
        "data-merlion-from=\"Alice\" data-merlion-to=\"API\" data-merlion-placement=\"over\"",
        "data-merlion-from=\"Alice\" data-merlion-to=\"API\" data-merlion-index=\"0\" \
         id=\"mseq-e0\"",
        "class=\"merlion-lifeline\"",
        "class=\"merlion-note-box\"",
        "class=\"merlion-fragment-tab\"",
        "class=\"merlion-cluster-title\"",
        "class=\"merlion-fragment-label\"",
        "class=\"merlion-fragment-divider\"",
        "class=\"merlion-fragment-section\"",
        "class=\"merlion-shape merlion-participant-foot\"",
    ] {
        assert!(svg.contains(needle), "missing {needle:?} in\n{svg}");
    }
    // Sequences reverse nothing and wrap nothing.
    assert!(!svg.contains("data-merlion-back"));
    assert!(!svg.contains("data-merlion-wrap"));
}

#[test]
fn elements_paint_in_the_spec_order() {
    let (seq, geom) = rich();
    let svg = draw(&seq, &geom);
    let order = [
        "merlion-box",
        "merlion-fragment",
        "merlion-lifeline",
        "merlion-activation",
        "merlion-note",
        "merlion-message",
    ];
    let mut last = 0;
    for name in order {
        let i = at(&svg, name);
        assert!(i > last, "{name} out of order in\n{svg}");
        last = i;
    }
}

#[test]
fn a_participant_created_inside_a_fragment_paints_over_its_box() {
    // A `create`d participant's head sits at its creating message's row, so a
    // fragment enclosing that row covers it unless the column paints later.
    let (mut seq, mut geom) = rich();
    seq.participants[2].created_by = Some(1);
    geom.participants[2].head = Rect {
        x: 350.0,
        y: 170.0,
        w: 80.0,
        h: 32.0,
    };
    geom.participants[2].foot = None;
    geom.participants[2].lifeline = (202.0, 300.0);
    let svg = draw(&seq, &geom);
    // `rich()`'s outer `loop` box spans y 150..310, so it encloses that head.
    let column = at(&svg, "data-merlion-id=\"DB\"");
    let frag = at(&svg, "merlion-fragment");
    assert!(
        column > frag,
        "the fragment box paints over the created head box in\n{svg}"
    );
}

#[test]
fn every_arrowhead_kind_defines_its_marker_once() {
    let heads = [
        (Head::Filled, "arrow"),
        (Head::Open, "open"),
        (Head::Cross, "cross"),
        (Head::HalfTop, "half-top"),
        (Head::HalfBottom, "half-bottom"),
        (Head::StickTop, "stick-top"),
        (Head::StickBottom, "stick-bottom"),
    ];
    let items: Vec<Item> = heads
        .iter()
        .enumerate()
        .map(|(i, (h, _))| {
            Item::Message(Message {
                head: *h,
                tail: if i % 2 == 0 { Head::None } else { *h },
                line: if i % 2 == 0 {
                    MessageLine::Solid
                } else {
                    MessageLine::Dotted
                },
                ..message(i as u32, 0, 1, "m")
            })
        })
        .collect();
    let seq = Sequence {
        participants: vec![
            participant("A", ParticipantKind::Participant),
            participant("B", ParticipantKind::Participant),
        ],
        messages: items.len() as u32,
        items,
        ..Sequence::default()
    };
    let geom = SequenceGeometry {
        width: 300.0,
        height: 400.0,
        participants: vec![column(50.0, "A"), column(220.0, "B")],
        messages: (0..heads.len())
            .map(|i| row(60.0 + 20.0 * i as f64, 50.0, 220.0, "m"))
            .collect(),
        ..SequenceGeometry::default()
    };
    let svg = draw(&seq, &geom);
    assert_well_formed(&svg);
    assert_safe(&svg, "mseq");
    for (_, name) in heads {
        let id = format!("id=\"mseq-{name}\"");
        assert_eq!(svg.matches(&id).count(), 1, "marker {name} in\n{svg}");
        assert!(
            svg.contains(&format!("marker-end=\"url(#mseq-{name})\"")),
            "{svg}"
        );
    }
    assert!(svg.contains("marker-start=\"url(#mseq-open)\""), "{svg}");
    assert!(
        svg.contains("class=\"merlion-edge-path merlion-dotted\""),
        "{svg}"
    );
    // A head the diagram never draws defines no marker.
    let plain = draw(&simple().0, &simple().1);
    assert!(!plain.contains("mseq-cross"), "{plain}");
}

#[test]
fn a_head_of_none_draws_no_marker() {
    let (mut seq, geom) = simple();
    seq.items = vec![Item::Message(Message {
        head: Head::None,
        ..message(0, 0, 1, "Hello")
    })];
    let svg = draw(&seq, &geom);
    assert!(!svg.contains("<defs>"), "{svg}");
    assert!(!svg.contains("marker-end"), "{svg}");
}

#[test]
fn a_self_message_draws_a_bracket_and_a_central_end_a_dot() {
    let (mut seq, mut geom) = simple();
    seq.participants
        .push(participant("C", ParticipantKind::Participant));
    seq.items = vec![
        Item::Message(message(0, 0, 0, "retry")),
        Item::Message(Message {
            central: Central::Both,
            ..message(1, 0, 1, "ping")
        }),
    ];
    seq.messages = 2;
    geom.participants.push(column(390.0, "C"));
    geom.messages = vec![
        MessageGeom {
            self_loop: true,
            to: (50.0, 154.0),
            ..row(120.0, 50.0, 50.0, "retry")
        },
        row(180.0, 50.0, 220.0, "ping"),
    ];
    let svg = draw(&seq, &geom);
    assert_well_formed(&svg);
    // The bracket reaches right of its own lifeline and returns.
    assert!(svg.contains("d=\"M50 120H90V154H50\""), "{svg}");
    assert_eq!(svg.matches("class=\"merlion-central\"").count(), 2, "{svg}");
}

#[test]
fn automatic_tones_follow_the_kind_and_the_fragment() {
    let (seq, geom) = rich();
    let svg = draw(&seq, &geom);
    assert!(
        svg.contains("class=\"merlion-node merlion-participant merlion-c-accent merlion-auto\""),
        "{svg}"
    );
    assert!(
        svg.contains("class=\"merlion-node merlion-participant merlion-c-store merlion-auto\""),
        "{svg}"
    );
    assert!(
        svg.contains("class=\"merlion-cluster merlion-fragment merlion-cc-series-1 merlion-auto\""),
        "{svg}"
    );
    assert!(
        svg.contains("class=\"merlion-cluster merlion-fragment merlion-cc-warn merlion-auto\""),
        "{svg}"
    );
    // The cluster tone role gets a rule at the cluster mix.
    let css = style_text(&svg);
    assert!(
        css.contains("#mseq .merlion-cc-warn>.merlion-cluster-box{"),
        "{css}"
    );
    // A plain participant, a box and a note take none.
    assert!(
        svg.contains("class=\"merlion-node merlion-participant\" data-merlion-id=\"API\""),
        "{svg}"
    );
    assert!(!svg.contains("merlion-box merlion-c"), "{svg}");
}

#[test]
fn auto_tone_off_draws_no_role_class() {
    let (seq, geom) = rich();
    let opts = RenderOptions {
        auto_tone: false,
        ..RenderOptions::default()
    };
    let mut diags = Diagnostics::new(false);
    let svg = draw_with(&seq, &geom, &opts, &mut diags);
    assert!(!svg.contains("merlion-auto"), "{svg}");
    assert!(!svg.contains("merlion-c-accent"), "{svg}");
    assert!(!svg.contains("merlion-cc-warn"), "{svg}");
}

#[test]
fn a_source_setting_auto_tone_off_matches_the_option() {
    let (mut seq, geom) = rich();
    seq.meta.auto_tone = Some(false);
    let off = RenderOptions {
        auto_tone: false,
        ..RenderOptions::default()
    };
    let mut diags = Diagnostics::new(false);
    let (plain, _) = rich();
    assert_eq!(
        draw(&seq, &geom),
        draw_with(&plain, &geom, &off, &mut diags)
    );
}

#[test]
fn autonumber_badges_print_the_sequence() {
    let (mut seq, geom) = rich();
    seq.autonumber = Some(Autonumber {
        start: 105,
        step: 10,
        visible: true,
    });
    let svg = draw(&seq, &geom);
    for n in ["1.05", "1.15", "1.25", "1.35"] {
        assert!(
            svg.contains(&format!(">{n}</tspan>")) || svg.contains(&format!(">{n}</text>")),
            "badge {n} missing in\n{svg}"
        );
    }
    assert_eq!(svg.matches("class=\"merlion-message-number\"").count(), 4);
}

#[test]
fn autonumber_off_hides_the_badges() {
    let (mut seq, geom) = rich();
    seq.autonumber = Some(Autonumber {
        visible: false,
        ..Autonumber::default()
    });
    let svg = draw(&seq, &geom);
    assert!(!body(&svg).contains("merlion-message-number"), "{svg}");
}

#[test]
fn a_source_colour_is_a_fixed_rule_and_an_info() {
    let (mut seq, mut geom) = simple();
    seq.boxes = vec![ParticipantBox {
        label: String::from("Group"),
        color: Some(Color::Rgba {
            r: 191,
            g: 223,
            b: 255,
            a: 255,
        }),
        participants: vec![0, 1],
        span: Span::default(),
    }];
    seq.items.push(Item::Fragment(fragment(
        FragmentKind::Rect(Some(Color::Named("aqua"))),
        vec![section("", vec![])],
    )));
    geom.boxes = vec![BoxGeom {
        box_: Rect {
            x: 8.0,
            y: 0.0,
            w: 280.0,
            h: 48.0,
        },
        label: label("Group"),
        label_x: 12.0,
        label_y: 10.0,
    }];
    geom.fragments = vec![FragmentGeom {
        kind: FragmentKind::Rect(Some(Color::Named("aqua"))),
        box_: Rect {
            x: 20.0,
            y: 200.0,
            w: 260.0,
            h: 60.0,
        },
        depth: 0,
        tab: Rect {
            x: 20.0,
            y: 200.0,
            w: 40.0,
            h: 18.0,
        },
        label: LabelLayout::default(),
        label_x: 64.0,
        label_y: 210.0,
        sections: vec![],
    }];
    let mut diags = Diagnostics::new(false);
    let svg = draw_with(&seq, &geom, &RenderOptions::default(), &mut diags);
    assert_eq!(
        diags
            .items
            .iter()
            .filter(|d| d.code == "I030" && d.severity == Severity::Info)
            .count(),
        1,
        "{:?}",
        diags.items
    );
    let css = style_text(&svg);
    assert!(
        css.contains("#mseq .merlion-bs-0>.merlion-cluster-box{fill:#bfdfff;}"),
        "{css}"
    );
    assert!(
        css.contains("#mseq .merlion-fs-0>.merlion-cluster-box{fill:aqua;}"),
        "{css}"
    );
    assert!(svg.contains("merlion-fragment merlion-fs-0"), "{svg}");
    // A `rect` takes no automatic tone.
    assert!(!svg.contains("merlion-fs-0 merlion-cc"), "{svg}");
    assert_safe(&svg, "mseq");
}

#[test]
fn an_empty_diagram_draws_an_empty_body() {
    let svg = draw(&Sequence::default(), &SequenceGeometry::default());
    assert_well_formed(&svg);
    assert_safe(&svg, "mseq");
    assert!(svg.contains("<g class=\"merlion-diagram\""), "{svg}");
}

#[test]
fn a_message_without_a_label_draws_no_chip() {
    let (mut seq, mut geom) = simple();
    seq.items = vec![Item::Message(message(0, 0, 1, ""))];
    geom.messages = vec![MessageGeom {
        label: None,
        ..row(120.0, 50.0, 220.0, "")
    }];
    let svg = draw(&seq, &geom);
    assert!(!body(&svg).contains("merlion-edge-label"), "{svg}");
    assert_well_formed(&svg);
}

#[test]
fn a_destroyed_participant_draws_a_cross_instead_of_a_foot() {
    let (mut seq, mut geom) = simple();
    seq.participants[1].destroyed_by = Some(0);
    geom.participants[1].foot = None;
    let svg = draw(&seq, &geom);
    assert_eq!(svg.matches("merlion-participant-foot").count(), 1, "{svg}");
    assert!(svg.contains("class=\"merlion-destroy\""), "{svg}");
    assert_well_formed(&svg);
}

#[test]
fn degenerate_geometry_draws_a_valid_document() {
    // A model and a geometry that disagree, with non-finite and negative extents: the
    // draw stage never panics and never writes an attribute a parser would reject.
    let (mut seq, mut geom) = rich();
    seq.items.push(Item::Message(message(99, 7, 8, "dangling")));
    seq.items
        .push(Item::Note(note(Placement::Over, 9, 9, "lost")));
    seq.messages = 100;
    geom.width = f64::NAN;
    geom.height = -40.0;
    geom.participants[0].x = f64::INFINITY;
    geom.participants[0].lifeline = (f64::NAN, f64::NAN);
    geom.messages[0].from = (f64::NAN, f64::NEG_INFINITY);
    geom.messages[0].self_loop = true;
    geom.notes[0].box_ = Rect {
        x: f64::NAN,
        y: 0.0,
        w: -10.0,
        h: f64::INFINITY,
    };
    geom.fragments[0].tab = Rect {
        x: f64::NAN,
        y: f64::NAN,
        w: f64::NAN,
        h: f64::NAN,
    };
    geom.activations[0].bar.w = -5.0;
    let svg = draw(&seq, &geom);
    assert_well_formed(&svg);
    assert_safe(&svg, "mseq");
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
    assert!(svg.contains("viewBox=\"0 0 0 0\""), "{svg}");
    // The outline reads the model alone and names the missing participants.
    assert!(
        outline_sequence(&seq).contains("? → ?"),
        "{}",
        outline_sequence(&seq)
    );
}

#[test]
fn a_cluster_tone_is_written_twice() {
    // specs/svg-output.md#theming: every colour is a presentation attribute as well as
    // a rule, so a renderer that drops the embedded style still draws the tone. The six
    // tone roles reach clusters only through the sequence's own rules, which is exactly
    // where the literal is easy to lose.
    let (seq, geom) = rich();
    let svg = draw(&seq, &geom);
    let toned = body(&svg)
        .lines()
        .find(|l| l.contains("merlion-cc-warn"))
        .expect("the alt fragment");
    assert!(toned.contains("stroke=\"#9a6700\""), "{toned}");
    assert!(
        toned.contains("fill=\"#f2eee8\"") && !toned.contains("fill=\"#fafafa\""),
        "the warn tone is missing from the box fill: {toned}"
    );
    let css = style_text(&svg);
    assert!(
        css.contains("#mseq .merlion-cc-warn>.merlion-cluster-box{"),
        "{css}"
    );
}

// -------------------------------------------------------------------- outline

#[test]
fn the_outline_groups_messages_under_their_fragments() {
    let seq = Sequence {
        participants: vec![
            Participant {
                id: String::from("Customer"),
                label: String::from("Customer"),
                ..Participant::default()
            },
            Participant {
                id: String::from("Web"),
                label: String::from("Web app"),
                ..Participant::default()
            },
            Participant {
                id: String::from("API"),
                label: String::from("API gateway"),
                ..Participant::default()
            },
            Participant {
                id: String::from("Bank"),
                label: String::from("Bank"),
                ..Participant::default()
            },
        ],
        items: vec![
            Item::Message(message(0, 0, 1, "Place order")),
            Item::Message(message(1, 1, 2, "POST /orders")),
            Item::Fragment(fragment(
                FragmentKind::Loop,
                vec![section(
                    "Every minute",
                    vec![
                        Item::Message(message(2, 2, 3, "Authorise payment")),
                        Item::Message(Message {
                            line: MessageLine::Dotted,
                            ..message(3, 3, 2, "Approved")
                        }),
                    ],
                )],
            )),
            Item::Fragment(fragment(
                FragmentKind::Alt,
                vec![
                    section(
                        "is sick",
                        vec![Item::Message(Message {
                            line: MessageLine::Dotted,
                            ..message(4, 3, 1, "Declined")
                        })],
                    ),
                    section(
                        "is well",
                        vec![Item::Message(Message {
                            line: MessageLine::Dotted,
                            ..message(5, 1, 0, "Order confirmed")
                        })],
                    ),
                ],
            )),
            Item::Note(note(Placement::Over, 0, 3, "One order, one transaction")),
        ],
        messages: 6,
        ..Sequence::default()
    };
    assert_eq!(
        outline_sequence(&seq),
        "Sequence diagram. 4 participants, 6 messages.\n\
         Participants: Customer, Web app (Web), API gateway (API), Bank.\n\
         1. Customer → Web app: Place order\n\
         2. Web app → API gateway: POST /orders\n\
         loop Every minute:\n\
         \x20 3. API gateway → Bank: Authorise payment\n\
         \x20 4. Bank --> API gateway: Approved\n\
         alt is sick:\n\
         \x20 5. Bank --> Web app: Declined\n\
         else is well:\n\
         \x20 6. Web app --> Customer: Order confirmed\n\
         Note over Customer,Bank: One order, one transaction"
    );
}

#[test]
fn the_outline_numbers_by_the_autonumber_sequence() {
    let (mut seq, _) = simple();
    seq.autonumber = Some(Autonumber {
        start: 105,
        step: 10,
        visible: true,
    });
    assert!(
        outline_sequence(&seq).contains("\n1.05. Alice → Bob: Hello"),
        "{}",
        outline_sequence(&seq)
    );
}

#[test]
fn the_outline_writes_every_arrow_glyph() {
    let (mut seq, _) = simple();
    seq.items = vec![
        Item::Message(Message {
            head: Head::None,
            ..message(0, 0, 1, "plain")
        }),
        Item::Message(Message {
            head: Head::None,
            tail: Head::Filled,
            ..message(1, 0, 1, "back")
        }),
        Item::Message(Message {
            tail: Head::Filled,
            ..message(2, 0, 1, "both")
        }),
        Item::Message(message(3, 0, 0, "self")),
    ];
    seq.messages = 4;
    let o = outline_sequence(&seq);
    assert!(o.contains("1. Alice — Bob: plain"), "{o}");
    assert!(o.contains("2. Alice ← Bob: back"), "{o}");
    assert!(o.contains("3. Alice ↔ Bob: both"), "{o}");
    assert!(o.contains("4. Alice → Alice: self"), "{o}");
}

#[test]
fn the_outline_lists_notes_by_placement() {
    let (mut seq, _) = simple();
    seq.items = vec![
        Item::Note(note(Placement::LeftOf, 0, 0, "left")),
        Item::Note(note(Placement::RightOf, 1, 1, "right")),
        Item::Note(note(Placement::Over, 0, 1, "both")),
    ];
    seq.messages = 0;
    let o = outline_sequence(&seq);
    assert!(o.contains("\nNote left of Alice: left"), "{o}");
    assert!(o.contains("\nNote right of Bob: right"), "{o}");
    assert!(o.contains("\nNote over Alice,Bob: both"), "{o}");
}

#[test]
fn the_outline_is_the_desc_and_survives_hostile_text() {
    let (mut seq, geom) = simple();
    seq.participants[0].label = String::from("**A** <b>x</b>");
    let svg = draw(&seq, &geom);
    let outline = outline_sequence(&seq);
    assert!(outline.contains("A <b>x</b>"), "{outline}");
    assert!(svg.contains("&lt;b&gt;x&lt;/b&gt;"), "{svg}");
    assert_well_formed(&svg);
}

#[test]
fn accessible_description_replaces_the_outline() {
    let (mut seq, geom) = simple();
    seq.meta.acc_descr = Some(String::from("Two actors exchange one message"));
    let mut diags = Diagnostics::new(false);
    let out = draw_sequence(&seq, &geom, &RenderOptions::default(), "mseq", &mut diags);
    assert!(
        out.svg
            .contains("<desc id=\"mseq-desc\">Two actors exchange one message</desc>"),
        "{}",
        out.svg
    );
    assert_eq!(out.outline, outline_sequence(&seq));
}
