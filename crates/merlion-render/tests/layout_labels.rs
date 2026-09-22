//! Edge-label chips over the compat corpus (specs/layout.md#6-edge-routing): a chip never
//! covers the arrowhead of its own edge.

use merlion_render::diag::Diagnostics;
use merlion_render::fuel::Fuel;
use merlion_render::geometry::{chip_size, Geometry};
use merlion_render::layout::layout_flowchart;
use merlion_render::model::{Arrow, Diagram, Flowchart};
use merlion_render::parse::{parse, ParseOptions};
use merlion_render::RenderOptions;

/// Length of an end marker along the path (10 × 10 markers, svg/mod.rs).
const MARKER: f64 = 10.0;

fn corpus() -> Vec<(String, Flowchart)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/corpus/compat");
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "mmd"))
        .collect();
    files.sort();
    let o = RenderOptions::default();
    let po = ParseOptions {
        strict: false,
        limits: o.limits,
    };
    files
        .iter()
        .filter_map(|p| {
            let src = std::fs::read_to_string(p).ok()?;
            let mut d = Diagnostics::new(false);
            match parse(&src, &po, &mut d).ok()? {
                Diagram::Flowchart(c) => Some((p.file_name()?.to_string_lossy().into(), c)),
            }
        })
        .collect()
}

/// The box an end marker covers: the last `MARKER` px of the path, `MARKER / 2` wide.
fn marker_box(tip: (f64, f64), from: (f64, f64)) -> (f64, f64, f64, f64) {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    let base = (tip.0 - dx / len * MARKER, tip.1 - dy / len * MARKER);
    let (nx, ny) = (-dy / len * MARKER / 2.0, dx / len * MARKER / 2.0);
    let xs = [tip.0, base.0 + nx, base.0 - nx];
    let ys = [tip.1, base.1 + ny, base.1 - ny];
    let f = |v: &[f64; 3], lo: bool| {
        v.iter()
            .copied()
            .fold(if lo { f64::MAX } else { f64::MIN }, |a, b| {
                if lo {
                    a.min(b)
                } else {
                    a.max(b)
                }
            })
    };
    (f(&xs, true), f(&ys, true), f(&xs, false), f(&ys, false))
}

fn covered(c: &Flowchart, g: &Geometry) -> usize {
    let mut n = 0;
    for (e, eg) in g.edges.iter().enumerate() {
        let (Some(l), Some(edge)) = (&eg.label, c.edges.get(e)) else {
            continue;
        };
        let pts = &eg.points;
        if pts.len() < 2 {
            continue;
        }
        let (w, h) = chip_size(&l.label);
        let chip = (l.x - w / 2.0, l.y - h / 2.0, l.x + w / 2.0, l.y + h / 2.0);
        let mut ends = Vec::new();
        if edge.arrow_end != Arrow::None {
            let (a, b) = (pts[pts.len() - 1], pts[pts.len() - 2]);
            ends.push(marker_box((a.x, a.y), (b.x, b.y)));
        }
        if edge.arrow_start != Arrow::None {
            ends.push(marker_box((pts[0].x, pts[0].y), (pts[1].x, pts[1].y)));
        }
        let hit =
            |m: (f64, f64, f64, f64)| chip.0 < m.2 && m.0 < chip.2 && chip.1 < m.3 && m.1 < chip.3;
        if ends.into_iter().any(hit) {
            n += 1;
        }
    }
    n
}

#[test]
fn no_chip_covers_its_own_arrowhead() {
    let mut bad = Vec::new();
    for (name, c) in corpus() {
        let o = RenderOptions::default();
        let mut fuel = Fuel::new(o.fuel);
        let mut d = Diagnostics::new(false);
        let Ok(g) = layout_flowchart(&c, &o, &mut fuel, &mut d) else {
            continue;
        };
        let k = covered(&c, &g);
        if k > 0 {
            bad.push((name, k));
        }
    }
    let total: usize = bad.iter().map(|b| b.1).sum();
    assert!(
        bad.is_empty(),
        "{} chips in {} diagrams: {:?}",
        total,
        bad.len(),
        bad
    );
}
