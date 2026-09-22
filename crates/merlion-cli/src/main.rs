//! `merlion` CLI (specs/integrations.md#cli): argument parsing, file and stdin I/O, and
//! layout hints read from files. The core it drives is pure; this binary owns every side
//! effect, including the wall-clock timing of `--batch`.

mod args;
mod fix;
mod fsio;
mod markdown;

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use args::{CheckArgs, Command, CssArgs, RenderArgs};
use markdown::Block;
use merlion_render::diag::{printable, Fix};
use merlion_render::stylesheet::{self, Palette, Stylesheet, StylesheetLimits};
use merlion_render::{
    error_diagnostic, json, Diagnostic, DirectionOption, RenderError, RenderOptions, RenderResult,
    Severity, Span,
};

/// A Markdown file may hold many diagrams, each under the core's 1 MiB limit.
const MAX_MARKDOWN_BYTES: u64 = 16 << 20;

fn main() -> ExitCode {
    let code = match args::parse(std::env::args_os().skip(1)) {
        Ok(cmd) => run(cmd),
        Err(msg) => usage_error(&msg),
    };
    ExitCode::from(code)
}

fn usage_error(msg: &str) -> u8 {
    eprintln!("merlion: {msg}\n\n{}", args::USAGE);
    2
}

/// Aggregated outcome across every diagram of one invocation (exit codes per
/// specs/integrations.md#cli).
#[derive(Default)]
struct Status {
    failed: bool,
    too_large: bool,
}

impl Status {
    fn code(&self) -> u8 {
        if self.failed {
            1
        } else if self.too_large {
            3
        } else {
            0
        }
    }

    fn record(&mut self, error: Option<&RenderError>) {
        match error {
            None => {}
            Some(RenderError::TooLarge { .. }) => self.too_large = true,
            Some(_) => self.failed = true,
        }
    }
}

fn run(cmd: Command) -> u8 {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("merlion: cannot read the working directory: {e}");
            return 1;
        }
    };
    let mut status = Status::default();
    match cmd {
        Command::Help => {
            print!("{}", args::USAGE);
            return 0;
        }
        Command::Version => {
            println!("merlion {}", merlion_render::VERSION);
            return 0;
        }
        Command::Render(r) => {
            let palette = match load_palette(&r, &cwd, &mut status) {
                Ok(p) => p,
                Err(PaletteError::Usage(msg)) => return usage_error(&msg),
                Err(PaletteError::Reported) => return status.code(),
            };
            let base = options(&r, palette);
            if let Some(dir) = &r.batch {
                render_batch(&r, &base, dir, &cwd, &mut status);
            } else if r.input.as_deref().is_some_and(markdown::is_markdown_path) {
                if r.outline.is_some() || r.hint.is_some() {
                    return usage_error(
                        "`--outline` and `--hint` apply to one diagram, not to a Markdown file",
                    );
                }
                render_markdown(&r, &base, &cwd, &mut status);
            } else {
                render_single(&r, &base, &cwd, &mut status);
            }
        }
        Command::Check(c) => check(&c, &cwd, &mut status),
        Command::Css(c) => css(&c, &cwd, &mut status),
        Command::Outline {
            input,
            follow_symlinks,
        } => {
            let guard = ReadGuard {
                cwd: &cwd,
                follow: follow_symlinks,
            };
            outline(input.as_deref(), guard, &mut status)
        }
    }
    status.code()
}

// ---------------------------------------------------------------------------------------
// Input

enum Input {
    Text(String),
    TooLarge,
}

fn display(path: Option<&Path>) -> String {
    path.map_or_else(|| "<stdin>".to_string(), |p| p.display().to_string())
}

/// Where inputs may be read from: paths are checked like hint paths
/// (specs/integrations.md#file-handling).
#[derive(Clone, Copy)]
struct ReadGuard<'a> {
    cwd: &'a Path,
    follow: bool,
}

/// Reads a file (or stdin) up to `cap` bytes. `Err` is already reported.
fn read_input(path: Option<&Path>, cap: u64, guard: ReadGuard) -> Result<Input, ()> {
    let name = display(path);
    let resolved = match path {
        Some(p) => Some(fsio::guard_read(p, guard.cwd, guard.follow).map_err(|msg| {
            eprintln!("merlion: {name}: refusing to read: {msg}");
        })?),
        None => None,
    };
    let mut buf = Vec::new();
    let res = match &resolved {
        Some(p) => std::fs::File::open(p).and_then(|f| f.take(cap + 1).read_to_end(&mut buf)),
        None => io::stdin().lock().take(cap + 1).read_to_end(&mut buf),
    };
    if let Err(e) = res {
        eprintln!("merlion: {name}: cannot read: {e}");
        return Err(());
    }
    if buf.len() as u64 > cap {
        return Ok(Input::TooLarge);
    }
    String::from_utf8(buf).map(Input::Text).map_err(|_| {
        eprintln!("merlion: {name}: input is not UTF-8");
    })
}

/// Reads an input, reporting an oversized one as `E004`. `None` means stop (status set).
fn read_or_record(
    path: Option<&Path>,
    cap: u64,
    guard: ReadGuard,
    status: &mut Status,
) -> Option<String> {
    match read_input(path, cap, guard) {
        Ok(Input::Text(t)) => Some(t),
        Ok(Input::TooLarge) => {
            print_diagnostics(&display(path), &[error_diagnostic(&too_large())]);
            status.too_large = true;
            None
        }
        Err(()) => {
            status.failed = true;
            None
        }
    }
}

fn input_cap(path: Option<&Path>) -> u64 {
    if path.is_some_and(markdown::is_markdown_path) {
        MAX_MARKDOWN_BYTES
    } else {
        RenderOptions::default().limits.input_bytes as u64
    }
}

// ---------------------------------------------------------------------------------------
// Diagnostics

fn too_large() -> RenderError {
    RenderError::TooLarge { what: "input" }
}

/// Rewrites a block-relative diagnostic into file positions.
fn map_diagnostic(d: &Diagnostic, block: &Block) -> Diagnostic {
    let map_span = |s: Span| {
        let (line, column) = block.map_position(s.line, s.column);
        let byte = |b: u32| u32::try_from(block.map_byte(b as usize)).unwrap_or(u32::MAX);
        Span {
            line,
            column,
            byte_start: byte(s.byte_start),
            byte_end: u32::try_from(block.map_byte_end(s.byte_end as usize)).unwrap_or(u32::MAX),
        }
    };
    Diagnostic {
        severity: d.severity,
        code: d.code,
        span: map_span(d.span),
        message: d.message.clone(),
        fix: d.fix.as_ref().map(|f| Fix {
            span: map_span(f.span),
            replacement: f.replacement.clone(),
        }),
    }
}

/// `file:line:col: severity code message` (specs/integrations.md#cli). A diagnostic with
/// no location points at 1:1. The file name and message are printed with control
/// characters escaped, so neither can drive the terminal.
fn print_diagnostics(file: &str, ds: &[Diagnostic]) {
    let mut err = io::stderr().lock();
    let file = printable(file);
    for d in ds {
        let _ = writeln!(
            err,
            "{file}:{}:{}: {} {} {}",
            d.span.line.max(1),
            d.span.column.max(1),
            d.severity.as_str(),
            d.code,
            printable(&d.message)
        );
    }
}

fn record_diagnostics(status: &mut Status, ds: &[Diagnostic]) {
    for d in ds.iter().filter(|d| d.severity == Severity::Error) {
        if d.code == "E004" {
            status.too_large = true;
        } else {
            status.failed = true;
        }
    }
}

// ---------------------------------------------------------------------------------------
// Output

fn guard_or_report(path: &Path, cwd: &Path, follow: bool) -> Option<PathBuf> {
    match fsio::guard_target(path, cwd, follow) {
        Ok(p) => Some(p),
        Err(msg) => {
            eprintln!("merlion: {}: refusing to write: {msg}", path.display());
            None
        }
    }
}

fn write_or_report(target: &Path, bytes: &[u8]) -> bool {
    match fsio::atomic_write(target, bytes) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("merlion: {}: cannot write: {e}", target.display());
            false
        }
    }
}

fn write_stdout(bytes: &[u8]) -> bool {
    let mut out = io::stdout().lock();
    out.write_all(bytes).and_then(|()| out.flush()).is_ok()
}

// ---------------------------------------------------------------------------------------
// Render

/// The render options of every diagram of the invocation; `palette` comes from `--css`.
fn options(r: &RenderArgs, palette: Option<Palette>) -> RenderOptions {
    let mut o = RenderOptions::default();
    if let Some(w) = r.width {
        o.target_width = w;
    }
    if r.direction_auto {
        o.direction = DirectionOption::Auto;
    }
    if let Some(e) = r.edge_style {
        o.edge_style = e;
    }
    if let Some(f) = r.font {
        o.font = f;
    }
    if let Some(f) = r.fuel {
        o.fuel = f;
    }
    o.strict = r.strict;
    o.id_prefix = r.id_prefix.clone();
    o.palette = palette;
    o.auto_tone = !r.no_auto_tone;
    o
}

/// Loads a layout hint. `Err` means the hint path was refused (reported; a failure). An
/// unreadable, oversized or non-UTF-8 hint is ignored with `I022` (specs/security.md).
fn load_hint(path: &Path, cwd: &Path, follow: bool) -> Result<Option<String>, ()> {
    let name = path.display().to_string();
    let resolved = fsio::guard_read(path, cwd, follow).map_err(|msg| {
        eprintln!("merlion: {name}: refusing to read hint: {msg}");
    })?;
    let ignored = |why: String| {
        print_diagnostics(
            &name,
            &[Diagnostic {
                severity: Severity::Info,
                code: "I022",
                span: Span::default(),
                message: format!("layout hint ignored: {why}"),
                fix: None,
            }],
        );
        Ok(None)
    };
    match fsio::read_hint(&resolved) {
        Ok(fsio::Hint::Text(t)) => Ok(Some(t)),
        Ok(fsio::Hint::TooLarge) => ignored("larger than 1 MiB".into()),
        Ok(fsio::Hint::NotUtf8) => ignored("not UTF-8".into()),
        Err(e) => ignored(e.to_string()),
    }
}

/// `--hint` defaults to the existing output file; `--no-hint` forces a fresh layout.
fn default_hint(target: &Path, r: &RenderArgs, cwd: &Path) -> Result<Option<String>, ()> {
    if r.no_hint || !target.is_file() {
        return Ok(None);
    }
    load_hint(target, cwd, r.follow_symlinks)
}

fn render_single(r: &RenderArgs, base: &RenderOptions, cwd: &Path, status: &mut Status) {
    let name = display(r.input.as_deref());
    // Every path is checked before any work, so a refused path never costs a render.
    let guarded = |p: &Option<PathBuf>| match p {
        Some(o) => guard_or_report(o, cwd, r.follow_symlinks).map(Some),
        None => Some(None),
    };
    let (Some(target), Some(outline_target)) = (guarded(&r.output), guarded(&r.outline)) else {
        status.failed = true;
        return;
    };
    let mut opts = base.clone();
    let hint = match (&r.hint, &target) {
        (Some(h), _) => load_hint(h, cwd, r.follow_symlinks),
        (None, Some(t)) => default_hint(t, r, cwd),
        (None, None) => Ok(None),
    };
    match hint {
        Ok(h) => opts.hint = h,
        Err(()) => return status.failed = true,
    }
    let guard = ReadGuard {
        cwd,
        follow: r.follow_symlinks,
    };
    let result = match read_input(r.input.as_deref(), opts.limits.input_bytes as u64, guard) {
        Ok(Input::Text(src)) => merlion_render::render(&src, &opts),
        Ok(Input::TooLarge) => RenderResult::from_error(too_large()),
        Err(()) => return status.failed = true,
    };
    print_diagnostics(&name, &result.diagnostics);
    status.record(result.error.as_ref());
    if r.json {
        status.failed |= !write_stdout(format!("{}\n", json::render_result(&result)).as_bytes());
    }
    if let Some(svg) = &result.svg {
        status.failed |= !match &target {
            Some(t) => write_or_report(t, svg.as_bytes()),
            None if r.json => true,
            None => write_stdout(svg.as_bytes()),
        };
    }
    if let (Some(t), Some(text)) = (&outline_target, &result.outline) {
        status.failed |= !write_or_report(t, text.as_bytes());
    }
}

/// Block `n` (1-based) of `<name>.md` renders to `<dir>/<name>-<n>.svg`; `<dir>` is `-o`
/// or the Markdown file's own directory.
fn render_markdown(r: &RenderArgs, base: &RenderOptions, cwd: &Path, status: &mut Status) {
    let Some(input) = r.input.as_deref() else {
        return;
    };
    let name = input.display().to_string();
    let guard = ReadGuard {
        cwd,
        follow: r.follow_symlinks,
    };
    let Some(text) = read_or_record(Some(input), MAX_MARKDOWN_BYTES, guard, status) else {
        return;
    };
    let stem = input
        .file_stem()
        .map_or_else(|| "diagram".into(), |s| s.to_string_lossy().into_owned());
    let dir = match (&r.output, input.parent()) {
        (Some(o), _) => o.clone(),
        (None, Some(p)) if !p.as_os_str().is_empty() => p.to_path_buf(),
        (None, _) => PathBuf::from("."),
    };
    for (i, block) in markdown::mermaid_blocks(&text).iter().enumerate() {
        let n = i + 1;
        let path = dir.join(format!("{stem}-{n}.svg"));
        let Some(target) = guard_or_report(&path, cwd, r.follow_symlinks) else {
            status.failed = true;
            continue;
        };
        let mut opts = base.clone();
        match default_hint(&target, r, cwd) {
            Ok(h) => opts.hint = h,
            Err(()) => {
                status.failed = true;
                continue;
            }
        }
        let mut result = merlion_render::render(&block.source, &opts);
        result.diagnostics = result
            .diagnostics
            .iter()
            .map(|d| map_diagnostic(d, block))
            .collect();
        print_diagnostics(&name, &result.diagnostics);
        status.record(result.error.as_ref());
        if r.json {
            // One JSON object per line: the block's number and output path, then the
            // fields of `render --json`.
            let mut line = format!(r#"{{"block":{n},"output":"#);
            json::push_str(&mut line, &path.display().to_string());
            line.push(',');
            line.push_str(json::render_result(&result).get(1..).unwrap_or("}"));
            line.push('\n');
            status.failed |= !write_stdout(line.as_bytes());
        }
        // A block that fails leaves its previous output untouched.
        if let Some(svg) = &result.svg {
            status.failed |= !write_or_report(&target, svg.as_bytes());
        }
    }
}

/// `render --batch <dir>`: every `.mmd` file of a directory, in name order, in one
/// process. With `--json-summary` it prints one JSON line per file with the render time
/// in microseconds, measured here because the core never reads a clock.
fn render_batch(r: &RenderArgs, base: &RenderOptions, dir: &Path, cwd: &Path, status: &mut Status) {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .filter(|e| {
                // A symbolic link is followed only with --follow-symlinks; the read
                // checks where it leads.
                let link = e.file_type().is_ok_and(|t| t.is_symlink());
                (!link || r.follow_symlinks) && e.path().extension().is_some_and(|x| x == "mmd")
            })
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect(),
        Err(e) => {
            eprintln!("merlion: {}: cannot read directory: {e}", dir.display());
            return status.failed = true;
        }
    };
    files.sort();
    for file in files {
        let name = file.display().to_string();
        let target = match &r.output {
            Some(out) => {
                let mut svg_name = file.file_stem().unwrap_or_default().to_os_string();
                svg_name.push(".svg");
                match guard_or_report(&out.join(svg_name), cwd, r.follow_symlinks) {
                    Some(t) => Some(t),
                    None => {
                        status.failed = true;
                        continue;
                    }
                }
            }
            None => None,
        };
        let mut opts = base.clone();
        if let Some(t) = &target {
            match default_hint(t, r, cwd) {
                Ok(h) => opts.hint = h,
                Err(()) => {
                    status.failed = true;
                    continue;
                }
            }
        }
        let guard = ReadGuard {
            cwd,
            follow: r.follow_symlinks,
        };
        let (result, micros) = match read_input(Some(&file), opts.limits.input_bytes as u64, guard)
        {
            Ok(Input::Text(src)) => {
                let start = Instant::now();
                let res = merlion_render::render(&src, &opts);
                (res, start.elapsed().as_micros())
            }
            Ok(Input::TooLarge) => (RenderResult::from_error(too_large()), 0),
            Err(()) => {
                status.failed = true;
                continue;
            }
        };
        let diags = &result.diagnostics;
        print_diagnostics(&name, diags);
        status.record(result.error.as_ref());
        if let (Some(t), Some(svg)) = (&target, &result.svg) {
            status.failed |= !write_or_report(t, svg.as_bytes());
        }
        if r.json_summary {
            let mut line = String::from(r#"{"file":"#);
            json::push_str(&mut line, &name);
            line.push_str(&format!(
                r#","ok":{},"micros":{micros},"fuel_used":{},"svg_bytes":{},"diagnostics":"#,
                result.svg.is_some(),
                result.fuel_used,
                result.svg.as_ref().map_or(0, String::len),
            ));
            json::push_diagnostics(&mut line, diags);
            line.push_str(r#","error":"#);
            json::push_error(&mut line, result.error.as_ref());
            line.push_str("}\n");
            status.failed |= !write_stdout(line.as_bytes());
        }
    }
}

// ---------------------------------------------------------------------------------------
// Stylesheets (specs/svg-output.md#stylesheet)

fn stylesheet_too_large() -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "E013",
        span: Span::default(),
        message: "stylesheet exceeds its limit on size".into(),
        fix: None,
    }
}

/// Reads and compiles a stylesheet under the file-handling rules: the path is guarded
/// like every input, and a file over 64 KiB is refused from its metadata before any byte
/// is read. Diagnostics are printed under the stylesheet's name; with `strict`, `W017`–
/// `W019` become errors. `None` means stop (reported, status set): `E013` sets exit 3,
/// anything else exit 1.
fn load_stylesheet(
    path: Option<&Path>,
    guard: ReadGuard,
    strict: bool,
    status: &mut Status,
) -> Option<Stylesheet> {
    let name = display(path);
    let limits = StylesheetLimits::default();
    let cap = limits.bytes as u64;
    if let Some(p) = path {
        let resolved = match fsio::guard_read(p, guard.cwd, guard.follow) {
            Ok(r) => r,
            Err(msg) => {
                eprintln!("merlion: {name}: refusing to read: {msg}");
                status.failed = true;
                return None;
            }
        };
        if std::fs::metadata(&resolved).is_ok_and(|m| m.len() > cap) {
            print_diagnostics(&name, &[stylesheet_too_large()]);
            status.too_large = true;
            return None;
        }
    }
    let text = match read_input(path, cap, guard) {
        Ok(Input::Text(t)) => t,
        Ok(Input::TooLarge) => {
            print_diagnostics(&name, &[stylesheet_too_large()]);
            status.too_large = true;
            return None;
        }
        Err(()) => {
            status.failed = true;
            return None;
        }
    };
    let (sheet, diags) = stylesheet::compile(&text, &limits);
    let mut items = diags.items;
    if strict {
        for d in &mut items {
            if matches!(d.code, "W017" | "W018" | "W019") {
                d.severity = Severity::Error;
            }
        }
    }
    print_diagnostics(&name, &items);
    for d in items.iter().filter(|d| d.severity == Severity::Error) {
        if d.code == "E013" {
            status.too_large = true;
        } else {
            status.failed = true;
        }
    }
    if status.failed || status.too_large {
        return None;
    }
    sheet
}

enum PaletteError {
    /// An undefined theme name: a usage error (exit 2).
    Usage(String),
    /// Reported already; the status carries the exit code.
    Reported,
}

/// `render --css`: compiles the stylesheet once for every diagram of the invocation and
/// resolves the palette of `--theme` (`:root` alone by default) and `--auto-dark`.
fn load_palette(
    r: &RenderArgs,
    cwd: &Path,
    status: &mut Status,
) -> Result<Option<Palette>, PaletteError> {
    let Some(path) = &r.css else {
        return Ok(None);
    };
    let guard = ReadGuard {
        cwd,
        follow: r.follow_symlinks,
    };
    let sheet =
        load_stylesheet(Some(path), guard, r.strict, status).ok_or(PaletteError::Reported)?;
    for t in [&r.theme, &r.auto_dark].into_iter().flatten() {
        if !sheet.theme_names().contains(&t.as_str()) {
            let mut known = sheet.theme_names().join("`, `");
            if known.is_empty() {
                known = "none".into();
            } else {
                known = format!("`{known}`");
            }
            return Err(PaletteError::Usage(format!(
                "{}: the stylesheet defines no theme `{}` (themes: {known})",
                path.display(),
                printable(t)
            )));
        }
    }
    sheet
        .palette(r.theme.as_deref(), r.auto_dark.as_deref())
        .map(Some)
        .ok_or_else(|| PaletteError::Usage("the stylesheet defines no such theme".into()))
}

/// `merlion css`: compiles a stylesheet into page CSS on stdout or `-o`.
fn css(c: &CssArgs, cwd: &Path, status: &mut Status) {
    let target = match &c.output {
        Some(o) => match guard_or_report(o, cwd, c.follow_symlinks) {
            Some(t) => Some(t),
            None => return status.failed = true,
        },
        None => None,
    };
    let guard = ReadGuard {
        cwd,
        follow: c.follow_symlinks,
    };
    let Some(sheet) = load_stylesheet(c.input.as_deref(), guard, c.strict, status) else {
        return;
    };
    let out = sheet.to_css();
    status.failed |= !match &target {
        Some(t) => write_or_report(t, out.as_bytes()),
        None => write_stdout(out.as_bytes()),
    };
}

// ---------------------------------------------------------------------------------------
// Check and outline

fn check(c: &CheckArgs, cwd: &Path, status: &mut Status) {
    let inputs: Vec<Option<&Path>> = if c.inputs.is_empty() {
        vec![None]
    } else {
        c.inputs.iter().map(|p| Some(p.as_path())).collect()
    };
    for input in inputs {
        let guard = ReadGuard {
            cwd,
            follow: c.follow_symlinks,
        };
        let Some(text) = read_or_record(input, input_cap(input), guard, status) else {
            continue;
        };
        let diags: Vec<Diagnostic> = if input.is_some_and(markdown::is_markdown_path) {
            markdown::mermaid_blocks(&text)
                .iter()
                .flat_map(|b| {
                    merlion_render::check(&b.source, c.strict)
                        .iter()
                        .map(|d| map_diagnostic(d, b))
                        .collect::<Vec<_>>()
                })
                .collect()
        } else {
            merlion_render::check(&text, c.strict)
        };
        print_diagnostics(&display(input), &diags);
        record_diagnostics(status, &diags);
        if c.fix {
            status.failed |= !apply_fixes(input, &text, &diags, cwd, c.follow_symlinks);
        }
    }
}

/// `check --fix`: applies every fix and writes the file atomically; fixed stdin input goes
/// to stdout. Returns false when the write fails or is refused.
fn apply_fixes(
    input: Option<&Path>,
    text: &str,
    diags: &[Diagnostic],
    cwd: &Path,
    follow: bool,
) -> bool {
    let edits: Vec<fix::Edit> = diags
        .iter()
        .filter_map(|d| d.fix.as_ref())
        .map(|f| fix::Edit {
            start: f.span.byte_start as usize,
            end: f.span.byte_end as usize,
            replacement: f.replacement.clone(),
        })
        .collect();
    let (fixed, applied) = fix::apply(text, &edits);
    match input {
        None => write_stdout(fixed.as_bytes()),
        Some(_) if applied == 0 => true,
        Some(path) => {
            let Some(target) = guard_or_report(path, cwd, follow) else {
                return false;
            };
            let ok = write_or_report(&target, fixed.as_bytes());
            if ok {
                eprintln!("merlion: {}: applied {applied} fix(es)", path.display());
            }
            ok
        }
    }
}

/// Prints the plain-text outline of every diagram (specs/svg-output.md#text-alternative).
fn outline(input: Option<&Path>, guard: ReadGuard, status: &mut Status) {
    let name = display(input);
    let Some(text) = read_or_record(input, input_cap(input), guard, status) else {
        return;
    };
    let sources: Vec<(String, Option<Block>)> = if input.is_some_and(markdown::is_markdown_path) {
        markdown::mermaid_blocks(&text)
            .into_iter()
            .map(|b| (b.source.clone(), Some(b)))
            .collect()
    } else {
        vec![(text, None)]
    };
    let mut outlines = Vec::new();
    for (src, block) in &sources {
        match merlion_render::outline(src) {
            Ok(o) => outlines.push(o),
            Err(e) => {
                // `check` explains every failure `outline` can hit with the same codes.
                let mut ds = merlion_render::check(src, false);
                if !ds.iter().any(|d| d.severity == Severity::Error) {
                    ds.push(error_diagnostic(&e));
                }
                if let Some(b) = block {
                    ds = ds.iter().map(|d| map_diagnostic(d, b)).collect();
                }
                print_diagnostics(&name, &ds);
                status.record(Some(&e));
            }
        }
    }
    let mut out = outlines.join("\n");
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    status.failed |= !write_stdout(out.as_bytes());
}
