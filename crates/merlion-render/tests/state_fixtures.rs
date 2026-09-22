//! The state corpus in `tests/fixtures/state/` renders end to end (specs/state.md#testing).
//!
//! Every other state test drives one stage over a hand-built model. These drive the whole
//! pipeline — parse, lower, lay out, draw — over real sources, which is where a mismatch
//! between the three stages shows.

mod svg_support;

use std::path::PathBuf;

use merlion_render::diag::{Diagnostics, Severity};
use merlion_render::fuel::Fuel;
use merlion_render::geometry::state::StateNote;
use merlion_render::geometry::{ClusterGeom, NodeGeom};
use merlion_render::layout::{layout_state, StateLayout};
use merlion_render::model::state::{StateKind, StateMachine};
use merlion_render::model::Diagram;
use merlion_render::parse::{parse, ParseOptions};
use merlion_render::{render, Direction, RenderOptions};

use svg_support::{assert_safe, assert_well_formed};

// --------------------------------------------------------------------------- corpus

fn fixtures() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/state");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mmd"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&p).unwrap();
            (name, text)
        })
        .collect();
    out.sort();
    out
}

fn machine(src: &str) -> StateMachine {
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

fn laid_out(sm: &StateMachine) -> StateLayout {
    let opts = RenderOptions::default();
    let mut fuel = Fuel::new(opts.fuel);
    let mut diags = Diagnostics::new(false);
    layout_state(sm, &opts, &mut fuel, &mut diags).expect("layout")
}

// --------------------------------------------------------------------------- boxes

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

// --------------------------------------------------------------------------- tests

#[test]
fn the_corpus_covers_the_grammar() {
    let names: Vec<String> = fixtures().into_iter().map(|(n, _)| n).collect();
    assert!((15..=25).contains(&names.len()), "{names:?}");
    let all = names.join(" ");
    for topic in ["concurrency", "choice", "notes", "direction", "styled"] {
        assert!(all.contains(topic), "no fixture covers {topic}: {all}");
    }
}

#[test]
fn every_fixture_renders() {
    for (name, src) in fixtures() {
        let r = render(&src, &RenderOptions::default());
        let svg = r
            .svg
            .unwrap_or_else(|| panic!("{name}: {:#?}", r.diagnostics));
        assert!(
            svg.contains("class=\"merlion merlion-state\""),
            "{name}: {svg}"
        );
        assert!(
            r.diagnostics.iter().all(|d| d.severity != Severity::Error),
            "{name}: {:#?}",
            r.diagnostics
        );
        assert!(r.outline.is_some_and(|o| !o.is_empty()), "{name}");
    }
}

#[test]
fn every_fixture_draws_a_safe_well_formed_document() {
    for (name, src) in fixtures() {
        let opts = RenderOptions {
            id_prefix: Some(String::from("mfix")),
            ..RenderOptions::default()
        };
        let svg = render(&src, &opts).svg.expect(&name);
        assert_well_formed(&svg);
        assert_safe(&svg, "mfix");
    }
}

#[test]
fn every_fixture_renders_the_same_bytes_twice() {
    for (name, src) in fixtures() {
        let opts = RenderOptions::default();
        assert_eq!(render(&src, &opts).svg, render(&src, &opts).svg, "{name}");
    }
}

#[test]
fn every_fixture_repeats_its_own_layout_when_hinted_with_it() {
    // A state diagram reads the flowchart hint as well as writes it
    // (specs/state.md#phases-and-options).
    for (name, src) in fixtures() {
        let first = render(&src, &RenderOptions::default()).svg.expect(&name);
        let opts = RenderOptions {
            hint: Some(first.clone()),
            ..RenderOptions::default()
        };
        assert_eq!(
            render(&src, &opts).svg.as_deref(),
            Some(first.as_str()),
            "{name}"
        );
    }
}

#[test]
fn every_fixture_draws_one_group_per_state_and_transition() {
    for (name, src) in fixtures() {
        let sm = machine(&src);
        let svg = render(&src, &RenderOptions::default()).svg.expect(&name);
        // A composite holding members is a cluster; every other state is a node
        // (specs/state.md#lowering).
        let composites = sm
            .states
            .iter()
            .enumerate()
            .filter(|(i, s)| {
                s.kind == StateKind::Composite && sm.states.iter().any(|c| c.parent == Some(*i))
            })
            .count();
        assert_eq!(
            svg.matches("class=\"merlion-node merlion-state-node")
                .count()
                + composites,
            sm.states.len(),
            "{name}"
        );
        assert_eq!(
            svg.matches("class=\"merlion-cluster merlion-composite")
                .count(),
            composites,
            "{name}"
        );
        assert_eq!(
            svg.matches("class=\"merlion-region\"").count(),
            sm.regions.len(),
            "{name}"
        );
        // A transition between a composite and a state nested inside it is dropped, so
        // the drawn transitions are at most the source's.
        let drawn = svg
            .matches("class=\"merlion-edge merlion-transition\"")
            .count();
        assert!(drawn <= sm.transitions.len(), "{name}: {drawn} transitions");
        assert_eq!(
            svg.matches("class=\"merlion-note\"").count(),
            sm.notes.len(),
            "{name}"
        );
    }
}

#[test]
fn no_two_shapes_overlap_and_every_member_sits_inside_its_composite() {
    for (name, src) in fixtures() {
        let sm = machine(&src);
        let layout = laid_out(&sm);
        let g = &layout.geometry;
        let boxes: Vec<Box2> = g.graph.nodes.iter().map(Box2::of_node).collect();
        for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                assert!(
                    !boxes[i].overlaps(&boxes[j], 0.5),
                    "{name}: nodes {i} and {j} overlap"
                );
            }
        }
        for (v, n) in layout.lowering.graph.nodes.iter().enumerate() {
            let Some(c) = n.subgraph else { continue };
            assert!(
                Box2::of_cluster(&g.graph.clusters[c]).contains(&boxes[v], 0.5),
                "{name}: node {v} leaves cluster {c}"
            );
        }
    }
}

#[test]
fn every_note_box_clears_every_shape() {
    for (name, src) in fixtures() {
        let sm = machine(&src);
        let g = laid_out(&sm).geometry;
        for (i, note) in g.notes.iter().enumerate() {
            let nb = Box2::of_note(note);
            for (v, n) in g.graph.nodes.iter().enumerate() {
                assert!(
                    !nb.overlaps(&Box2::of_node(n), 0.5),
                    "{name}: note {i} overlaps node {v}"
                );
            }
            for (j, other) in g.notes.iter().enumerate().skip(i + 1) {
                assert!(
                    !nb.overlaps(&Box2::of_note(other), 0.5),
                    "{name}: notes {i} and {j} overlap"
                );
            }
        }
    }
}

#[test]
fn the_drawing_sits_inside_the_view_box() {
    for (name, src) in fixtures() {
        let sm = machine(&src);
        let g = laid_out(&sm).geometry;
        let extent = Box2 {
            x0: 0.0,
            y0: 0.0,
            x1: g.graph.width,
            y1: g.graph.height,
        };
        for (v, n) in g.graph.nodes.iter().enumerate() {
            assert!(
                extent.contains(&Box2::of_node(n), 0.5),
                "{name}: node {v} falls outside {:?}",
                (g.graph.width, g.graph.height)
            );
        }
        for (i, note) in g.notes.iter().enumerate() {
            assert!(
                extent.contains(&Box2::of_note(note), 0.5),
                "{name}: note {i} falls outside {:?}",
                (g.graph.width, g.graph.height)
            );
        }
    }
}

#[test]
fn a_direction_statement_turns_the_whole_drawing() {
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/state/ingest-direction.mmd"),
    )
    .unwrap();
    let sm = machine(&src);
    assert_eq!(sm.direction, Direction::LR);
    let svg = render(&src, &RenderOptions::default()).svg.expect("svg");
    assert!(svg.contains("data-merlion-layout=\"v1;LR"), "{svg}");
}
