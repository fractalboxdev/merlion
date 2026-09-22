//! Hand-written argument parser over `std::env::args_os` (specs/supply-chain.md: no
//! argument-parsing crate).

use std::ffi::OsString;
use std::path::PathBuf;

use merlion_render::{EdgeStyle, FontMode};

pub const USAGE: &str = "\
Usage:
  merlion render [<input>] [-o <output>] [--width <px>] [--direction auto|source]
                 [--edge-style orthogonal|polyline|spline] [--font link|embed|system]
                 [--hint <previous.svg>] [--no-hint] [--strict] [--outline <file>]
                 [--id-prefix <prefix>] [--fuel <units>] [--follow-symlinks] [--json]
                 [--no-auto-tone]
                 [--css <file.css> [--theme <name>] [--auto-dark <name>]]
  merlion render --batch <dir> [-o <outdir>] [--json-summary] [render options]
  merlion css [<input.css>] [-o <output.css>] [--strict] [--follow-symlinks]
  merlion check [<input>...] [--strict] [--fix] [--follow-symlinks]
  merlion outline [<input>] [--follow-symlinks]
  merlion --version
  merlion --help

The input defaults to stdin (also `-`) and the output to stdout. A Markdown input
(.md, .mdx) renders every ```mermaid block; -o then names a directory and block n of
<name>.md is written to <dir>/<name>-<n>.svg.

`css` compiles a stylesheet into page CSS: literal token values under fixed selector
shapes. `render --css` bakes it into the SVG: `--theme` picks a [data-theme] block over
:root (default :root alone), `--auto-dark` adds a named block as the
prefers-color-scheme: dark variant. --strict turns W017-W019 into errors.

Decisions, stores, terminals and top-level subgraphs take automatic tones unless an
element has its own class or style; --no-auto-tone draws them untoned.

Exit codes: 0 rendered, 1 failed, 2 usage error, 3 input exceeds limits.
";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub width: Option<f64>,
    pub direction_auto: bool,
    pub edge_style: Option<EdgeStyle>,
    pub font: Option<FontMode>,
    pub hint: Option<PathBuf>,
    pub no_hint: bool,
    pub strict: bool,
    pub outline: Option<PathBuf>,
    pub id_prefix: Option<String>,
    pub fuel: Option<u64>,
    pub follow_symlinks: bool,
    pub json: bool,
    pub batch: Option<PathBuf>,
    pub json_summary: bool,
    pub css: Option<PathBuf>,
    pub theme: Option<String>,
    pub auto_dark: Option<String>,
    pub no_auto_tone: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CssArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub strict: bool,
    pub follow_symlinks: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckArgs {
    pub inputs: Vec<PathBuf>,
    pub strict: bool,
    pub fix: bool,
    pub follow_symlinks: bool,
}

// Parsed once per process; boxing `RenderArgs` would buy nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Help,
    Version,
    Render(RenderArgs),
    Check(CheckArgs),
    Css(CssArgs),
    Outline {
        input: Option<PathBuf>,
        follow_symlinks: bool,
    },
}

/// Parses the arguments after the program name. `Err` carries a usage message (exit 2).
pub fn parse<I: IntoIterator<Item = OsString>>(args: I) -> Result<Command, String> {
    let toks = tokenize(args);
    if toks
        .iter()
        .any(|t| matches!(t, Tok::Opt { name, .. } if name == "--help" || name == "-h"))
    {
        return Ok(Command::Help);
    }
    let mut it = toks.into_iter();
    match it.next() {
        Some(Tok::Opt { name, inline: None }) if name == "--version" || name == "-V" => {
            match it.next() {
                None => Ok(Command::Version),
                Some(_) => Err("--version takes no other arguments".into()),
            }
        }
        Some(Tok::Pos(cmd)) => match cmd.to_str() {
            Some("render") => parse_render(it).map(Command::Render),
            Some("check") => parse_check(it).map(Command::Check),
            Some("css") => parse_css(it).map(Command::Css),
            Some("outline") => parse_outline(it),
            _ => Err(format!("unknown command `{}`", cmd.to_string_lossy())),
        },
        Some(Tok::Opt { name, .. }) => Err(format!("unknown option `{name}`")),
        None => Err("missing command".into()),
    }
}

enum Tok {
    Pos(OsString),
    /// `--name`, `--name=value` or `-o`.
    Opt {
        name: String,
        inline: Option<OsString>,
    },
}

fn tokenize<I: IntoIterator<Item = OsString>>(args: I) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut rest_positional = false;
    for a in args {
        if rest_positional {
            out.push(Tok::Pos(a));
            continue;
        }
        let Some(s) = a.to_str() else {
            // Non-UTF-8 arguments can only be paths.
            out.push(Tok::Pos(a));
            continue;
        };
        if s == "--" {
            rest_positional = true;
        } else if let Some(long) = s.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((n, v)) => (n, Some(OsString::from(v))),
                None => (long, None),
            };
            out.push(Tok::Opt {
                name: format!("--{name}"),
                inline,
            });
        } else if s.len() > 1 && s.starts_with('-') {
            out.push(Tok::Opt {
                name: s.to_string(),
                inline: None,
            });
        } else {
            out.push(Tok::Pos(a));
        }
    }
    out
}

/// The value of an option: inline (`--x=v`) or the next argument.
fn value(
    name: &str,
    inline: Option<OsString>,
    it: &mut impl Iterator<Item = Tok>,
) -> Result<OsString, String> {
    if let Some(v) = inline {
        return Ok(v);
    }
    match it.next() {
        Some(Tok::Pos(v)) => Ok(v),
        _ => Err(format!("`{name}` needs a value")),
    }
}

fn str_value(
    name: &str,
    inline: Option<OsString>,
    it: &mut impl Iterator<Item = Tok>,
) -> Result<String, String> {
    value(name, inline, it)?
        .into_string()
        .map_err(|_| format!("`{name}` needs a UTF-8 value"))
}

fn flag(name: &str, inline: Option<OsString>) -> Result<bool, String> {
    match inline {
        None => Ok(true),
        Some(_) => Err(format!("`{name}` takes no value")),
    }
}

/// `-` means stdin.
fn input_path(p: OsString) -> Option<PathBuf> {
    (p != "-").then(|| PathBuf::from(p))
}

fn parse_render(mut it: impl Iterator<Item = Tok>) -> Result<RenderArgs, String> {
    let mut r = RenderArgs::default();
    let mut inputs = 0;
    while let Some(tok) = it.next() {
        let (name, inline) = match tok {
            Tok::Pos(p) => {
                inputs += 1;
                r.input = input_path(p);
                continue;
            }
            Tok::Opt { name, inline } => (name, inline),
        };
        match name.as_str() {
            "-o" | "--output" => r.output = Some(value(&name, inline, &mut it)?.into()),
            "--width" => {
                let v = str_value(&name, inline, &mut it)?;
                match v.parse::<f64>() {
                    Ok(w) if w.is_finite() && w > 0.0 => r.width = Some(w),
                    _ => return Err(format!("`--width` needs a positive number, got `{v}`")),
                }
            }
            "--direction" => match str_value(&name, inline, &mut it)?.as_str() {
                "auto" => r.direction_auto = true,
                "source" => r.direction_auto = false,
                v => return Err(format!("`--direction` is `auto` or `source`, got `{v}`")),
            },
            "--edge-style" => {
                r.edge_style = Some(match str_value(&name, inline, &mut it)?.as_str() {
                    "orthogonal" => EdgeStyle::Orthogonal,
                    "polyline" => EdgeStyle::Polyline,
                    "spline" => EdgeStyle::Spline,
                    v => return Err(format!("unknown edge style `{v}`")),
                })
            }
            "--font" => {
                r.font = Some(match str_value(&name, inline, &mut it)?.as_str() {
                    "link" => FontMode::Link,
                    "embed" => FontMode::Embed,
                    "system" => FontMode::System,
                    v => return Err(format!("unknown font mode `{v}`")),
                })
            }
            "--hint" => r.hint = Some(value(&name, inline, &mut it)?.into()),
            "--no-hint" => r.no_hint = flag(&name, inline)?,
            "--strict" => r.strict = flag(&name, inline)?,
            "--outline" => r.outline = Some(value(&name, inline, &mut it)?.into()),
            "--id-prefix" => r.id_prefix = Some(str_value(&name, inline, &mut it)?),
            "--fuel" => {
                let v = str_value(&name, inline, &mut it)?;
                r.fuel = Some(
                    v.parse::<u64>()
                        .map_err(|_| format!("`--fuel` needs a whole number, got `{v}`"))?,
                );
            }
            "--follow-symlinks" => r.follow_symlinks = flag(&name, inline)?,
            "--json" => r.json = flag(&name, inline)?,
            "--batch" => r.batch = Some(value(&name, inline, &mut it)?.into()),
            "--json-summary" => r.json_summary = flag(&name, inline)?,
            "--css" => r.css = Some(value(&name, inline, &mut it)?.into()),
            "--theme" => r.theme = Some(str_value(&name, inline, &mut it)?),
            "--auto-dark" => r.auto_dark = Some(str_value(&name, inline, &mut it)?),
            "--no-auto-tone" => r.no_auto_tone = flag(&name, inline)?,
            _ => return Err(format!("unknown option `{name}` for `render`")),
        }
    }
    if inputs > 1 {
        return Err("`render` takes at most one input".into());
    }
    if r.css.is_none() && (r.theme.is_some() || r.auto_dark.is_some()) {
        return Err("`--theme` and `--auto-dark` need `--css`".into());
    }
    if r.hint.is_some() && r.no_hint {
        return Err("`--hint` and `--no-hint` conflict".into());
    }
    if r.batch.is_some() {
        if inputs > 0 {
            return Err("`--batch` takes a directory instead of an input".into());
        }
        if r.json || r.outline.is_some() || r.hint.is_some() {
            return Err("`--batch` does not combine with `--json`, `--outline` or `--hint`".into());
        }
    } else if r.json_summary {
        return Err("`--json-summary` needs `--batch`".into());
    }
    Ok(r)
}

fn parse_check(it: impl Iterator<Item = Tok>) -> Result<CheckArgs, String> {
    let mut c = CheckArgs::default();
    for tok in it {
        match tok {
            Tok::Pos(p) => c.inputs.push(PathBuf::from(p)),
            Tok::Opt { name, inline } => match name.as_str() {
                "--strict" => c.strict = flag(&name, inline)?,
                "--fix" => c.fix = flag(&name, inline)?,
                "--follow-symlinks" => c.follow_symlinks = flag(&name, inline)?,
                _ => return Err(format!("unknown option `{name}` for `check`")),
            },
        }
    }
    Ok(c)
}

fn parse_css(mut it: impl Iterator<Item = Tok>) -> Result<CssArgs, String> {
    let mut c = CssArgs::default();
    let mut inputs = 0;
    while let Some(tok) = it.next() {
        match tok {
            Tok::Pos(p) => {
                inputs += 1;
                c.input = input_path(p);
            }
            Tok::Opt { name, inline } => match name.as_str() {
                "-o" | "--output" => c.output = Some(value(&name, inline, &mut it)?.into()),
                "--strict" => c.strict = flag(&name, inline)?,
                "--follow-symlinks" => c.follow_symlinks = flag(&name, inline)?,
                _ => return Err(format!("unknown option `{name}` for `css`")),
            },
        }
    }
    if inputs > 1 {
        return Err("`css` takes at most one input".into());
    }
    Ok(c)
}

fn parse_outline(it: impl Iterator<Item = Tok>) -> Result<Command, String> {
    let mut input = None;
    let mut inputs = 0;
    let mut follow_symlinks = false;
    for tok in it {
        match tok {
            Tok::Pos(p) => {
                inputs += 1;
                input = input_path(p);
            }
            Tok::Opt { name, inline } if name == "--follow-symlinks" => {
                follow_symlinks = flag(&name, inline)?;
            }
            Tok::Opt { name, .. } => return Err(format!("unknown option `{name}` for `outline`")),
        }
    }
    if inputs > 1 {
        return Err("`outline` takes at most one input".into());
    }
    Ok(Command::Outline {
        input,
        follow_symlinks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(OsString::from))
    }

    fn render(args: &[&str]) -> RenderArgs {
        match p(args) {
            Ok(Command::Render(r)) => r,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn global_flags() {
        assert_eq!(p(&["--version"]), Ok(Command::Version));
        assert_eq!(p(&["-V"]), Ok(Command::Version));
        assert_eq!(p(&["--help"]), Ok(Command::Help));
        assert_eq!(p(&["render", "-h"]), Ok(Command::Help));
        assert!(p(&[]).is_err());
        assert!(p(&["draw"]).is_err());
    }

    #[test]
    fn render_defaults_to_stdin_and_stdout() {
        assert_eq!(render(&["render"]), RenderArgs::default());
        assert_eq!(render(&["render", "-"]).input, None);
    }

    #[test]
    fn render_options() {
        let r = render(&[
            "render",
            "in.mmd",
            "-o",
            "out.svg",
            "--width=640",
            "--direction",
            "auto",
            "--edge-style",
            "spline",
            "--font",
            "embed",
            "--hint",
            "prev.svg",
            "--strict",
            "--outline",
            "o.txt",
            "--id-prefix",
            "d1",
            "--fuel",
            "1000",
            "--follow-symlinks",
            "--json",
            "--no-auto-tone",
        ]);
        assert_eq!(r.input, Some(PathBuf::from("in.mmd")));
        assert_eq!(r.output, Some(PathBuf::from("out.svg")));
        assert_eq!(r.width, Some(640.0));
        assert!(r.direction_auto);
        assert_eq!(r.edge_style, Some(EdgeStyle::Spline));
        assert_eq!(r.font, Some(FontMode::Embed));
        assert_eq!(r.hint, Some(PathBuf::from("prev.svg")));
        assert!(r.strict && r.follow_symlinks && r.json);
        assert_eq!(r.outline, Some(PathBuf::from("o.txt")));
        assert_eq!(r.id_prefix.as_deref(), Some("d1"));
        assert_eq!(r.fuel, Some(1000));
        assert!(r.no_auto_tone);
        assert!(!render(&["render"]).no_auto_tone);
    }

    #[test]
    fn render_rejects_bad_values_and_conflicts() {
        for bad in [
            &["render", "--width"][..],
            &["render", "--width", "0"],
            &["render", "--width", "NaN"],
            &["render", "--width", "-5"],
            &["render", "--direction", "up"],
            &["render", "--edge-style", "curvy"],
            &["render", "--font", "bold"],
            &["render", "--fuel", "-1"],
            &["render", "--bogus"],
            &["render", "a", "b"],
            &["render", "--hint", "x", "--no-hint"],
            &["render", "--batch", "d", "in.mmd"],
            &["render", "--batch", "d", "--json"],
            &["render", "--json-summary"],
            &["render", "--strict=yes"],
        ] {
            assert!(p(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn double_dash_ends_options() {
        assert_eq!(
            render(&["render", "--", "-o"]).input,
            Some(PathBuf::from("-o"))
        );
    }

    #[test]
    fn batch_mode() {
        let r = render(&["render", "--batch", "corpus", "-o", "out", "--json-summary"]);
        assert_eq!(r.batch, Some(PathBuf::from("corpus")));
        assert_eq!(r.output, Some(PathBuf::from("out")));
        assert!(r.json_summary);
    }

    #[test]
    fn check_and_outline() {
        assert_eq!(
            p(&["check", "a.mmd", "b.md", "--strict", "--fix"]),
            Ok(Command::Check(CheckArgs {
                inputs: vec!["a.mmd".into(), "b.md".into()],
                strict: true,
                fix: true,
                follow_symlinks: false,
            }))
        );
        assert_eq!(
            p(&["outline", "a.mmd"]),
            Ok(Command::Outline {
                input: Some("a.mmd".into()),
                follow_symlinks: false,
            })
        );
        assert_eq!(
            p(&["outline", "--follow-symlinks"]),
            Ok(Command::Outline {
                input: None,
                follow_symlinks: true,
            })
        );
        assert!(p(&["outline", "a", "b"]).is_err());
        assert!(p(&["check", "--width", "3"]).is_err());
    }

    #[test]
    fn css_command() {
        assert_eq!(
            p(&[
                "css",
                "s.css",
                "-o",
                "out.css",
                "--strict",
                "--follow-symlinks"
            ]),
            Ok(Command::Css(CssArgs {
                input: Some("s.css".into()),
                output: Some("out.css".into()),
                strict: true,
                follow_symlinks: true,
            }))
        );
        assert_eq!(p(&["css"]), Ok(Command::Css(CssArgs::default())));
        assert_eq!(p(&["css", "-"]), Ok(Command::Css(CssArgs::default())));
        assert!(p(&["css", "a", "b"]).is_err());
        assert!(p(&["css", "--json"]).is_err());
    }

    #[test]
    fn render_stylesheet_options() {
        let r = render(&[
            "render",
            "--css",
            "s.css",
            "--theme",
            "dark",
            "--auto-dark=night",
        ]);
        assert_eq!(r.css, Some(PathBuf::from("s.css")));
        assert_eq!(r.theme.as_deref(), Some("dark"));
        assert_eq!(r.auto_dark.as_deref(), Some("night"));
        assert!(p(&["render", "--theme", "dark"]).is_err());
        assert!(p(&["render", "--auto-dark", "dark"]).is_err());
        assert!(p(&["render", "--css"]).is_err());
    }
}
