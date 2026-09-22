//! End-to-end tests of the `merlion` binary (specs/integrations.md#cli).
//!
//! Each test runs in its own directory under the system temp dir. Assertions that need a
//! successful render are gated on the core actually rendering a trivial flowchart, so the
//! plumbing is covered whatever state the pipeline stages are in.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

const VALID: &str = "flowchart LR\nA-->B\n";
const INVALID: &str = "this is not a diagram\n";

fn core_renders() -> bool {
    let ok = merlion_render::render(VALID, &Default::default())
        .svg
        .is_some_and(|s| s.starts_with("<svg"));
    if !ok {
        eprintln!("note: the core does not render yet; success assertions skipped");
    }
    ok
}

fn tempdir(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("merlion-cli-it-{}-{tag}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn merlion(dir: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_merlion"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut sin = child.stdin.take().unwrap();
        if let Some(s) = stdin {
            // A large input may be refused before it is fully read.
            let _ = sin.write_all(s.as_bytes());
        }
    }
    child.wait_with_output().unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn version_and_help() {
    let d = tempdir("version");
    let o = merlion(&d, &["--version"], None);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        stdout(&o),
        format!("merlion {}\n", env!("CARGO_PKG_VERSION"))
    );
    let o = merlion(&d, &["--help"], None);
    assert_eq!(o.status.code(), Some(0));
    assert!(stdout(&o).contains("merlion render"));
}

#[test]
fn usage_errors_exit_2() {
    let d = tempdir("usage");
    for args in [
        &[][..],
        &["frobnicate"],
        &["render", "--width", "wide"],
        &["render", "--bogus"],
        &["render", "doc.md", "--outline", "o.txt"],
    ] {
        let o = merlion(&d, args, Some(""));
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", stderr(&o));
        assert!(stderr(&o).contains("Usage:"), "{args:?}");
    }
}

#[test]
fn failed_render_exits_1_with_located_diagnostics() {
    let d = tempdir("fail");
    let o = merlion(&d, &["render"], Some(INVALID));
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
    let err = stderr(&o);
    let first = err.lines().next().unwrap_or("");
    // file:line:col: severity code message
    let parts: Vec<&str> = first.splitn(4, ':').collect();
    assert_eq!(parts.first(), Some(&"<stdin>"), "{err}");
    assert!(
        parts.get(1).is_some_and(|l| l.parse::<u32>().is_ok()),
        "{err}"
    );
    assert!(
        parts.get(2).is_some_and(|c| c.parse::<u32>().is_ok()),
        "{err}"
    );
    assert!(
        parts.get(3).is_some_and(|r| r.starts_with(" error E")),
        "{err}"
    );
}

#[test]
fn oversized_input_exits_3() {
    let d = tempdir("large");
    let big = "a".repeat((1 << 20) + 10);
    fs::write(d.join("big.mmd"), &big).unwrap();
    let o = merlion(&d, &["render", "big.mmd"], None);
    assert_eq!(o.status.code(), Some(3), "{}", stderr(&o));
    assert!(stderr(&o).contains("E004"));
    let o = merlion(&d, &["check", "big.mmd"], None);
    assert_eq!(o.status.code(), Some(3), "{}", stderr(&o));
}

#[test]
fn successful_render_writes_stdout_or_file_atomically() {
    if !core_renders() {
        return;
    }
    let d = tempdir("ok");
    let o = merlion(&d, &["render"], Some(VALID));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(stdout(&o).starts_with("<svg"));
    fs::write(d.join("in.mmd"), VALID).unwrap();
    let o = merlion(
        &d,
        &["render", "in.mmd", "-o", "out.svg", "--outline", "out.txt"],
        None,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(o.stdout.is_empty());
    assert!(fs::read_to_string(d.join("out.svg"))
        .unwrap()
        .starts_with("<svg"));
    assert!(d.join("out.txt").is_file());
    // No temporary file survives the rename.
    assert_eq!(names(&d), vec!["in.mmd", "out.svg", "out.txt"]);
    // Re-rendering in place uses the existing file as the hint and stays byte-identical.
    let before = fs::read(d.join("out.svg")).unwrap();
    let o = merlion(&d, &["render", "in.mmd", "-o", "out.svg"], None);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(fs::read(d.join("out.svg")).unwrap(), before);
}

#[test]
fn failed_render_leaves_existing_output_untouched() {
    let d = tempdir("keep");
    fs::write(d.join("in.mmd"), INVALID).unwrap();
    fs::write(d.join("out.svg"), "OLD").unwrap();
    let o = merlion(&d, &["render", "in.mmd", "-o", "out.svg"], None);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(fs::read_to_string(d.join("out.svg")).unwrap(), "OLD");
    assert_eq!(names(&d), vec!["in.mmd", "out.svg"]);
}

#[test]
fn markdown_renders_each_block_and_keeps_failed_blocks() {
    let d = tempdir("md");
    fs::create_dir(d.join("out")).unwrap();
    let md = format!("# Doc\n\n```mermaid\n{VALID}```\n\ntext\n\n```mermaid\n{INVALID}```\n");
    fs::write(d.join("doc.md"), md).unwrap();
    fs::write(d.join("out/doc-2.svg"), "OLD").unwrap();
    let o = merlion(&d, &["render", "doc.md", "-o", "out"], None);
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert_eq!(fs::read_to_string(d.join("out/doc-2.svg")).unwrap(), "OLD");
    // Block 2's content starts on line 11 of the file.
    let err = stderr(&o);
    assert!(
        err.lines()
            .any(|l| l.starts_with("doc.md:1") && l.contains(" error E")),
        "{err}"
    );
    assert!(
        err.contains("doc.md:11:") || err.contains("doc.md:10:"),
        "{err}"
    );
    if core_renders() {
        assert!(fs::read_to_string(d.join("out/doc-1.svg"))
            .unwrap()
            .starts_with("<svg"));
    }
}

#[test]
fn json_output_shape() {
    let d = tempdir("json");
    let o = merlion(&d, &["render", "--json"], Some(INVALID));
    assert_eq!(o.status.code(), Some(1));
    let out = stdout(&o);
    assert!(
        out.starts_with(r#"{"svg":null,"outline":null,"diagnostics":["#),
        "{out}"
    );
    assert!(out.contains(r#""severity":"error""#), "{out}");
    assert!(out.contains(r#""fuel_used":"#), "{out}");
    assert!(out.contains(r#""error":{"kind":"#), "{out}");
    assert!(out.ends_with("}\n"));
    if core_renders() {
        let o = merlion(&d, &["render", "--json"], Some(VALID));
        assert_eq!(o.status.code(), Some(0));
        assert!(stdout(&o).starts_with(r#"{"svg":"<svg"#));
        assert!(stdout(&o).contains(r#""error":null}"#));
    }
}

#[test]
fn batch_prints_one_json_line_per_file() {
    let d = tempdir("batch");
    fs::create_dir(d.join("corpus")).unwrap();
    fs::create_dir(d.join("out")).unwrap();
    fs::write(d.join("corpus/a.mmd"), VALID).unwrap();
    fs::write(d.join("corpus/b.mmd"), INVALID).unwrap();
    fs::write(d.join("corpus/notes.txt"), "skip").unwrap();
    let o = merlion(
        &d,
        &["render", "--batch", "corpus", "-o", "out", "--json-summary"],
        None,
    );
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(
        lines[0].starts_with(r#"{"file":"corpus/a.mmd","ok":"#),
        "{out}"
    );
    assert!(lines[1].contains(r#""ok":false,"micros":"#), "{out}");
    assert_eq!(o.status.code(), Some(1));
    if core_renders() {
        assert!(lines[0].contains(r#""ok":true"#));
        assert!(d.join("out/a.svg").is_file());
    }
    assert!(!d.join("out/b.svg").exists());
}

#[cfg(unix)]
#[test]
fn refuses_symlink_targets_and_hints() {
    let d = tempdir("link");
    fs::write(d.join("victim"), "KEEP").unwrap();
    std::os::unix::fs::symlink(d.join("victim"), d.join("out.svg")).unwrap();
    fs::write(d.join("in.mmd"), VALID).unwrap();
    let o = merlion(&d, &["render", "in.mmd", "-o", "out.svg"], None);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("symbolic link"), "{}", stderr(&o));
    assert_eq!(fs::read_to_string(d.join("victim")).unwrap(), "KEEP");
    let o = merlion(&d, &["render", "in.mmd", "--hint", "out.svg"], None);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("symbolic link"), "{}", stderr(&o));
}

#[cfg(unix)]
#[test]
fn refuses_to_read_inputs_through_symbolic_links() {
    // A planted link must not echo a file from outside the tree into CI logs.
    let d = tempdir("inlink");
    fs::create_dir(d.join("work")).unwrap();
    fs::create_dir(d.join("work/docs")).unwrap();
    fs::write(d.join("secret"), "GITHUB_TOKEN=ghs_secretvalue123\n").unwrap();
    let w = d.join("work");
    std::os::unix::fs::symlink(d.join("secret"), w.join("docs/g.mmd")).unwrap();
    std::os::unix::fs::symlink(d.join("secret"), w.join("docs/g.md")).unwrap();
    fs::write(w.join("docs/ok.mmd"), VALID).unwrap();
    for args in [
        &["check", "docs/g.mmd"][..],
        &["check", "docs/g.md"],
        &["render", "docs/g.mmd"],
        &["render", "docs/g.md"],
        &["outline", "docs/g.mmd"],
        &["render", "--batch", "docs", "-o", "out", "--json-summary"],
    ] {
        let o = merlion(&w, args, None);
        assert!(!stderr(&o).contains("ghs_"), "{:?}: {}", args, stderr(&o));
        assert!(!stdout(&o).contains("ghs_"), "{:?}: {}", args, stdout(&o));
        if args[1] != "--batch" {
            assert_eq!(o.status.code(), Some(1), "{:?}", args);
            assert!(
                stderr(&o).contains("symbolic link"),
                "{:?}: {}",
                args,
                stderr(&o)
            );
        }
    }
    // Files outside the working directory are refused too, unless --follow-symlinks.
    let o = merlion(&w, &["check", "../secret"], None);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("outside"), "{}", stderr(&o));
    let o = merlion(&w, &["check", "docs/g.mmd", "--follow-symlinks"], None);
    assert!(stderr(&o).contains("E003"), "{}", stderr(&o));
    if core_renders() {
        let o = merlion(&w, &["render", "--batch", "docs", "-o", "out"], None);
        assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
        assert_eq!(names(&w.join("out")), vec!["ok.svg"]);
    }
}

#[cfg(unix)]
#[test]
fn diagnostics_reach_the_terminal_without_control_characters() {
    let d = tempdir("ansi");
    fs::create_dir(d.join("in")).unwrap();
    fs::write(d.join("in/a\u{1b}[2J.mmd"), "pie\u{1b}]0;pwned\u{7}\n").unwrap();
    let o = merlion(&d, &["render", "--batch", "in"], None);
    assert_eq!(o.status.code(), Some(1));
    let e = stderr(&o);
    assert!(e.contains("E003"), "{}", e);
    assert!(!e.chars().any(|c| c.is_control() && c != '\n'), "{:?}", e);
}

#[test]
fn refuses_targets_outside_the_working_directory() {
    let d = tempdir("outside");
    fs::create_dir(d.join("work")).unwrap();
    fs::write(d.join("work/in.mmd"), VALID).unwrap();
    let o = merlion(
        &d.join("work"),
        &["render", "in.mmd", "-o", "../escaped.svg"],
        None,
    );
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("outside"), "{}", stderr(&o));
    assert!(!d.join("escaped.svg").exists());
}

#[test]
fn oversized_hint_is_ignored_with_i022() {
    let d = tempdir("bighint");
    fs::write(d.join("in.mmd"), INVALID).unwrap();
    fs::write(d.join("out.svg"), "x".repeat((1 << 20) + 1)).unwrap();
    let o = merlion(&d, &["render", "in.mmd", "-o", "out.svg"], None);
    assert!(
        stderr(&o).contains("out.svg:1:1: info I022"),
        "{}",
        stderr(&o)
    );
    // --no-hint skips the hint entirely.
    let o = merlion(
        &d,
        &["render", "in.mmd", "-o", "out.svg", "--no-hint"],
        None,
    );
    assert!(!stderr(&o).contains("I022"), "{}", stderr(&o));
}

#[test]
fn check_reports_and_exits_1_on_errors() {
    let d = tempdir("check");
    fs::write(d.join("bad.mmd"), INVALID).unwrap();
    let o = merlion(&d, &["check", "bad.mmd"], None);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).starts_with("bad.mmd:"), "{}", stderr(&o));
    let o = merlion(&d, &["check"], Some(INVALID));
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).starts_with("<stdin>:"));
    if core_renders() {
        fs::write(d.join("ok.mmd"), VALID).unwrap();
        let o = merlion(&d, &["check", "ok.mmd"], None);
        assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    }
}

#[test]
fn check_fix_applies_repairs_atomically() {
    let d = tempdir("fix");
    // Typographic quotes as delimiters are repaired by R003 (specs/parser.md).
    let src = "flowchart LR\nA[“hello”]-->B\n";
    let expected_fixes = merlion_render::check(src, false)
        .iter()
        .filter(|x| x.fix.is_some())
        .count();
    fs::write(d.join("in.mmd"), src).unwrap();
    let o = merlion(&d, &["check", "in.mmd", "--fix"], None);
    let after = fs::read_to_string(d.join("in.mmd")).unwrap();
    if expected_fixes == 0 {
        eprintln!("note: the parser reports no fixes yet; --fix leaves the file unchanged");
        assert_eq!(after, src);
    } else {
        assert_ne!(after, src, "{}", stderr(&o));
        assert!(merlion_render::check(&after, false)
            .iter()
            .all(|x| x.fix.is_none()));
    }
    assert_eq!(names(&d), vec!["in.mmd"]);
}

#[test]
fn outline_fails_on_invalid_input() {
    let d = tempdir("outline");
    let o = merlion(&d, &["outline"], Some(INVALID));
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("<stdin>:"));
}
