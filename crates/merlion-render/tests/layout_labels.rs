//! Edge labels and routes over the compat corpus (specs/layout.md#6-edge-routing): a chip
//! never covers the arrowhead of its own edge, and neither chips nor edges cover a cluster
//! title.

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

type Box4 = (f64, f64, f64, f64);

fn title_boxes(g: &Geometry) -> Vec<Box4> {
    g.clusters
        .iter()
        .filter(|c| c.label.width > 0.0)
        .map(|c| {
            let (w, h) = (c.label.width, c.label.height);
            (
                c.label_x - w / 2.0,
                c.label_y - h / 2.0,
                c.label_x + w / 2.0,
                c.label_y + h / 2.0,
            )
        })
        .collect()
}

/// Whether segment a–b passes through the interior of `r` (Liang–Barsky clip).
fn crosses(a: (f64, f64), b: (f64, f64), r: Box4) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-dx, a.0 - r.0),
        (dx, r.2 - a.0),
        (-dy, a.1 - r.1),
        (dy, r.3 - a.1),
    ] {
        if p == 0.0 {
            if q <= 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    t1 - t0 > 1e-6
}

#[test]
fn chips_and_edges_keep_off_cluster_titles() {
    let mut chips = Vec::new();
    let mut edges = Vec::new();
    for (name, c) in corpus() {
        let o = RenderOptions::default();
        let mut fuel = Fuel::new(o.fuel);
        let mut d = Diagnostics::new(false);
        let Ok(g) = layout_flowchart(&c, &o, &mut fuel, &mut d) else {
            continue;
        };
        let titles = title_boxes(&g);
        for eg in &g.edges {
            if let Some(l) = &eg.label {
                let (w, h) = chip_size(&l.label);
                let chip = (l.x - w / 2.0, l.y - h / 2.0, l.x + w / 2.0, l.y + h / 2.0);
                if titles
                    .iter()
                    .any(|t| chip.0 < t.2 && t.0 < chip.2 && chip.1 < t.3 && t.1 < chip.3)
                {
                    chips.push(name.clone());
                }
            }
            let hit = eg.points.windows(2).any(|s| {
                titles
                    .iter()
                    .any(|&t| crosses((s[0].x, s[0].y), (s[1].x, s[1].y), t))
            });
            if hit {
                edges.push(name.clone());
            }
        }
    }
    assert!(
        chips.is_empty() && edges.is_empty(),
        "chips {:?}\nedges {:?}",
        chips,
        edges
    );
}
