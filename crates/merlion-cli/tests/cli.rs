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

// ---------------------------------------------------------------------------------------
// Stylesheets (specs/integrations.md#cli, specs/svg-output.md#stylesheet)

const SHEET: &str = r#"
:root { --merlion-accent: #0f766e; --merlion-fg: #202830; }
[data-theme="dark"] { --merlion-bg: #101418; --merlion-fg: #e6e6e6; }
[data-theme="brand"] { --merlion-bg: #fdf6e3; }
.merlion-c-store { --merlion-tone: #b8408f; --merlion-dash: 4 2; }
.viewer-rule { color: red; }
"#;

const ROLES: &str = "flowchart LR\nA-->B\nclass A store\n";

#[test]
fn css_compiles_to_page_css_on_stdout_or_a_file() {
    let d = tempdir("css");
    fs::write(d.join("site.css"), SHEET).unwrap();
    let o = merlion(&d, &["css", "site.css"], None);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let css = stdout(&o);
    assert!(
        css.starts_with(":root {\n  --merlion-fg: #202830;\n  --merlion-accent: #0f766e;\n}\n"),
        "{css}"
    );
    assert!(
        css.contains(
            ".merlion .merlion-c-store {\n  --merlion-tone: #b8408f;\n  --merlion-dash: 4 2;\n}\n"
        ),
        "{css}"
    );
    assert!(!css.contains("viewer-rule"), "{css}");
    // The token-free rule is reported once, located, as I032.
    assert!(stderr(&o).starts_with("site.css:"), "{}", stderr(&o));
    assert!(stderr(&o).contains(" info I032 "), "{}", stderr(&o));
    // -o writes atomically; compiling the output again gives the same bytes.
    let o = merlion(
        &d,
        &["css", "site.css", "-o", "out/site.compiled.css"],
        None,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(o.stdout.is_empty());
    let written = fs::read_to_string(d.join("out/site.compiled.css")).unwrap();
    assert_eq!(written, css);
    assert_eq!(names(&d.join("out")), vec!["site.compiled.css"]);
    let o = merlion(&d, &["css", "out/site.compiled.css"], None);
    assert_eq!(stdout(&o), css);
    assert!(stderr(&o).is_empty(), "{}", stderr(&o));
    // Standard input works too.
    let o = merlion(&d, &["css"], Some(SHEET));
    assert_eq!(stdout(&o), css);
    assert!(stderr(&o).starts_with("<stdin>:"), "{}", stderr(&o));
}

#[test]
fn css_warnings_pass_and_strict_makes_them_errors() {
    let d = tempdir("cssstrict");
    let sheet =
        ":root { --merlion-bg: #fff; --merlion-font: Comic; }\nbody { --merlion-fg: red; }\n";
    fs::write(d.join("s.css"), sheet).unwrap();
    let o = merlion(&d, &["css", "s.css", "-o", "o.css"], None);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let err = stderr(&o);
    assert!(
        err.contains("s.css:1:") && err.contains(" warning W018 "),
        "{err}"
    );
    assert!(err.contains(" warning W017 "), "{err}");
    assert!(d.join("o.css").is_file());
    fs::remove_file(d.join("o.css")).unwrap();
    let o = merlion(&d, &["css", "s.css", "-o", "o.css", "--strict"], None);
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains(" error W018 "), "{}", stderr(&o));
    assert!(!d.join("o.css").exists());
}

#[test]
fn css_limits_exit_3_with_e013() {
    let d = tempdir("cssbig");
    fs::write(
        d.join("big.css"),
        format!(":root{{--merlion-bg:#fff;}}\n/*{}*/", "x".repeat(64 * 1024)),
    )
    .unwrap();
    for args in [&["css", "big.css"][..], &["render", "--css", "big.css"]] {
        let o = merlion(&d, args, Some(VALID));
        assert_eq!(o.status.code(), Some(3), "{args:?}: {}", stderr(&o));
        assert!(
            stderr(&o).contains("big.css:1:1: error E013"),
            "{}",
            stderr(&o)
        );
        assert!(o.stdout.is_empty());
    }
    let rules: String = (0..600)
        .map(|i| format!(".merlion-c-r{i} {{ --merlion-tone: #123456; }}\n"))
        .collect();
    fs::write(d.join("many.css"), rules).unwrap();
    let o = merlion(&d, &["css", "many.css"], None);
    assert_eq!(o.status.code(), Some(3), "{}", stderr(&o));
    assert!(stderr(&o).contains("E013"), "{}", stderr(&o));
}

#[test]
fn css_rejects_non_utf8_and_usage_errors() {
    let d = tempdir("cssusage");
    fs::write(d.join("bin.css"), [0xff, 0xfe]).unwrap();
    let o = merlion(&d, &["css", "bin.css"], None);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("not UTF-8"), "{}", stderr(&o));
    for args in [
        &["css", "a.css", "b.css"][..],
        &["css", "--width", "3"],
        &["render", "--theme", "dark"],
        &["render", "--auto-dark", "dark"],
    ] {
        let o = merlion(&d, args, Some(VALID));
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", stderr(&o));
        assert!(stderr(&o).contains("Usage:"), "{args:?}");
    }
}

#[cfg(unix)]
#[test]
fn css_inputs_follow_the_file_handling_rules() {
    let d = tempdir("csslink");
    fs::create_dir(d.join("work")).unwrap();
    fs::write(
        d.join("secret"),
        ":root{--merlion-bg:#fff}\nGITHUB_TOKEN=ghs_secretvalue123\n",
    )
    .unwrap();
    let w = d.join("work");
    std::os::unix::fs::symlink(d.join("secret"), w.join("s.css")).unwrap();
    fs::write(w.join("in.mmd"), VALID).unwrap();
    for args in [
        &["css", "s.css"][..],
        &["render", "in.mmd", "--css", "s.css"],
        &["css", "../secret"],
        &["render", "in.mmd", "--css", "../secret"],
    ] {
        let o = merlion(&w, args, None);
        assert_eq!(o.status.code(), Some(1), "{args:?}: {}", stderr(&o));
        assert!(!stderr(&o).contains("ghs_"), "{args:?}");
        assert!(!stdout(&o).contains("ghs_"), "{args:?}");
        assert!(
            stderr(&o).contains("refusing to read"),
            "{args:?}: {}",
            stderr(&o)
        );
    }
    let o = merlion(&w, &["css", "s.css", "--follow-symlinks"], None);
    assert!(stdout(&o).starts_with(":root"), "{}", stderr(&o));
}

#[test]
fn render_css_bakes_the_chosen_theme() {
    if !core_renders() {
        return;
    }
    let d = tempdir("bake");
    fs::write(d.join("site.css"), SHEET).unwrap();
    fs::write(d.join("in.mmd"), ROLES).unwrap();
    let plain = stdout(&merlion(&d, &["render", "in.mmd"], None));
    let o = merlion(&d, &["render", "in.mmd", "--css", "site.css"], None);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let root = stdout(&o);
    assert_ne!(root, plain);
    assert!(root.contains("#b8408f"), "role tone baked");
    assert!(root.contains("#202830"), "foreground baked");
    assert!(!root.contains("#101418"), "--theme defaults to :root alone");
    // Layout is untouched by the palette.
    let layout = |s: &str| {
        s.split("data-merlion-layout=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .map(String::from)
    };
    assert_eq!(layout(&root), layout(&plain));
    let o = merlion(
        &d,
        &["render", "in.mmd", "--css", "site.css", "--theme", "dark"],
        None,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let dark = stdout(&o);
    assert!(dark.contains("#101418"), "dark bg baked");
    assert!(!dark.contains("prefers-color-scheme"));
    let o = merlion(
        &d,
        &[
            "render",
            "in.mmd",
            "--css",
            "site.css",
            "--theme",
            "brand",
            "--auto-dark",
            "dark",
        ],
        None,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let auto = stdout(&o);
    assert!(auto.contains("#fdf6e3") && auto.contains("@media (prefers-color-scheme: dark)"));
    // The same palette gives the same bytes; the palette joins the id.
    assert_eq!(
        stdout(&merlion(
            &d,
            &["render", "in.mmd", "--css", "site.css"],
            None
        )),
        root
    );
    // The stylesheet's I032 is reported under its own name.
    assert!(stderr(&o).contains("site.css:"), "{}", stderr(&o));
}

#[test]
fn render_css_unknown_theme_is_a_usage_error() {
    let d = tempdir("baketheme");
    fs::write(d.join("site.css"), SHEET).unwrap();
    fs::write(d.join("in.mmd"), VALID).unwrap();
    for flag in ["--theme", "--auto-dark"] {
        let o = merlion(
            &d,
            &["render", "in.mmd", "--css", "site.css", flag, "nope"],
            None,
        );
        assert_eq!(o.status.code(), Some(2), "{flag}: {}", stderr(&o));
        assert!(stderr(&o).contains("`nope`"), "{}", stderr(&o));
        assert!(o.stdout.is_empty());
    }
    // The automatic dark block is not a theme name.
    fs::write(
        d.join("auto.css"),
        "@media (prefers-color-scheme: dark) { :root:not([data-theme]) { --merlion-bg: #000; } }\n",
    )
    .unwrap();
    let o = merlion(
        &d,
        &["render", "in.mmd", "--css", "auto.css", "--theme", "dark"],
        None,
    );
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
}

#[test]
fn render_css_applies_to_every_markdown_block_and_batch_file() {
    if !core_renders() {
        return;
    }
    let d = tempdir("bakemd");
    fs::write(d.join("site.css"), SHEET).unwrap();
    fs::write(
        d.join("doc.md"),
        format!("```mermaid\n{ROLES}```\n\n```mermaid\n{ROLES}```\n"),
    )
    .unwrap();
    let o = merlion(
        &d,
        &[
            "render", "doc.md", "-o", "out", "--css", "site.css", "--theme", "dark",
        ],
        None,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    // Parsed once: the I032 of the stylesheet is printed once.
    assert_eq!(stderr(&o).matches("I032").count(), 1, "{}", stderr(&o));
    for n in [1, 2] {
        let svg = fs::read_to_string(d.join(format!("out/doc-{n}.svg"))).unwrap();
        assert!(svg.contains("#101418") && svg.contains("#b8408f"));
    }
    fs::create_dir(d.join("corpus")).unwrap();
    fs::write(d.join("corpus/a.mmd"), ROLES).unwrap();
    let o = merlion(
        &d,
        &[
            "render", "--batch", "corpus", "-o", "b", "--css", "site.css",
        ],
        None,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(fs::read_to_string(d.join("b/a.svg"))
        .unwrap()
        .contains("#b8408f"));
}

#[test]
fn no_auto_tone_matches_the_core_option_byte_for_byte() {
    if !core_renders() {
        return;
    }
    let d = tempdir("auto-tone");
    let src = "flowchart LR\nsubgraph g [G]\n  d{D} --> c[(C)]\nend\nc --> s([S])\n";
    let o = merlion(&d, &["render", "--no-auto-tone"], Some(src));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let off = merlion_render::render(
        src,
        &merlion_render::RenderOptions {
            auto_tone: false,
            ..Default::default()
        },
    )
    .svg
    .unwrap();
    assert_eq!(stdout(&o), off);
    assert!(!off.contains("merlion-auto"));
    let o = merlion(&d, &["render"], Some(src));
    assert!(stdout(&o).contains("merlion-c-warn merlion-auto"));
    let o = merlion(&d, &["render", "--no-auto-tone=yes"], Some(src));
    assert_eq!(o.status.code(), Some(2));
}

// ---------------------------------------------------------------------------------------
// Sequence diagrams (specs/sequence.md). Every command is diagram-agnostic: the same output
// paths, exit codes and diagnostic formatting carry a sequence.

const SEQ: &str = "sequenceDiagram\n    actor Alice\n    participant Bob\n    Alice->>+Bob: Hello\n    Bob-->>-Alice: Hi\n";

#[test]
fn sequence_renders_through_every_output_path() {
    if !core_renders() {
        return;
    }
    let d = tempdir("seq-render");
    let o = merlion(&d, &["render"], Some(SEQ));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(
        stdout(&o).contains(r#"class="merlion merlion-sequence""#),
        "{}",
        stdout(&o)
    );

    fs::write(d.join("in.mmd"), SEQ).unwrap();
    let o = merlion(
        &d,
        &["render", "in.mmd", "-o", "out.svg", "--outline", "out.txt"],
        None,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let outline = fs::read_to_string(d.join("out.txt")).unwrap();
    assert!(
        outline.starts_with("Sequence diagram. 2 participants, 2 messages."),
        "{outline}"
    );
    assert!(outline.contains("1. Alice → Bob: Hello"), "{outline}");
    assert_eq!(names(&d), vec!["in.mmd", "out.svg", "out.txt"]);

    // A sequence's geometry is the source's, so a re-render hinted with its own output repeats it.
    let before = fs::read(d.join("out.svg")).unwrap();
    let o = merlion(&d, &["render", "in.mmd", "-o", "out.svg"], None);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert_eq!(fs::read(d.join("out.svg")).unwrap(), before);
}

#[test]
fn sequence_renders_in_markdown_and_in_batch() {
    if !core_renders() {
        return;
    }
    let d = tempdir("seq-md");
    fs::create_dir(d.join("out")).unwrap();
    let md = format!("# Doc\n\n```mermaid\n{SEQ}```\n\ntext\n\n```mermaid\n{VALID}```\n");
    fs::write(d.join("doc.md"), md).unwrap();
    let o = merlion(&d, &["render", "doc.md", "-o", "out"], None);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(fs::read_to_string(d.join("out/doc-1.svg"))
        .unwrap()
        .contains("merlion-sequence"));
    assert!(!fs::read_to_string(d.join("out/doc-2.svg"))
        .unwrap()
        .contains("merlion-sequence"));

    let d = tempdir("seq-batch");
    fs::create_dir(d.join("corpus")).unwrap();
    fs::create_dir(d.join("out")).unwrap();
    fs::write(d.join("corpus/seq.mmd"), SEQ).unwrap();
    fs::write(d.join("corpus/flow.mmd"), VALID).unwrap();
    let o = merlion(
        &d,
        &["render", "--batch", "corpus", "-o", "out", "--json-summary"],
        None,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let out = stdout(&o);
    assert_eq!(out.lines().count(), 2, "{out}");
    assert!(out.lines().all(|l| l.contains(r#""ok":true"#)), "{out}");
    assert!(fs::read_to_string(d.join("out/seq.svg"))
        .unwrap()
        .contains("merlion-sequence"));
}

#[test]
fn sequence_json_carries_the_svg_and_the_outline() {
    if !core_renders() {
        return;
    }
    let d = tempdir("seq-json");
    let o = merlion(&d, &["render", "--json"], Some(SEQ));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.starts_with(r#"{"svg":"<svg"#), "{out}");
    assert!(
        out.contains(r#""outline":"Sequence diagram. 2 participants, 2 messages."#),
        "{out}"
    );
    assert!(out.contains(r#""error":null}"#), "{out}");
}

#[test]
fn sequence_outline_prints_the_text_alternative() {
    if !core_renders() {
        return;
    }
    let d = tempdir("seq-outline");
    let o = merlion(&d, &["outline"], Some(SEQ));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.starts_with("Sequence diagram. 2 participants, 2 messages."),
        "{out}"
    );
    assert!(out.contains("Participants: Alice, Bob."), "{out}");
    assert!(out.contains("2. Bob --> Alice: Hi"), "{out}");
}

#[test]
fn sequence_check_locates_diagnostics_and_fix_applies_the_repair() {
    let d = tempdir("seq-check");
    // A message line with no colon before its text is R013 MessageTextUnmarked (specs/sequence.md#diagnostics).
    let src = "sequenceDiagram\n    participant Alice\n    Alice->>Bob Hello\n";
    let o = merlion(&d, &["check"], Some(src));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let err = stderr(&o);
    assert!(err.contains("<stdin>:3:"), "{err}");
    assert!(err.contains("R013"), "{err}");

    fs::write(d.join("in.mmd"), src).unwrap();
    let o = merlion(&d, &["check", "in.mmd", "--fix"], None);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let after = fs::read_to_string(d.join("in.mmd")).unwrap();
    assert_eq!(
        after,
        "sequenceDiagram\n    participant Alice\n    Alice->>Bob: Hello\n"
    );
    assert!(merlion_render::check(&after, false)
        .iter()
        .all(|x| x.fix.is_none()));
    assert_eq!(names(&d), vec!["in.mmd"]);

    // An error in a sequence exits 1 and prints in the shared format.
    fs::write(d.join("bad.mmd"), "sequenceDiagram\n    participant\n").unwrap();
    let o = merlion(&d, &["check", "bad.mmd"], None);
    let err = stderr(&o);
    assert!(err.starts_with("bad.mmd:2:"), "{err}");
    assert_eq!(o.status.code(), Some(1), "{err}");
}

// ---------------------------------------------------------------------------------------
// State diagrams (specs/state.md). A state machine lowers onto the flowchart engine, so the
// commands carry it exactly as they carry a flowchart, hint included.

const STATE: &str =
    "stateDiagram-v2\n    [*] --> Idle\n    Idle --> Busy : work arrives\n    Busy --> Idle : the queue drains\n    Busy --> [*]\n";

#[test]
fn state_renders_through_every_output_path() {
    if !core_renders() {
        return;
    }
    let d = tempdir("state-render");
    let o = merlion(&d, &["render"], Some(STATE));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(
        stdout(&o).contains(r#"class="merlion merlion-state""#),
        "{}",
        stdout(&o)
    );

    fs::write(d.join("in.mmd"), STATE).unwrap();
    let o = merlion(
        &d,
        &["render", "in.mmd", "-o", "out.svg", "--outline", "out.txt"],
        None,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let outline = fs::read_to_string(d.join("out.txt")).unwrap();
    assert!(
        outline.starts_with("State diagram, top to bottom. 4 states, 4 transitions."),
        "{outline}"
    );
    assert!(outline.contains("Idle → Busy [work arrives]"), "{outline}");
    assert_eq!(names(&d), vec!["in.mmd", "out.svg", "out.txt"]);

    // The hint is the flowchart's, so a re-render reading the previous output repeats it.
    let before = fs::read(d.join("out.svg")).unwrap();
    let o = merlion(&d, &["render", "in.mmd", "-o", "out.svg"], None);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert_eq!(fs::read(d.join("out.svg")).unwrap(), before);
}

#[test]
fn state_renders_in_markdown_and_in_batch() {
    if !core_renders() {
        return;
    }
    let d = tempdir("state-md");
    fs::create_dir(d.join("out")).unwrap();
    let md = format!("# Doc\n\n```mermaid\n{STATE}```\n\ntext\n\n```mermaid\n{VALID}```\n");
    fs::write(d.join("doc.md"), md).unwrap();
    let o = merlion(&d, &["render", "doc.md", "-o", "out"], None);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(fs::read_to_string(d.join("out/doc-1.svg"))
        .unwrap()
        .contains("merlion-state"));
    assert!(!fs::read_to_string(d.join("out/doc-2.svg"))
        .unwrap()
        .contains("merlion-state"));

    let d = tempdir("state-batch");
    fs::create_dir(d.join("corpus")).unwrap();
    fs::create_dir(d.join("out")).unwrap();
    fs::write(d.join("corpus/state.mmd"), STATE).unwrap();
    fs::write(d.join("corpus/seq.mmd"), SEQ).unwrap();
    fs::write(d.join("corpus/flow.mmd"), VALID).unwrap();
    let o = merlion(
        &d,
        &["render", "--batch", "corpus", "-o", "out", "--json-summary"],
        None,
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let out = stdout(&o);
    assert_eq!(out.lines().count(), 3, "{out}");
    assert!(out.lines().all(|l| l.contains(r#""ok":true"#)), "{out}");
    assert!(fs::read_to_string(d.join("out/state.svg"))
        .unwrap()
        .contains("merlion-state"));
}

#[test]
fn state_json_carries_the_svg_and_the_outline() {
    if !core_renders() {
        return;
    }
    let d = tempdir("state-json");
    let o = merlion(&d, &["render", "--json"], Some(STATE));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.starts_with(r#"{"svg":"<svg"#), "{out}");
    assert!(
        out.contains(r#""outline":"State diagram, top to bottom. 4 states, 4 transitions."#),
        "{out}"
    );
    assert!(out.contains(r#""error":null}"#), "{out}");
}

#[test]
fn state_outline_prints_the_text_alternative() {
    if !core_renders() {
        return;
    }
    let d = tempdir("state-outline");
    let o = merlion(&d, &["outline"], Some(STATE));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.starts_with("State diagram, top to bottom. 4 states, 4 transitions."),
        "{out}"
    );
    assert!(out.contains("start → Idle"), "{out}");
    assert!(
        out.contains("Busy → Idle [the queue drains]; → end"),
        "{out}"
    );
}

#[test]
fn state_check_locates_diagnostics_and_fix_applies_the_repair() {
    let d = tempdir("state-check");
    // An arrow other than `-->` is R018 (specs/state.md#diagnostics).
    let src = "stateDiagram-v2\n    [*] --> Idle\n    Idle ->> Busy : work arrives\n";
    let o = merlion(&d, &["check"], Some(src));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let err = stderr(&o);
    assert!(err.contains("<stdin>:3:"), "{err}");
    assert!(err.contains("R018"), "{err}");

    fs::write(d.join("in.mmd"), src).unwrap();
    let o = merlion(&d, &["check", "in.mmd", "--fix"], None);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let after = fs::read_to_string(d.join("in.mmd")).unwrap();
    assert_eq!(
        after,
        "stateDiagram-v2\n    [*] --> Idle\n    Idle --> Busy : work arrives\n"
    );
    assert!(merlion_render::check(&after, false)
        .iter()
        .all(|x| x.fix.is_none()));
    assert_eq!(names(&d), vec!["in.mmd"]);

    // An error in a state diagram exits 1 and prints in the shared format.
    fs::write(d.join("bad.mmd"), "stateDiagram-v2\n    state\n").unwrap();
    let o = merlion(&d, &["check", "bad.mmd"], None);
    let err = stderr(&o);
    assert!(err.starts_with("bad.mmd:2:"), "{err}");
    assert_eq!(o.status.code(), Some(1), "{err}");
}
