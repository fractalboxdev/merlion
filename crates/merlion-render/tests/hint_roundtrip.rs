//! Re-rendering in place is byte-stable: a diagram rendered with its own previous SVG as
//! the layout hint reproduces that SVG (specs/svg-output.md#layout-hint, `diagram_id`).

use merlion_render::{render, RenderOptions};

fn roundtrip(name: &str, src: &str) -> Option<String> {
    let first = render(src, &RenderOptions::default()).svg?;
    let opts = RenderOptions {
        hint: Some(first.clone()),
        ..RenderOptions::default()
    };
    let second = render(src, &opts).svg;
    (second.as_deref() != Some(first.as_str())).then(|| name.to_string())
}

#[test]
fn a_render_hinted_with_itself_is_unchanged() {
    let src = "graph LR\nA-->B\nC-->B\nD-->B\nE-->A\nF-->E\nG-->E\nH-->C\nH-->D\nH-->G\nF-->H\n";
    assert_eq!(roundtrip("min", src), None);
}

#[test]
fn every_compat_diagram_is_stable_under_its_own_hint() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/corpus/compat");
    let Ok(rd) = std::fs::read_dir(dir) else {
        eprintln!("note: {dir} missing; corpus check skipped");
        return;
    };
    let mut files: Vec<_> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "mmd"))
        .collect();
    files.sort();
    let unstable: Vec<String> = files
        .iter()
        .filter_map(|p| {
            let src = std::fs::read_to_string(p).ok()?;
            roundtrip(&p.file_name()?.to_string_lossy(), &src)
        })
        .collect();
    assert!(unstable.is_empty(), "{:?}", unstable);
}
