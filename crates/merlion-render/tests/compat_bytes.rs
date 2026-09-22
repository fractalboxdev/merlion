//! Flowchart output is byte-stable: every diagram in the `compat` corpus renders to the
//! bytes recorded in `fixtures/compat-digests.txt` (specs/architecture.md#determinism).
//!
//! The digest is FNV-1a 64 over the SVG with `data-merlion-version` blanked, so a version
//! bump alone never reddens the test. Run with `MERLION_BLESS=1` to rewrite the fixture
//! after an intended change to flowchart output, and read the diff before committing it.

use std::fmt::Write as _;
use std::path::PathBuf;

use merlion_render::{render, RenderOptions};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/compat-digests.txt"
);
const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/corpus/compat");

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

/// The SVG with the version attribute's value removed.
fn without_version(svg: &str) -> String {
    let key = "data-merlion-version=\"";
    match svg.find(key) {
        Some(i) => {
            let rest = &svg[i + key.len()..];
            let end = rest.find('"').map_or(rest.len(), |j| j);
            format!("{}{}{}", &svg[..i + key.len()], "", &rest[end..])
        }
        None => svg.to_string(),
    }
}

fn corpus() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(CORPUS)
        .expect("compat corpus")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "mmd"))
        .collect();
    files.sort();
    files
}

fn digests() -> String {
    let opts = RenderOptions {
        id_prefix: Some(String::from("m")),
        ..RenderOptions::default()
    };
    let mut out = String::new();
    for path in corpus() {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(svg) = render(&src, &opts).svg else {
            continue;
        };
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let _ = writeln!(
            out,
            "{} {:016x}",
            name,
            fnv1a64(without_version(&svg).as_bytes())
        );
    }
    out
}

#[test]
fn compat_flowcharts_render_the_recorded_bytes() {
    let fresh = digests();
    assert!(
        fresh.lines().count() > 300,
        "corpus shrank to {} rendered diagrams",
        fresh.lines().count()
    );
    if std::env::var_os("MERLION_BLESS").is_some() {
        std::fs::write(FIXTURE, &fresh).expect("write digests");
        return;
    }
    let recorded = std::fs::read_to_string(FIXTURE).expect("read digests");
    let mut changed: Vec<(&str, &str)> = Vec::new();
    for (a, b) in recorded.lines().zip(fresh.lines()) {
        if a != b {
            changed.push((a, b));
        }
    }
    assert!(
        changed.is_empty() && recorded.lines().count() == fresh.lines().count(),
        "flowchart output changed for {} diagrams, first: {:?}\n\
         rerun with MERLION_BLESS=1 when the change is intended",
        changed.len(),
        changed.first()
    );
}
