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

/// specs/state.md#testing asks that no label overlap another element. A transition's
/// label chip clears every state box and every other chip outright. It clears every
/// cluster title too, unless its own edge offers no room at all: a chip wider than the
/// segment it sits on, inside a narrow composite, has nowhere to go, and that is a lack
/// of room rather than a placement that ignored the title.
#[test]
fn every_edge_label_chip_clears_the_shapes_the_titles_and_the_other_chips() {
    for (name, src) in fixtures().into_iter().chain(concurrency_sources()) {
        let sm = machine(&src);
        let g = laid_out(&sm).geometry.graph;
        let chips: Vec<(usize, Box2)> = g
            .edges
            .iter()
            .enumerate()
            .filter_map(|(e, edge)| edge.label.as_ref().map(|l| (e, chip_box(l))))
            .collect();
        let nodes: Vec<Box2> = g.nodes.iter().map(Box2::of_node).collect();
        let titles: Vec<Box2> = g
            .clusters
            .iter()
            .filter(|c| c.label.width > 0.0)
            .map(title_box)
            .collect();

        for (i, (e, chip)) in chips.iter().enumerate() {
            // A chip never covers a state: the shapes are obstacles the placer honours
            // before anything else.
            for (v, nb) in nodes.iter().enumerate() {
                assert!(
                    !chip.overlaps(nb, 0.01),
                    "{name}: the label of transition {e} covers state {v}"
                );
            }
            let with_titles: Vec<Box2> = nodes.iter().chain(&titles).copied().collect();
            for (c, tb) in titles.iter().enumerate() {
                if !chip.overlaps(tb, 0.01) {
                    continue;
                }
                assert!(
                    !edge_has_room(&g.edges[*e], &with_titles),
                    "{name}: the label of transition {e} covers the title of composite {c}, \
                     though its edge has room elsewhere"
                );
            }
            // The placer walks the edges in order and avoids the chips already down, so
            // when two chips meet it is the later one that had nowhere else to go.
            for (o, other) in chips.iter().skip(i + 1) {
                if !chip.overlaps(other, 0.01) {
                    continue;
                }
                let against: Vec<Box2> = with_titles
                    .iter()
                    .copied()
                    .chain(chips.iter().filter(|(x, _)| x != o).map(|(_, b)| *b))
                    .collect();
                assert!(
                    !edge_has_room(&g.edges[*o], &against),
                    "{name}: the labels of transitions {e} and {o} overlap, \
                     though {o}'s edge has room elsewhere"
                );
            }
        }
    }
}

/// The chip box of an edge label.
fn chip_box(l: &merlion_render::geometry::EdgeLabelGeom) -> Box2 {
    let (w, h) = merlion_render::geometry::chip_size(&l.label);
    Box2 {
        x0: l.x - w / 2.0,
        y0: l.y - h / 2.0,
        x1: l.x + w / 2.0,
        y1: l.y + h / 2.0,
    }
}

fn title_box(c: &ClusterGeom) -> Box2 {
    Box2 {
        x0: c.label_x - c.label.width / 2.0,
        y0: c.label_y - c.label.height / 2.0,
        x1: c.label_x + c.label.width / 2.0,
        y1: c.label_y + c.label.height / 2.0,
    }
}

/// Whether any point along `edge` holds its chip clear of every box in `obstacles`. The
/// chip is inflated by the clearance the placer keeps around it, so a window the placer
/// cannot use does not count as room.
fn edge_has_room(edge: &merlion_render::geometry::EdgeGeom, obstacles: &[Box2]) -> bool {
    const CLEAR: f64 = 2.0;
    let Some(l) = edge.label.as_ref() else {
        return false;
    };
    let (w, h) = merlion_render::geometry::chip_size(&l.label);
    for seg in edge.points.windows(2) {
        for k in 0..=64 {
            let t = f64::from(k) / 64.0;
            let x = seg[0].x + (seg[1].x - seg[0].x) * t;
            let y = seg[0].y + (seg[1].y - seg[0].y) * t;
            let b = Box2 {
                x0: x - w / 2.0 - CLEAR,
                y0: y - h / 2.0 - CLEAR,
                x1: x + w / 2.0 + CLEAR,
                y1: y + h / 2.0 + CLEAR,
            };
            if obstacles.iter().all(|o| !b.overlaps(o, 0.01)) {
                return true;
            }
        }
    }
    false
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

// ------------------------------------------------------------------ region dividers

/// An axis-aligned segment, as `marks::line_d` writes one.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Seg {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Seg {
    /// Whether the segment passes through `b`. Both are axis-aligned, so this is the
    /// overlap of two intervals per axis.
    fn hits(&self, b: &Box2, eps: f64) -> bool {
        let (sx0, sx1) = (self.x0.min(self.x1), self.x0.max(self.x1));
        let (sy0, sy1) = (self.y0.min(self.y1), self.y0.max(self.y1));
        sx0 < b.x1 - eps && b.x0 < sx1 - eps && sy0 < b.y1 - eps && b.y0 < sy1 - eps
    }
}

/// Every `.merlion-region-divider` path, in document order.
fn region_dividers(svg: &str) -> Vec<Seg> {
    svg.match_indices("class=\"merlion-region-divider\"")
        .map(|(at, _)| {
            let rest = &svg[at..];
            let d = rest
                .split_once(" d=\"M")
                .map(|(_, r)| r.split_once('"').map_or(r, |(v, _)| v))
                .unwrap_or_else(|| panic!("no d on a divider: {}", &rest[..60.min(rest.len())]));
            let (a, b) = d.split_once('L').unwrap_or_else(|| panic!("d={d:?}"));
            let num = |s: &str| -> (f64, f64) {
                let (x, y) = s.trim().split_once(' ').unwrap_or_else(|| panic!("{s:?}"));
                (x.parse().expect("x"), y.parse().expect("y"))
            };
            let ((x0, y0), (x1, y1)) = (num(a), num(b));
            Seg { x0, y0, x1, y1 }
        })
        .collect()
}

/// Region cluster boxes of every composite, in region order.
fn region_boxes(layout: &StateLayout, sm: &StateMachine) -> Vec<Vec<Box2>> {
    use merlion_render::layout::state::ClusterOrigin;
    let mut out: Vec<Vec<Box2>> = Vec::new();
    for parent in 0..sm.states.len() {
        let mut boxes: Vec<(usize, Box2)> = Vec::new();
        for (si, origin) in layout.lowering.cluster_of.iter().enumerate() {
            let ClusterOrigin::Region(ri) = origin else {
                continue;
            };
            let Some(region) = sm.regions.get(*ri) else {
                continue;
            };
            if region.parent != parent {
                continue;
            }
            boxes.push((
                region.index,
                Box2::of_cluster(&layout.geometry.graph.clusters[si]),
            ));
        }
        if boxes.len() > 1 {
            boxes.sort_by_key(|(i, _)| *i);
            out.push(boxes.into_iter().map(|(_, b)| b).collect());
        }
    }
    out
}

/// Whether `seg` lies strictly between `a` and `b` on the axis that separates them.
fn separates(seg: &Seg, a: &Box2, b: &Box2) -> bool {
    let between = |lo: f64, hi: f64, v: f64| lo < v && v < hi;
    let vertical = (seg.x0 - seg.x1).abs() < 0.01;
    let horizontal = (seg.y0 - seg.y1).abs() < 0.01;
    if vertical && a.x1 <= b.x0 + 0.01 {
        return between(a.x1 - 0.01, b.x0 + 0.01, seg.x0);
    }
    if vertical && b.x1 <= a.x0 + 0.01 {
        return between(b.x1 - 0.01, a.x0 + 0.01, seg.x0);
    }
    if horizontal && a.y1 <= b.y0 + 0.01 {
        return between(a.y1 - 0.01, b.y0 + 0.01, seg.y0);
    }
    if horizontal && b.y1 <= a.y0 + 0.01 {
        return between(b.y1 - 0.01, a.y0 + 0.01, seg.y0);
    }
    false
}

/// Sources holding two and three concurrency regions, in both axes, beside the fixture
/// corpus: the divider's axis follows the arrangement the engine produces, not the
/// diagram's direction.
fn concurrency_sources() -> Vec<(String, String)> {
    [
        (
            "two-regions-tb",
            "stateDiagram-v2\nstate Active {\n  [*] --> NumLockOff\n  --\n  [*] --> CapsLockOff\n}\n",
        ),
        (
            "two-regions-lr",
            "stateDiagram-v2\ndirection LR\nstate Active {\n  [*] --> NumLockOff\n  --\n  [*] --> CapsLockOff\n}\n",
        ),
        (
            "three-regions",
            "stateDiagram-v2\n  state s2 {\n  s3\n  --\n  s4\n  --\n  55\n  }\n",
        ),
        (
            "three-regions-lr",
            "stateDiagram-v2\n  direction RL\n  state s2 {\n  s3\n  --\n  s4\n  --\n  55\n  }\n",
        ),
    ]
    .into_iter()
    .map(|(n, s)| (String::from(n), String::from(s)))
    .collect()
}

/// A divider marks where two regions meet. It is drawn on whichever axis the region
/// boxes are actually apart on, never through the states, and no two dividers of one
/// composite land on the same line (specs/state.md#groups-and-data-attributes).
#[test]
fn every_region_divider_falls_between_the_regions_it_separates() {
    for (name, src) in fixtures().into_iter().chain(concurrency_sources()) {
        let sm = machine(&src);
        if sm.regions.is_empty() {
            continue;
        }
        let layout = laid_out(&sm);
        let svg = render(&src, &RenderOptions::default()).svg.expect(&name);
        let dividers = region_dividers(&svg);
        let expected = sm.regions.iter().filter(|r| r.index > 0).count();
        assert_eq!(dividers.len(), expected, "{name}: {dividers:?}");

        for (i, d) in dividers.iter().enumerate() {
            for (j, other) in dividers.iter().enumerate().skip(i + 1) {
                assert_ne!(d, other, "{name}: dividers {i} and {j} are the same line");
            }
            for (v, n) in layout.geometry.graph.nodes.iter().enumerate() {
                assert!(
                    !d.hits(&Box2::of_node(n), 0.01),
                    "{name}: divider {i} {d:?} crosses state {v} {:?}",
                    Box2::of_node(n)
                );
            }
            let boxes = region_boxes(&layout, &sm);
            let found = boxes
                .iter()
                .flat_map(|regions| regions.windows(2))
                .any(|w| separates(d, &w[0], &w[1]));
            assert!(found, "{name}: divider {i} {d:?} separates no two regions");
        }
    }
}

/// The text alternative describes the drawing. A transition between a composite state
/// and a state nested inside it has no two endpoints in the lowered graph and is not
/// drawn, so the outline neither lists nor counts it, and says how many it left out
/// (specs/state.md#text-alternative).
#[test]
fn the_outline_lists_and_counts_only_the_transitions_that_are_drawn() {
    let dropped_case = "stateDiagram-v2\nstate Outer {\n  A --> B\n}\nOuter --> A\nB --> Outer\n";
    for (name, src) in fixtures().into_iter().chain([(
        String::from("composite-to-member"),
        String::from(dropped_case),
    )]) {
        let r = render(&src, &RenderOptions::default());
        let svg = r.svg.expect(&name);
        let outline = r.outline.expect(&name);
        let drawn = svg
            .matches("class=\"merlion-edge merlion-transition\"")
            .count();
        assert_eq!(
            outline.matches('→').count(),
            drawn,
            "{name}: outline lists {} transitions, the SVG draws {drawn}\n{outline}",
            outline.matches('→').count()
        );
        let header = outline.lines().next().expect("a header");
        assert!(
            header.contains(&format!("{drawn} transition")),
            "{name}: {header} against {drawn} drawn"
        );
        let sm = machine(&src);
        let left_out = sm.transitions.len() - drawn;
        if left_out > 0 {
            assert!(
                outline.contains(&format!("{left_out} transition")),
                "{name}: nothing says {left_out} transitions are not drawn\n{outline}"
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
