//! Flowchart parser (specs/parser.md), following the syntax mermaid 12 documents for
//! flowcharts: statements separated by newlines or `;`, node shapes, links with
//! labels, `&` groups, chains, subgraphs, `classDef` / `class` / `:::`, `style`,
//! `linkStyle`, `click`, `accTitle` / `accDescr`.
//!
//! The parser is statement-oriented and never recurses: subgraph nesting is an
//! explicit stack bounded by `Limits::nesting` (`E010`). Every scan is bounded by the
//! end of the current line, the label cap or the end of input, and consumes what it
//! scans, so parsing stays linear in practice even on a single 1 MiB line.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::cursor::LineIndex;
use super::label::{self, QuoteScan};
use super::{link, style, ParseOptions, Stop};
use crate::diag::{Diagnostic, Diagnostics, Fix, Severity, Span};
use crate::model::{
    Arrow, ClassDef, Edge, Flowchart, Link, Meta, Node, Shape, Stroke, Style, Subgraph,
};
use crate::options::Direction;

/// Words that break Mermaid's grammar when used as a node id (`R004`).
const RESERVED: [&str; 3] = ["end", "graph", "subgraph"];

/// Node shapes by opener, longest opener first. Each opener lists its closers; when no
/// closer is found the next (shorter) opener that also matches is tried, so
/// `A[/path/to]` falls back to a rectangle labelled `/path/to`.
const OPENERS: &[(&str, &[(&str, Shape)])] = &[
    ("(((", &[(")))", Shape::DoubleCircle)]),
    ("((", &[("))", Shape::Circle)]),
    ("([", &[("])", Shape::Stadium)]),
    ("(", &[(")", Shape::Round)]),
    ("[[", &[("]]", Shape::Subroutine)]),
    ("[(", &[(")]", Shape::Cylinder)]),
    (
        "[/",
        &[("/]", Shape::Parallelogram), ("\\]", Shape::Trapezoid)],
    ),
    (
        "[\\",
        &[
            ("\\]", Shape::ParallelogramAlt),
            ("/]", Shape::TrapezoidAlt),
        ],
    ),
    ("[", &[("]", Shape::Rect)]),
    ("{{", &[("}}", Shape::Hexagon)]),
    ("{", &[("}", Shape::Rhombus)]),
    (">", &[("]", Shape::Asymmetric)]),
];

/// `@{ shape: … }` names (mermaid 12 "expanded node shapes") that map onto
/// [`Shape`]. Other names are drawn as rectangles with `W015`.
const AT_SHAPES: &[(&str, Shape)] = &[
    ("bolt", Shape::Bolt),
    ("circ", Shape::Circle),
    ("circle", Shape::Circle),
    ("collate", Shape::Hourglass),
    ("com-link", Shape::Bolt),
    ("cross-circ", Shape::CrossedCircle),
    ("crossed-circle", Shape::CrossedCircle),
    ("cyl", Shape::Cylinder),
    ("cylinder", Shape::Cylinder),
    ("database", Shape::Cylinder),
    ("db", Shape::Cylinder),
    ("dbl-circ", Shape::DoubleCircle),
    ("decision", Shape::Rhombus),
    ("diam", Shape::Rhombus),
    ("diamond", Shape::Rhombus),
    ("double-circle", Shape::DoubleCircle),
    ("event", Shape::Round),
    ("f-circ", Shape::FilledCircle),
    ("filled-circle", Shape::FilledCircle),
    ("fork", Shape::Fork),
    ("fr-circ", Shape::FramedCircle),
    ("fr-rect", Shape::Subroutine),
    ("framed-circle", Shape::FramedCircle),
    ("framed-rectangle", Shape::Subroutine),
    ("hex", Shape::Hexagon),
    ("hexagon", Shape::Hexagon),
    ("hourglass", Shape::Hourglass),
    ("in-out", Shape::Parallelogram),
    ("inv-trapezoid", Shape::TrapezoidAlt),
    ("join", Shape::Fork),
    ("junction", Shape::FilledCircle),
    ("lean-l", Shape::ParallelogramAlt),
    ("lean-left", Shape::ParallelogramAlt),
    ("lean-r", Shape::Parallelogram),
    ("lean-right", Shape::Parallelogram),
    ("lightning-bolt", Shape::Bolt),
    ("manual", Shape::TrapezoidAlt),
    ("odd", Shape::Asymmetric),
    ("out-in", Shape::ParallelogramAlt),
    ("pill", Shape::Stadium),
    ("prepare", Shape::Hexagon),
    ("priority", Shape::Trapezoid),
    ("proc", Shape::Rect),
    ("process", Shape::Rect),
    ("question", Shape::Rhombus),
    ("rect", Shape::Rect),
    ("rectangle", Shape::Rect),
    ("rounded", Shape::Round),
    ("sm-circ", Shape::SmallCircle),
    ("small-circle", Shape::SmallCircle),
    ("stadium", Shape::Stadium),
    ("start", Shape::SmallCircle),
    ("stop", Shape::FramedCircle),
    ("subproc", Shape::Subroutine),
    ("subprocess", Shape::Subroutine),
    ("subroutine", Shape::Subroutine),
    ("summary", Shape::CrossedCircle),
    ("terminal", Shape::Stadium),
    ("trap-b", Shape::Trapezoid),
    ("trap-t", Shape::TrapezoidAlt),
    ("trapezoid", Shape::Trapezoid),
    ("trapezoid-bottom", Shape::Trapezoid),
    ("trapezoid-top", Shape::TrapezoidAlt),
];

struct NodeB {
    id: String,
    label: String,
    shape: Shape,
    subgraph: Option<usize>,
    span: Span,
    /// Given a shape, `@{…}`, a standalone statement, or renamed by `R004`.
    declared: bool,
    /// Given a bracket shape, an `@{…}` shape or label, or a label by `R004`: a node
    /// even when its id names a subgraph.
    shaped: bool,
    /// End of the id at its first occurrence, where `R005` inserts a label.
    first_id: (usize, usize),
}

struct EdgeB {
    from: usize,
    to: usize,
    label: Option<String>,
    stroke: Stroke,
    arrow_start: Arrow,
    arrow_end: Arrow,
    min_len: u32,
    span: Span,
}

struct SubB {
    id: String,
    title: String,
    parent: Option<usize>,
    direction: Option<Direction>,
    span: Span,
}

/// Statements about nodes that apply once every node is known, in source order.
enum Op {
    Class {
        id: String,
        class: String,
    },
    Style {
        id: String,
        span: Span,
        style: Style,
    },
    Click {
        id: String,
        link: Link,
    },
}

enum LinkTarget {
    Default,
    Indices(Vec<(usize, Span)>),
}

struct LinkTok {
    stroke: Stroke,
    start: Arrow,
    end: Arrow,
    min_len: u32,
    label: Option<String>,
}

struct Group {
    nodes: Vec<usize>,
    start: usize,
    end: usize,
}

/// A node label read from inside a shape.
struct LabelOut {
    text: String,
    shape: Shape,
    /// Byte range of the raw label between opener and closer (for `R001` / `W012`).
    raw: (usize, usize),
    quoted: bool,
    typographic: Vec<(usize, usize)>,
    /// Just past the closer.
    end: usize,
}

struct P<'a, 'd> {
    src: &'a str,
    idx: &'a LineIndex<'a>,
    pos: usize,
    opts: &'a ParseOptions,
    diags: &'d mut Diagnostics,
    meta: Meta,
    direction: Direction,
    nodes: Vec<NodeB>,
    node_map: BTreeMap<String, usize>,
    edges: Vec<EdgeB>,
    edge_ids: BTreeSet<String>,
    subs: Vec<SubB>,
    sub_ids: BTreeSet<String>,
    stack: Vec<usize>,
    class_defs: Vec<ClassDef>,
    default_link_style: Style,
    link_styles: Vec<(Vec<(usize, Span)>, Style)>,
    ops: Vec<Op>,
    /// Every id-like word in the source; renamed and generated ids avoid them.
    words: BTreeSet<String>,
    renames: BTreeMap<String, String>,
}

/// Parses a flowchart whose header word ends at `pos`.
pub(crate) fn parse_flowchart(
    idx: &LineIndex,
    pos: usize,
    meta: Meta,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Result<Flowchart, Stop> {
    let src = idx.src();
    let mut p = P {
        src,
        idx,
        pos,
        opts,
        diags,
        meta,
        direction: Direction::TB,
        nodes: Vec::new(),
        node_map: BTreeMap::new(),
        edges: Vec::new(),
        edge_ids: BTreeSet::new(),
        subs: Vec::new(),
        sub_ids: BTreeSet::new(),
        stack: Vec::new(),
        class_defs: Vec::new(),
        default_link_style: Style::default(),
        link_styles: Vec::new(),
        ops: Vec::new(),
        words: collect_words(src),
        renames: BTreeMap::new(),
    };
    p.header_direction()?;
    p.statements()?;
    Ok(p.finish())
}

fn is_id_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Every maximal run of id characters (`[\p{Alnum}_]`), for collision-free renames.
fn collect_words(src: &str) -> BTreeSet<String> {
    let mut words = BTreeSet::new();
    let mut start = None;
    for (i, c) in src.char_indices() {
        match (is_id_char(c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                words.insert(src.get(s..i).unwrap_or("").to_string());
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        words.insert(src.get(s..).unwrap_or("").to_string());
    }
    words
}

fn parse_direction(tok: &str) -> Option<Direction> {
    Some(match tok.to_ascii_uppercase().as_str() {
        "TB" | "TD" | "V" => Direction::TB,
        "BT" | "^" => Direction::BT,
        "LR" | ">" => Direction::LR,
        "RL" | "<" => Direction::RL,
        _ => return None,
    })
}

fn valid_class_name(name: &str) -> bool {
    let b = name.as_bytes();
    match b.first() {
        Some(c) if c.is_ascii_alphabetic() || *c == b'_' => {}
        _ => return false,
    }
    b.len() <= 64
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
}

impl P<'_, '_> {
    // ------------------------------------------------------------ cursor helpers

    fn rest(&self) -> &str {
        self.src.get(self.pos..).unwrap_or("")
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn byte(&self, at: usize) -> Option<u8> {
        self.src.as_bytes().get(at).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    fn skip_hws(&mut self) -> bool {
        let start = self.pos;
        while matches!(self.byte(self.pos), Some(b' ' | b'\t' | b'\r')) {
            self.pos += 1;
        }
        self.pos != start
    }

    /// Offset of the end of the line containing `at` (its `\n` or the end of input).
    fn eol_from(&self, at: usize) -> usize {
        self.src
            .get(at..)
            .and_then(|r| r.find('\n'))
            .map_or(self.src.len(), |i| at + i)
    }

    /// At a statement boundary: end of input, newline, `;` or a `%%` comment.
    fn at_stmt_end(&self) -> bool {
        matches!(self.peek(), None | Some('\n' | ';')) || self.starts_with("%%")
    }

    /// End of the current statement's text: the next `;` or newline.
    fn stmt_text_end(&self) -> usize {
        self.rest()
            .find([';', '\n'])
            .map_or(self.src.len(), |i| self.pos + i)
    }

    fn scan_id(&self, from: usize) -> usize {
        let mut end = from;
        let mut chars = self.src.get(from..).unwrap_or("").char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let ok = if is_id_char(c) {
                true
            } else if (c == '-' || c == '.') && end > from {
                // `-` and `.` join id parts (`my-node`, `v1.2`) but never start a
                // link (`--`, `-.`, `->`).
                chars.peek().is_some_and(|&(_, n)| is_id_char(n))
            } else {
                false
            };
            if !ok {
                break;
            }
            end = from + i + c.len_utf8();
        }
        end
    }

    fn span(&self, start: usize, end: usize) -> Span {
        self.idx.span(start, end)
    }

    /// Records `E002` at `start..end` and stops.
    fn fail(&mut self, start: usize, end: usize, message: impl Into<String>) -> Stop {
        let span = self.span(start, end);
        self.diags.emit(Severity::Error, "E002", span, message);
        Stop::Failed
    }

    /// `E002` at the current position, naming what was found.
    fn fail_here(&mut self, expected: &str) -> Stop {
        let (found, len) = match self.peek() {
            None => (String::from("end of input"), 0),
            Some('\n') => (String::from("end of line"), 0),
            Some(c) => (alloc::format!("`{}`", c), c.len_utf8()),
        };
        let pos = self.pos;
        self.fail(
            pos,
            pos + len,
            alloc::format!("{}, found {}", expected, found),
        )
    }

    fn repair(&mut self, code: &'static str, span: Span, message: String, fix: Fix) {
        self.diags.push(Diagnostic {
            severity: Severity::Repair,
            code,
            span,
            message,
            fix: Some(fix),
        });
    }

    fn warn(&mut self, code: &'static str, start: usize, end: usize, message: String) {
        let span = self.span(start, end);
        self.diags.emit(Severity::Warning, code, span, message);
    }

    fn typographic(&mut self, ranges: &[(usize, usize)]) {
        for &(s, e) in ranges {
            super::typographic_quote(self.idx, s, e, self.diags);
        }
    }

    /// Cleans label text and applies the length limit (`W012`).
    fn finish_label(&mut self, raw: &str, start: usize, end: usize) -> String {
        let mut text = label::clean_label(raw);
        let max = self.opts.limits.label_bytes;
        if label::truncate(&mut text, max) {
            self.warn(
                "W012",
                start,
                end,
                alloc::format!("label longer than {} bytes; truncated", max),
            );
        }
        text
    }

    // ------------------------------------------------------------ statements

    fn header_direction(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        if !self.at_stmt_end() {
            let start = self.pos;
            while self.peek().is_some_and(|c| !c.is_whitespace() && c != ';') {
                self.pos += self.peek().map_or(1, char::len_utf8);
            }
            let tok = self.src.get(start..self.pos).unwrap_or("");
            match parse_direction(tok) {
                Some(d) => self.direction = d,
                None => {
                    let msg = alloc::format!(
                        "expected a direction (TB, TD, BT, LR or RL) after the header, found `{}`",
                        tok
                    );
                    return Err(self.fail(start, self.pos, msg));
                }
            }
        }
        self.end_statement("expected `;` or a newline after the header")
    }

    /// After a statement: optional comment, then a newline, `;` or the end of input.
    fn end_statement(&mut self, expected: &str) -> Result<(), Stop> {
        self.skip_hws();
        if self.starts_with("%%") && !self.starts_with("%%{") {
            self.pos = self.eol_from(self.pos);
        }
        if matches!(self.peek(), None | Some('\n' | ';')) {
            Ok(())
        } else {
            Err(self.fail_here(expected))
        }
    }

    fn statements(&mut self) -> Result<(), Stop> {
        loop {
            loop {
                while self.peek().is_some_and(char::is_whitespace) {
                    self.pos += self.peek().map_or(1, char::len_utf8);
                }
                if self.peek() == Some(';') {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos >= self.src.len() {
                break;
            }
            let fence_like = matches!(self.peek(), Some('`' | '~'))
                && super::cursor::at_line_start(self.src, self.pos)
                && super::repair::is_fence_line(
                    self.src
                        .get(self.pos..self.eol_from(self.pos))
                        .unwrap_or(""),
                );
            if fence_like {
                self.pos = super::strip_fence_line(self.idx, self.pos, self.diags);
                continue;
            }
            if self.starts_with("%%{") {
                self.pos =
                    super::directive(self.idx, self.pos, &mut self.meta, self.opts, self.diags);
                continue;
            }
            if self.starts_with("%%") {
                self.pos = self.eol_from(self.pos);
                continue;
            }
            self.statement()?;
            self.end_statement("expected a link, `&`, `;` or a newline")?;
        }
        // R002: close every open subgraph at the end of input.
        let insert_at = self.src.len();
        let text = if self.src.ends_with('\n') {
            "end\n"
        } else {
            "\nend"
        };
        while let Some(s) = self.stack.pop() {
            let (span, title) = match self.subs.get(s) {
                Some(sub) => (sub.span, sub.title.clone()),
                None => (Span::default(), String::new()),
            };
            let fix_span = self.span(insert_at, insert_at);
            self.repair(
                "R002",
                span,
                alloc::format!(
                    "subgraph `{}` has no matching `end`; closed at end of input",
                    title
                ),
                Fix {
                    span: fix_span,
                    replacement: String::from(text),
                },
            );
        }
        Ok(())
    }

    /// Whether the keyword ending at `word_end` applies: it is followed by whitespace,
    /// `;` or the end of the line, and not by a link (`end --> x` is an edge).
    fn keyword_applies(&self, word_end: usize) -> bool {
        match self.src.get(word_end..).and_then(|r| r.chars().next()) {
            None | Some('\n' | ';') => true,
            Some(' ' | '\t' | '\r') => {
                let mut i = word_end;
                while matches!(self.byte(i), Some(b' ' | b'\t' | b'\r')) {
                    i += 1;
                }
                !self.is_link_start(i)
            }
            _ => false,
        }
    }

    fn is_link_start(&self, at: usize) -> bool {
        let r = self.src.get(at..).unwrap_or("");
        let r = r
            .strip_prefix(['o', 'x'])
            .filter(|t| t.starts_with("--") || t.starts_with("==") || t.starts_with("-."))
            .unwrap_or(r);
        let r = r.strip_prefix('<').unwrap_or(r);
        r.starts_with("--") || r.starts_with("==") || r.starts_with("-.") || r.starts_with("~~~")
    }

    fn statement(&mut self) -> Result<(), Stop> {
        let start = self.pos;
        let word_end = self.scan_id(start);
        let word = self.src.get(start..word_end).unwrap_or("");
        let next_non_ws = {
            let mut i = word_end;
            while matches!(self.byte(i), Some(b' ' | b'\t' | b'\r')) {
                i += 1;
            }
            self.byte(i)
        };
        match word {
            "subgraph" if self.keyword_applies(word_end) => {
                self.pos = word_end;
                self.subgraph(start)
            }
            "end" if matches!(next_non_ws, None | Some(b'\n' | b';' | b'%')) => {
                self.pos = word_end;
                if self.stack.pop().is_none() {
                    return Err(self.fail(
                        start,
                        word_end,
                        "unexpected `end`: no subgraph is open",
                    ));
                }
                Ok(())
            }
            "direction" if self.keyword_applies(word_end) => {
                self.pos = word_end;
                self.direction_stmt()
            }
            "classDef" if self.keyword_applies(word_end) => {
                self.pos = word_end;
                self.class_def()
            }
            "class" if self.keyword_applies(word_end) => {
                self.pos = word_end;
                self.class_stmt()
            }
            "style" if self.keyword_applies(word_end) => {
                self.pos = word_end;
                self.style_stmt()
            }
            "linkStyle" if self.keyword_applies(word_end) => {
                self.pos = word_end;
                self.link_style()
            }
            "click" if self.keyword_applies(word_end) => {
                self.pos = word_end;
                self.click()
            }
            "accTitle" if next_non_ws == Some(b':') => {
                self.pos = word_end;
                self.acc_line(true)
            }
            "accDescr" if next_non_ws == Some(b':') => {
                self.pos = word_end;
                self.acc_line(false)
            }
            "accDescr" if next_non_ws == Some(b'{') => {
                self.pos = word_end;
                self.acc_block()
            }
            _ => self.chain(),
        }
    }

    // ------------------------------------------------------------ nodes and edges

    /// `group (link group)*`; a group alone declares its nodes.
    fn chain(&mut self) -> Result<(), Stop> {
        // `e1@{ animate: true }` configures an edge declared earlier with `e1@-->`.
        let id_end = self.scan_id(self.pos);
        if id_end > self.pos && self.src.get(id_end..).is_some_and(|r| r.starts_with("@{")) {
            let id = self.src.get(self.pos..id_end).unwrap_or("");
            if self.edge_ids.contains(id) {
                self.pos = id_end + 1;
                let _ = self.brace_block()?;
                return Ok(());
            }
        }
        let mut prev = self.group()?;
        let mut linked = false;
        loop {
            let save = self.pos;
            self.skip_hws();
            if self.at_stmt_end() {
                self.pos = save;
                break;
            }
            self.skip_edge_id();
            let Some(tok) = self.link()? else {
                return Err(self.fail_here("expected a link, `&`, `;` or a newline"));
            };
            self.skip_hws();
            let next = self.group()?;
            let span = self.span(prev.start, next.end);
            for &a in &prev.nodes {
                for &b in &next.nodes {
                    if self.edges.len() >= self.opts.limits.edges {
                        return Err(Stop::TooLarge("edges"));
                    }
                    self.edges.push(EdgeB {
                        from: a,
                        to: b,
                        label: tok.label.clone(),
                        stroke: tok.stroke,
                        arrow_start: tok.start,
                        arrow_end: tok.end,
                        min_len: tok.min_len,
                        span,
                    });
                }
            }
            linked = true;
            prev = next;
        }
        if !linked {
            for &n in &prev.nodes {
                if let Some(node) = self.nodes.get_mut(n) {
                    node.declared = true;
                }
            }
        }
        Ok(())
    }

    /// `vertex (& vertex)*`
    fn group(&mut self) -> Result<Group, Stop> {
        let start = self.pos;
        let mut nodes = alloc::vec![self.vertex()?];
        loop {
            let save = self.pos;
            self.skip_hws();
            if self.peek() == Some('&') {
                self.pos += 1;
                self.skip_hws();
                nodes.push(self.vertex()?);
            } else {
                self.pos = save;
                break;
            }
        }
        Ok(Group {
            nodes,
            start,
            end: self.pos,
        })
    }

    /// Skips an edge id (`e1@-->`); Merlion accepts edge ids and ignores them.
    fn skip_edge_id(&mut self) {
        let end = self.scan_id(self.pos);
        if end > self.pos && self.byte(end) == Some(b'@') && self.is_link_start(end + 1) {
            let id = self.src.get(self.pos..end).unwrap_or("").to_string();
            self.edge_ids.insert(id);
            self.pos = end + 1;
        }
    }

    /// The renamed id for a reserved word (`R004`): `end_`, `end__`, … whichever is
    /// unused anywhere in the source.
    fn renamed(&mut self, raw: &str) -> String {
        if let Some(r) = self.renames.get(raw) {
            return r.clone();
        }
        let mut name = alloc::format!("{}_", raw);
        while self.words.contains(&name) || self.node_map.contains_key(&name) {
            name.push('_');
        }
        self.renames.insert(raw.to_string(), name.clone());
        name
    }

    /// Resolves an id in a `class` / `style` / `click` statement, applying `R004`.
    fn reference_id(&mut self, start: usize, end: usize) -> String {
        let raw = self.src.get(start..end).unwrap_or("").to_string();
        if !RESERVED.contains(&raw.as_str()) {
            return raw;
        }
        let renamed = self.renamed(&raw);
        let span = self.span(start, end);
        self.repair(
            "R004",
            span,
            alloc::format!(
                "`{}` is a reserved word; node renamed to `{}`",
                raw,
                renamed
            ),
            Fix {
                span,
                replacement: renamed.clone(),
            },
        );
        renamed
    }

    fn vertex(&mut self) -> Result<usize, Stop> {
        let start = self.pos;
        let id_end = self.scan_id(start);
        if id_end == start {
            return Err(self.fail_here("expected a node id"));
        }
        let raw = self.src.get(start..id_end).unwrap_or("").to_string();
        self.pos = id_end;
        let reserved = RESERVED.contains(&raw.as_str());
        let id = if reserved {
            self.renamed(&raw)
        } else {
            raw.clone()
        };
        let existing = self.node_map.get(&id).copied();

        // Shape: `@{…}` or a bracket opener (optionally after spaces).
        let mut shape: Option<(Shape, Option<String>)> = None;
        let at_block = self.starts_with("@{");
        if at_block {
            shape = Some(self.at_shape()?);
        } else {
            let save = self.pos;
            let spaced = self.skip_hws();
            match self.peek() {
                Some('[' | '(' | '{') => shape = Some(self.shape()?),
                Some('>') if !spaced => shape = Some(self.shape()?),
                _ => self.pos = save,
            }
        }
        let shape_end = self.pos;

        // `:::class` shorthand, possibly repeated.
        let mut classes = Vec::new();
        while self.starts_with(":::") {
            self.pos += 3;
            let cs = self.pos;
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                self.pos += 1;
            }
            if self.pos == cs {
                return Err(self.fail_here("expected a class name after `:::`"));
            }
            let name = self.src.get(cs..self.pos).unwrap_or("").to_string();
            if valid_class_name(&name) {
                classes.push(name);
            } else {
                let p = self.pos;
                self.warn(
                    "W011",
                    cs,
                    p,
                    alloc::format!(
                        "class name `{}` is outside `[A-Za-z_][A-Za-z0-9_-]{{0,63}}`; dropped",
                        name
                    ),
                );
            }
        }

        let n = match existing {
            Some(n) => n,
            None => {
                if self.nodes.len() >= self.opts.limits.nodes {
                    return Err(Stop::TooLarge("nodes"));
                }
                let span = self.span(start, shape_end);
                self.nodes.push(NodeB {
                    id: id.clone(),
                    label: id.clone(),
                    shape: Shape::Rect,
                    subgraph: None,
                    span,
                    declared: false,
                    shaped: false,
                    first_id: (start, id_end),
                });
                self.node_map.insert(id.clone(), self.nodes.len() - 1);
                self.nodes.len() - 1
            }
        };
        if reserved {
            let add_label = existing.is_none() && shape.is_none();
            let replacement = if add_label {
                alloc::format!("{}[{}]", id, raw)
            } else {
                id.clone()
            };
            let span = self.span(start, id_end);
            self.repair(
                "R004",
                span,
                alloc::format!("`{}` is a reserved word; node renamed to `{}`", raw, id),
                Fix { span, replacement },
            );
            if add_label {
                if let Some(node) = self.nodes.get_mut(n) {
                    node.label = raw.clone();
                    node.declared = true;
                    node.shaped = true;
                }
            }
        }
        if let (Some((s, label)), Some(node)) = (shape, self.nodes.get_mut(n)) {
            node.shaped |= !at_block || label.is_some() || s != Shape::Rect;
            node.shape = s;
            if let Some(l) = label {
                node.label = l;
            }
            node.declared = true;
        }
        for class in classes {
            self.ops.push(Op::Class {
                id: id.clone(),
                class,
            });
        }
        self.assign_subgraph(n);
        Ok(n)
    }

    /// Mermaid's membership rule: a node mentioned inside a subgraph belongs to it,
    /// unless it already belongs to a subgraph that has closed; an inner subgraph
    /// takes a node from an enclosing one that is still open.
    fn assign_subgraph(&mut self, n: usize) {
        let Some(&top) = self.stack.last() else {
            return;
        };
        let stack = &self.stack;
        if let Some(node) = self.nodes.get_mut(n) {
            match node.subgraph {
                None => node.subgraph = Some(top),
                Some(t) if t != top && stack.contains(&t) => node.subgraph = Some(top),
                _ => {}
            }
        }
    }

    /// Parses a bracketed shape at the cursor; returns the shape and its label.
    fn shape(&mut self) -> Result<(Shape, Option<String>), Stop> {
        let open_at = self.pos;
        let mut last_closer = "]";
        for &(opener, closers) in OPENERS {
            if !self.starts_with(opener) {
                continue;
            }
            last_closer = closers.first().map_or("]", |c| c.0);
            if let Some(out) = self.label_in(open_at + opener.len(), closers)? {
                self.pos = out.end;
                self.typographic(&out.typographic);
                let (rs, re) = out.raw;
                let raw = self.src.get(rs..re).unwrap_or("").to_string();
                if !out.quoted && label::needs_quoting(&raw) {
                    let span = self.span(rs, re);
                    self.repair(
                        "R001",
                        span,
                        String::from("unquoted label contains `()`, `[]`, `{}` or `:`; quoted"),
                        Fix {
                            span,
                            replacement: label::quote_label(&raw),
                        },
                    );
                }
                let _ = &out.text;
                return Ok((out.shape, Some(out.text)));
            }
        }
        let msg = alloc::format!("expected `{}` to close the node label", last_closer);
        Err(self.fail(open_at, open_at + 1, msg))
    }

    /// Reads a label from `content_start` up to one of `closers`. `Ok(None)` means no
    /// closer was found (the caller tries a shorter opener); `Err` is an unterminated
    /// ASCII string.
    fn label_in(
        &mut self,
        content_start: usize,
        closers: &[(&str, Shape)],
    ) -> Result<Option<LabelOut>, Stop> {
        let closer_at = |p: &Self, at: usize| -> Option<(usize, Shape)> {
            let r = p.src.get(at..).unwrap_or("");
            closers
                .iter()
                .find(|(c, _)| r.starts_with(c))
                .map(|&(c, s)| (c.len(), s))
        };
        // Quoted label: `A["…"]`, possibly after spaces.
        let mut q = content_start;
        while matches!(self.byte(q), Some(b' ' | b'\t')) {
            q += 1;
        }
        match label::scan_quoted(self.src, q) {
            QuoteScan::NotQuote => {}
            QuoteScan::Unterminated { typographic: false } => {
                return Err(self.fail(q, q + 1, "unterminated string; expected a closing `\"`"));
            }
            QuoteScan::Unterminated { typographic: true } => {}
            QuoteScan::Found(qs) => {
                let mut after = qs.end;
                while matches!(self.byte(after), Some(b' ' | b'\t')) {
                    after += 1;
                }
                if let Some((len, shape)) = closer_at(self, after) {
                    let raw = self
                        .src
                        .get(qs.content_start..qs.content_end)
                        .unwrap_or("")
                        .to_string();
                    let text = self.finish_label(&raw, qs.content_start, qs.content_end);
                    return Ok(Some(LabelOut {
                        text,
                        shape,
                        raw: (qs.content_start, qs.content_end),
                        quoted: true,
                        typographic: qs.typographic,
                        end: after + len,
                    }));
                }
                if qs.typographic.is_empty() {
                    return Ok(None);
                }
                // A typographic quote not followed by the closer is plain text.
            }
        }
        // Unquoted: a bracket-depth-aware scan to the end of the line. Both passes stop
        // at a cap well above the label limit, so a node whose closer is missing costs
        // at most the cap, not the rest of a long line.
        let cap = self.opts.limits.label_bytes.saturating_mul(2).max(8_192);
        let limit = content_start.saturating_add(cap).min(self.src.len());
        let mut found: Option<(usize, usize, Shape)> = None;
        let mut depth = 0usize;
        let mut i = content_start;
        while i < limit {
            if depth == 0 {
                if let Some((len, shape)) = closer_at(self, i) {
                    found = Some((i, len, shape));
                    break;
                }
            }
            let c = self
                .src
                .get(i..)
                .and_then(|r| r.chars().next())
                .unwrap_or('\n');
            match c {
                '\n' => break,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            i += c.len_utf8();
        }
        if found.is_none() {
            // Unbalanced brackets: take the first closer on the line.
            let mut i = content_start;
            while i < limit {
                if let Some((len, shape)) = closer_at(self, i) {
                    found = Some((i, len, shape));
                    break;
                }
                let c = self
                    .src
                    .get(i..)
                    .and_then(|r| r.chars().next())
                    .unwrap_or('\n');
                if c == '\n' {
                    break;
                }
                i += c.len_utf8();
            }
        }
        let Some((close, len, shape)) = found else {
            return Ok(None);
        };
        let raw = self.src.get(content_start..close).unwrap_or("").to_string();
        let text = self.finish_label(&raw, content_start, close);
        Ok(Some(LabelOut {
            text,
            shape,
            raw: (content_start, close),
            quoted: false,
            typographic: Vec::new(),
            end: close + len,
        }))
    }

    /// Finds the `}` closing a `{` at the cursor (quote-aware); returns the body range
    /// and moves past the `}`.
    fn brace_block(&mut self) -> Result<(usize, usize), Stop> {
        let open = self.pos;
        let body_start = open + 1;
        let mut quote: Option<char> = None;
        let rest = self.src.get(body_start..).unwrap_or("");
        for (i, c) in rest.char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '}') => {
                    self.pos = body_start + i + 1;
                    return Ok((body_start, body_start + i));
                }
                _ => {}
            }
        }
        Err(self.fail(open, open + 1, "expected `}` to close `@{`"))
    }

    /// `@{ shape: name, label: "text", … }`
    fn at_shape(&mut self) -> Result<(Shape, Option<String>), Stop> {
        self.pos += 1; // `@`; `brace_block` starts at `{`
        let (body_start, body_end) = self.brace_block()?;
        let body = self.src.get(body_start..body_end).unwrap_or("");
        // Split on `,` and newlines outside quotes.
        let mut pieces = Vec::new();
        let mut quote: Option<char> = None;
        let mut piece_start = 0usize;
        for (i, c) in body.char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, ',' | '\n') => {
                    pieces.push((piece_start, i));
                    piece_start = i + 1;
                }
                _ => {}
            }
        }
        pieces.push((piece_start, body.len()));
        let mut shape = Shape::Rect;
        let mut label_text = None;
        for (s, e) in pieces {
            let piece = body.get(s..e).unwrap_or("");
            if piece.trim().is_empty() {
                continue;
            }
            let abs = (body_start + s, body_start + e);
            let Some((key, value)) = piece.split_once(':') else {
                self.warn(
                    "W015",
                    abs.0,
                    abs.1,
                    alloc::format!(
                        "expected `key: value` in `@{{…}}`, found `{}`; ignored",
                        piece.trim()
                    ),
                );
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            let unquoted = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            match key {
                "shape" => match AT_SHAPES.binary_search_by(|(n, _)| n.cmp(&unquoted)) {
                    Ok(i) => shape = AT_SHAPES.get(i).map_or(Shape::Rect, |x| x.1),
                    Err(_) => self.warn(
                        "W015",
                        abs.0,
                        abs.1,
                        alloc::format!(
                            "shape `{}` is not supported; drawn as a rectangle",
                            unquoted
                        ),
                    ),
                },
                "label" => {
                    let text = self.finish_label(unquoted, abs.0, abs.1);
                    label_text = Some(text);
                }
                _ => self.warn(
                    "W015",
                    abs.0,
                    abs.1,
                    alloc::format!("`@{{…}}` key `{}` is not supported; ignored", key),
                ),
            }
        }
        Ok((shape, label_text))
    }

    /// Parses a link at the cursor, or returns `None` when there is none.
    fn link(&mut self) -> Result<Option<LinkTok>, Stop> {
        let s = self.pos;
        let mut i = s;
        let mut start = Arrow::None;
        match self.byte(i) {
            Some(b'<') if matches!(self.byte(i + 1), Some(b'-' | b'=')) => {
                start = Arrow::Arrow;
                i += 1;
            }
            Some(b @ (b'o' | b'x')) if self.is_link_start(i) => {
                start = if b == b'o' {
                    Arrow::Circle
                } else {
                    Arrow::Cross
                };
                i += 1;
            }
            _ => {}
        }
        let run = |p: &Self, from: usize, ch: u8| -> usize {
            let mut k = 0usize;
            while p.byte(from + k) == Some(ch) {
                k += 1;
            }
            k
        };
        let head = |p: &Self, at: usize| -> Option<Arrow> {
            match p.byte(at) {
                Some(b'>') => Some(Arrow::Arrow),
                Some(b'o') => Some(Arrow::Circle),
                Some(b'x') => Some(Arrow::Cross),
                _ => None,
            }
        };
        let len = |k: usize| u32::try_from(k).unwrap_or(u32::MAX).max(1);
        let mut tok = match self.byte(i) {
            Some(b'~') => {
                let k = run(self, i, b'~');
                if k < 3 || start != Arrow::None {
                    return Ok(None);
                }
                self.pos = i + k;
                LinkTok {
                    stroke: Stroke::Invisible,
                    start,
                    end: Arrow::None,
                    min_len: len(k - 2),
                    label: None,
                }
            }
            Some(ch @ (b'=' | b'-')) if !(ch == b'-' && self.byte(i + 1) == Some(b'.')) => {
                let stroke = if ch == b'=' {
                    Stroke::Thick
                } else {
                    Stroke::Normal
                };
                let k = run(self, i, ch);
                if k < 2 {
                    return Ok(None);
                }
                if let Some(h) = head(self, i + k) {
                    self.pos = i + k + 1;
                    LinkTok {
                        stroke,
                        start,
                        end: h,
                        min_len: len(k - 1),
                        label: None,
                    }
                } else if k >= 3 {
                    self.pos = i + k;
                    LinkTok {
                        stroke,
                        start,
                        end: Arrow::None,
                        min_len: len(k - 2),
                        label: None,
                    }
                } else {
                    self.text_link(stroke, start, i + 2)?
                }
            }
            Some(b'-') => {
                // Dotted: `-.` + more dots + `-` + optional head, or `-. text .->`.
                let d = run(self, i + 1, b'.');
                let j = i + 1 + d;
                if self.byte(j) == Some(b'-') {
                    let h = head(self, j + 1);
                    self.pos = j + 1 + usize::from(h.is_some());
                    LinkTok {
                        stroke: Stroke::Dotted,
                        start,
                        end: h.unwrap_or(Arrow::None),
                        min_len: len(d),
                        label: None,
                    }
                } else {
                    self.text_link(Stroke::Dotted, start, j)?
                }
            }
            _ => return Ok(None),
        };
        // `|label|` after the link.
        let save = self.pos;
        self.skip_hws();
        if self.peek() == Some('|') {
            tok.label = self.pipe_label()?;
        } else {
            self.pos = save;
        }
        Ok(Some(tok))
    }

    /// `-- text -->`, `== text ==>`, `-. text .->`: the label runs to the first end token.
    fn text_link(
        &mut self,
        stroke: Stroke,
        start: Arrow,
        text_start: usize,
    ) -> Result<LinkTok, Stop> {
        // An end token at `at`: (token end, end arrow, min length).
        let end_at = |p: &Self, at: usize| -> Option<(usize, Arrow, usize)> {
            let head = |x: usize| match p.byte(x) {
                Some(b'>') => Some(Arrow::Arrow),
                Some(b'o') => Some(Arrow::Circle),
                Some(b'x') => Some(Arrow::Cross),
                _ => None,
            };
            match stroke {
                Stroke::Dotted => {
                    let mut d = 0usize;
                    while p.byte(at + d) == Some(b'.') {
                        d += 1;
                    }
                    if d == 0 || p.byte(at + d) != Some(b'-') {
                        return None;
                    }
                    let h = head(at + d + 1);
                    Some((
                        at + d + 1 + usize::from(h.is_some()),
                        h.unwrap_or(Arrow::None),
                        d,
                    ))
                }
                _ => {
                    let ch = if stroke == Stroke::Thick { b'=' } else { b'-' };
                    let mut k = 0usize;
                    while p.byte(at + k) == Some(ch) {
                        k += 1;
                    }
                    if k < 2 {
                        return None;
                    }
                    match head(at + k) {
                        Some(h) => Some((at + k + 1, h, k - 1)),
                        None if k >= 3 => Some((at + k, Arrow::None, k - 2)),
                        None => None,
                    }
                }
            }
        };
        let finish = |p: &mut Self,
                      raw_start: usize,
                      raw_end: usize,
                      quoted: Option<(usize, usize)>,
                      end: (usize, Arrow, usize)| {
            let (ls, le) = quoted.unwrap_or((raw_start, raw_end));
            let raw = p.src.get(ls..le).unwrap_or("").to_string();
            let text = p.finish_label(&raw, ls, le);
            p.pos = end.0;
            LinkTok {
                stroke,
                start,
                end: end.1,
                min_len: u32::try_from(end.2).unwrap_or(u32::MAX).max(1),
                label: if text.is_empty() { None } else { Some(text) },
            }
        };
        // Quoted label: `-- "text" -->`.
        let mut q = text_start;
        while matches!(self.byte(q), Some(b' ' | b'\t')) {
            q += 1;
        }
        if let QuoteScan::Found(qs) = label::scan_quoted(self.src, q) {
            let mut after = qs.end;
            while matches!(self.byte(after), Some(b' ' | b'\t')) {
                after += 1;
            }
            if let Some(end) = end_at(self, after) {
                self.typographic(&qs.typographic);
                return Ok(finish(
                    self,
                    text_start,
                    after,
                    Some((qs.content_start, qs.content_end)),
                    end,
                ));
            }
        }
        let mut j = text_start;
        while let Some(c) = self.src.get(j..).and_then(|r| r.chars().next()) {
            if c == '\n' {
                break;
            }
            if let Some(end) = end_at(self, j) {
                return Ok(finish(self, text_start, j, None, end));
            }
            j += c.len_utf8();
        }
        let example = match stroke {
            Stroke::Thick => "==>",
            Stroke::Dotted => ".->",
            _ => "-->",
        };
        let at = text_start.saturating_sub(2);
        Err(self.fail(
            at,
            text_start,
            alloc::format!("expected the end of the link label, such as `{}`", example),
        ))
    }

    /// `|label|` at the cursor.
    fn pipe_label(&mut self) -> Result<Option<String>, Stop> {
        let open = self.pos;
        let content_start = open + 1;
        let mut q = content_start;
        while matches!(self.byte(q), Some(b' ' | b'\t')) {
            q += 1;
        }
        if let QuoteScan::Found(qs) = label::scan_quoted(self.src, q) {
            let mut after = qs.end;
            while matches!(self.byte(after), Some(b' ' | b'\t')) {
                after += 1;
            }
            let one_line = !self.src.get(q..qs.end).is_some_and(|s| s.contains('\n'));
            if self.byte(after) == Some(b'|') && one_line {
                self.typographic(&qs.typographic);
                let raw = self
                    .src
                    .get(qs.content_start..qs.content_end)
                    .unwrap_or("")
                    .to_string();
                let text = self.finish_label(&raw, qs.content_start, qs.content_end);
                self.pos = after + 1;
                return Ok(if text.is_empty() { None } else { Some(text) });
            }
        }
        let Some(close) = self
            .src
            .get(content_start..)
            .and_then(|r| r.find(['|', '\n']))
            .map(|i| content_start + i)
            .filter(|&i| self.byte(i) == Some(b'|'))
        else {
            return Err(self.fail(open, open + 1, "expected `|` to close the edge label"));
        };
        let raw = self.src.get(content_start..close).unwrap_or("").to_string();
        let text = self.finish_label(&raw, content_start, close);
        self.pos = close + 1;
        Ok(if text.is_empty() { None } else { Some(text) })
    }

    // ------------------------------------------------------------ subgraphs

    fn subgraph(&mut self, kw_start: usize) -> Result<(), Stop> {
        self.skip_hws();
        if self.at_stmt_end() {
            return Err(self.fail_here("expected a subgraph id or title after `subgraph`"));
        }
        if self.stack.len() >= self.opts.limits.nesting {
            let eol = self.eol_from(kw_start);
            let span = self.span(kw_start, eol);
            self.diags.emit(
                Severity::Error,
                "E010",
                span,
                alloc::format!("subgraphs nest deeper than {}", self.opts.limits.nesting),
            );
            return Err(Stop::Failed);
        }
        let (id, title) = match label::scan_quoted(self.src, self.pos) {
            QuoteScan::Found(qs) => {
                self.typographic(&qs.typographic);
                let raw = self
                    .src
                    .get(qs.content_start..qs.content_end)
                    .unwrap_or("")
                    .to_string();
                let title = self.finish_label(&raw, qs.content_start, qs.content_end);
                self.pos = qs.end;
                (None, title)
            }
            QuoteScan::Unterminated { .. } => {
                let p = self.pos;
                return Err(self.fail(p, p + 1, "unterminated string; expected a closing `\"`"));
            }
            QuoteScan::NotQuote => {
                let id_start = self.pos;
                let id_end = self.scan_id(id_start);
                if id_end == id_start {
                    return Err(self.fail_here("expected a subgraph id or title after `subgraph`"));
                }
                let raw_id = self.src.get(id_start..id_end).unwrap_or("").to_string();
                self.pos = id_end;
                let save = self.pos;
                self.skip_hws();
                if self.peek() == Some('[') {
                    let open = self.pos;
                    match self.label_in(open + 1, &[("]", Shape::Rect)])? {
                        Some(out) => {
                            self.pos = out.end;
                            self.typographic(&out.typographic);
                            let (rs, re) = out.raw;
                            let raw = self.src.get(rs..re).unwrap_or("").to_string();
                            if !out.quoted && label::needs_quoting(&raw) {
                                let span = self.span(rs, re);
                                self.repair(
                                    "R001",
                                    span,
                                    String::from("unquoted subgraph title contains `()`, `[]`, `{}` or `:`; quoted"),
                                    Fix {
                                        span,
                                        replacement: label::quote_label(&raw),
                                    },
                                );
                            }
                            (Some(raw_id), out.text)
                        }
                        None => {
                            return Err(self.fail(
                                open,
                                open + 1,
                                "expected `]` to close the subgraph title",
                            ));
                        }
                    }
                } else if self.at_stmt_end() {
                    self.pos = save;
                    (Some(raw_id.clone()), raw_id)
                } else {
                    // `subgraph Many words here`: the whole text is the title and the
                    // id is generated (mermaid's flowDb.addSubGraph).
                    let end = self.stmt_text_end();
                    let raw = self.src.get(id_start..end).unwrap_or("").to_string();
                    let title = self.finish_label(raw.trim_end(), id_start, end);
                    self.pos = end;
                    (None, title)
                }
            }
        };
        let id = match id {
            Some(id) => id,
            None => {
                let mut name = alloc::format!("subGraph{}", self.subs.len());
                while self.words.contains(&name) || self.sub_ids.contains(&name) {
                    name.push('_');
                }
                name
            }
        };
        let span = self.span(kw_start, self.pos);
        self.sub_ids.insert(id.clone());
        self.subs.push(SubB {
            id,
            title,
            parent: self.stack.last().copied(),
            direction: None,
            span,
        });
        self.stack.push(self.subs.len() - 1);
        Ok(())
    }

    fn direction_stmt(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        let start = self.pos;
        while self.peek().is_some_and(|c| !c.is_whitespace() && c != ';') {
            self.pos += self.peek().map_or(1, char::len_utf8);
        }
        let tok = self.src.get(start..self.pos).unwrap_or("");
        let Some(d) = parse_direction(tok) else {
            let msg = alloc::format!(
                "expected a direction (TB, TD, BT, LR or RL), found `{}`",
                tok
            );
            return Err(self.fail(start, self.pos, msg));
        };
        match self.stack.last().copied() {
            Some(s) => {
                if let Some(sub) = self.subs.get_mut(s) {
                    sub.direction = Some(d);
                }
            }
            None => self.direction = d,
        }
        Ok(())
    }

    // ------------------------------------------------------------ styles

    /// Parses the style list running to the end of the statement, reporting rejected
    /// declarations (`W010`) and fixed colours (`I030`, once per diagram).
    fn style_list(&mut self) -> Style {
        self.skip_hws();
        let start = self.pos;
        let end = self.stmt_text_end();
        let text = self.src.get(start..end).unwrap_or("");
        let parsed = style::parse_style_list(text);
        self.pos = end;
        for r in &parsed.rejected {
            self.warn("W010", start + r.start, start + r.end, r.message.clone());
        }
        if parsed.fixed_colour {
            let span = self.span(start, end);
            self.diags.emit_once(
                Severity::Info,
                "I030",
                span,
                "the source sets a fixed colour, which stays the same in every theme",
            );
        }
        parsed.style
    }

    fn class_def(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        let start = self.pos;
        while self.peek().is_some_and(|c| !c.is_whitespace() && c != ';') {
            self.pos += self.peek().map_or(1, char::len_utf8);
        }
        if self.pos == start {
            return Err(self.fail_here("expected a class name after `classDef`"));
        }
        let names = self.src.get(start..self.pos).unwrap_or("").to_string();
        let mut valid = Vec::new();
        let mut offset = start;
        for name in names.split(',') {
            let (ns, ne) = (offset, offset + name.len());
            offset = ne + 1;
            if valid_class_name(name) {
                valid.push(name.to_string());
            } else {
                self.warn(
                    "W011",
                    ns,
                    ne,
                    alloc::format!("class name `{}` is outside `[A-Za-z_][A-Za-z0-9_-]{{0,63}}`; `classDef` dropped", name),
                );
            }
        }
        let style = self.style_list();
        for name in valid {
            match self.class_defs.iter_mut().find(|c| c.name == name) {
                Some(existing) => existing.style.merge(&style),
                None => self.class_defs.push(ClassDef {
                    name,
                    style: style.clone(),
                }),
            }
        }
        Ok(())
    }

    /// `id(, id)*` for `class`; returns (id, span) pairs after `R004`.
    fn id_list(&mut self, after: &str) -> Result<Vec<(String, Span)>, Stop> {
        let mut ids = Vec::new();
        loop {
            self.skip_hws();
            let s = self.pos;
            let e = self.scan_id(s);
            if e == s {
                return Err(self.fail_here(&alloc::format!("expected a node id after `{}`", after)));
            }
            let id = self.reference_id(s, e);
            ids.push((id, self.span(s, e)));
            self.pos = e;
            self.skip_hws();
            if self.peek() == Some(',') {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(ids)
    }

    fn class_stmt(&mut self) -> Result<(), Stop> {
        let ids = self.id_list("class")?;
        self.skip_hws();
        let start = self.pos;
        while self.peek().is_some_and(|c| !c.is_whitespace() && c != ';') {
            self.pos += self.peek().map_or(1, char::len_utf8);
        }
        if self.pos == start {
            return Err(self.fail_here("expected a class name after the node ids"));
        }
        let names = self.src.get(start..self.pos).unwrap_or("").to_string();
        let mut offset = start;
        for name in names.split(',') {
            let (ns, ne) = (offset, offset + name.len());
            offset = ne + 1;
            if !valid_class_name(name) {
                self.warn(
                    "W011",
                    ns,
                    ne,
                    alloc::format!(
                        "class name `{}` is outside `[A-Za-z_][A-Za-z0-9_-]{{0,63}}`; dropped",
                        name
                    ),
                );
                continue;
            }
            for (id, _) in &ids {
                self.ops.push(Op::Class {
                    id: id.clone(),
                    class: name.to_string(),
                });
            }
        }
        Ok(())
    }

    fn style_stmt(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        let s = self.pos;
        let e = self.scan_id(s);
        if e == s {
            return Err(self.fail_here("expected a node id after `style`"));
        }
        let id = self.reference_id(s, e);
        let span = self.span(s, e);
        self.pos = e;
        let style = self.style_list();
        self.ops.push(Op::Style { id, span, style });
        Ok(())
    }

    fn link_style(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        let target = if self.starts_with("default")
            && !self
                .byte(self.pos + 7)
                .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            self.pos += 7;
            LinkTarget::Default
        } else {
            let mut list = Vec::new();
            loop {
                self.skip_hws();
                let s = self.pos;
                let mut v: usize = 0;
                while let Some(b) = self.byte(self.pos).filter(u8::is_ascii_digit) {
                    v = v.saturating_mul(10).saturating_add(usize::from(b - b'0'));
                    self.pos += 1;
                }
                if self.pos == s {
                    return Err(self.fail_here(
                        "expected `default` or a list of edge indices after `linkStyle`",
                    ));
                }
                list.push((v, self.span(s, self.pos)));
                self.skip_hws();
                if self.peek() == Some(',') {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            LinkTarget::Indices(list)
        };
        // `interpolate <curve>` sets a per-edge curve, which the model does not carry.
        self.skip_hws();
        if self.starts_with("interpolate") {
            self.pos += "interpolate".len();
            self.skip_hws();
            while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
                self.pos += 1;
            }
        }
        let style = self.style_list();
        match target {
            LinkTarget::Default => self.default_link_style.merge(&style),
            LinkTarget::Indices(list) => self.link_styles.push((list, style)),
        }
        Ok(())
    }

    // ------------------------------------------------------------ click and acc*

    /// One click argument: a quoted string (content range) or a bare word.
    fn click_token(&mut self) -> Option<(usize, usize, bool)> {
        self.skip_hws();
        if self.at_stmt_end() {
            return None;
        }
        if let QuoteScan::Found(qs) = label::scan_quoted(self.src, self.pos) {
            self.typographic(&qs.typographic);
            self.pos = qs.end;
            return Some((qs.content_start, qs.content_end, true));
        }
        let s = self.pos;
        while self.peek().is_some_and(|c| !c.is_whitespace() && c != ';') {
            self.pos += self.peek().map_or(1, char::len_utf8);
        }
        Some((s, self.pos, false))
    }

    /// Skips the rest of a statement, stepping over quoted strings.
    fn skip_statement(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' || c == ';' {
                break;
            }
            if let QuoteScan::Found(qs) = label::scan_quoted(self.src, self.pos) {
                self.pos = qs.end;
            } else {
                self.pos += c.len_utf8();
            }
        }
    }

    fn click(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        let s = self.pos;
        let e = self.scan_id(s);
        if e == s {
            return Err(self.fail_here("expected a node id after `click`"));
        }
        let id = self.reference_id(s, e);
        self.pos = e;
        let Some((ts, te, quoted)) = self.click_token() else {
            return Err(self.fail_here("expected `href`, a URL or a callback after the node id"));
        };
        let word = self.src.get(ts..te).unwrap_or("");
        let url = if !quoted && word == "href" {
            match self.click_token() {
                Some(u) => u,
                None => return Err(self.fail_here("expected a URL after `href`")),
            }
        } else if quoted {
            (ts, te, true)
        } else {
            // `click id callback` / `click id call fn()`: page JavaScript is never called.
            let stmt_start = s;
            self.skip_statement();
            let span = self.span(stmt_start, self.pos);
            self.diags.emit(
                Severity::Info,
                "I031",
                span,
                "`click` callbacks are ignored; the output never calls page JavaScript",
            );
            return Ok(());
        };
        let mut target_blank = false;
        while let Some((as_, ae, q)) = self.click_token() {
            let arg = self.src.get(as_..ae).unwrap_or("");
            if q {
                continue; // tooltip
            }
            match arg {
                "_blank" => target_blank = true,
                "_self" | "_parent" | "_top" => target_blank = false,
                _ => {
                    return Err(self.fail(
                        as_,
                        ae,
                        alloc::format!(
                            "expected a tooltip or a link target (`_self`, `_blank`), found `{}`",
                            arg
                        ),
                    ));
                }
            }
        }
        let raw = self.src.get(url.0..url.1).unwrap_or("");
        match link::check_url(raw) {
            Some(url) => self.ops.push(Op::Click {
                id,
                link: Link { url, target_blank },
            }),
            None => self.warn(
                "W013",
                url.0,
                url.1,
                String::from("link URL is not relative, `https`, `http` or `mailto`; dropped"),
            ),
        }
        Ok(())
    }

    /// `accTitle: text` / `accDescr: text`, to the end of the line.
    fn acc_line(&mut self, title: bool) -> Result<(), Stop> {
        self.skip_hws();
        self.pos += 1; // `:`
        let start = self.pos;
        let end = self.eol_from(start);
        let raw = self.src.get(start..end).unwrap_or("").trim().to_string();
        let mut text = raw;
        let max = self.opts.limits.label_bytes;
        if label::truncate(&mut text, max) {
            self.warn(
                "W012",
                start,
                end,
                alloc::format!("text longer than {} bytes; truncated", max),
            );
        }
        if title {
            self.meta.acc_title = Some(text);
        } else {
            self.meta.acc_descr = Some(text);
        }
        self.pos = end;
        Ok(())
    }

    /// `accDescr { … }`, possibly over several lines.
    fn acc_block(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        let open = self.pos;
        let body_start = open + 1;
        let Some(close) = self
            .src
            .get(body_start..)
            .and_then(|r| r.find('}'))
            .map(|i| body_start + i)
        else {
            return Err(self.fail(open, open + 1, "expected `}` to close `accDescr {`"));
        };
        let body = self.src.get(body_start..close).unwrap_or("");
        let lines: Vec<&str> = body
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        let mut text = lines.join("\n");
        let max = self.opts.limits.label_bytes;
        if label::truncate(&mut text, max) {
            self.warn(
                "W012",
                body_start,
                close,
                alloc::format!("text longer than {} bytes; truncated", max),
            );
        }
        self.meta.acc_descr = Some(text);
        self.pos = close + 1;
        Ok(())
    }

    // ------------------------------------------------------------ model

    fn finish(mut self) -> Flowchart {
        // Edges to a subgraph id: Mermaid connects the edge to the cluster. The model
        // has node-to-node edges only, so the endpoint becomes the subgraph's first
        // member node (in declaration order, nested subgraphs included).
        let mut sub_by_id: BTreeMap<String, usize> = BTreeMap::new();
        for (i, s) in self.subs.iter().enumerate() {
            sub_by_id.entry(s.id.clone()).or_insert(i);
        }
        // A node that only names a subgraph (a bare endpoint, a bare statement, or
        // `id@{…}` without a shape or label) stands for the subgraph.
        let candidate: Vec<Option<usize>> = self
            .nodes
            .iter()
            .map(|n| {
                if n.shaped {
                    None
                } else {
                    sub_by_id.get(&n.id).copied()
                }
            })
            .collect();
        // A subgraph named inside another subgraph nests there (mermaid resolves the
        // name at the end, so the subgraph may be declared later). Links that would
        // close a cycle are not made.
        let ancestor_or_self = |subs: &[SubB], x: usize, mut cur: Option<usize>| {
            let mut steps = 0usize;
            while let Some(c) = cur {
                if c == x {
                    return true;
                }
                if steps > subs.len() {
                    return true;
                }
                steps += 1;
                cur = subs.get(c).and_then(|s| s.parent);
            }
            false
        };
        let mut absorbed: Vec<bool> = alloc::vec![false; self.nodes.len()];
        for (n, node) in self.nodes.iter().enumerate() {
            let (Some(x), Some(s)) = (candidate.get(n).copied().flatten(), node.subgraph) else {
                continue;
            };
            let Some(parent) = self.subs.get(x).map(|sub| sub.parent) else {
                continue;
            };
            if parent.is_none() && !ancestor_or_self(&self.subs, x, Some(s)) {
                if let Some(sub) = self.subs.get_mut(x) {
                    sub.parent = Some(s);
                }
            }
            if self.subs.get(x).and_then(|sub| sub.parent) == Some(s) {
                absorbed[n] = true;
            }
        }
        // First real member of every subgraph: each node walks up its ancestor chain,
        // which the nesting limit bounds, so this is O(nodes × nesting).
        let mut first_member: Vec<Option<usize>> = alloc::vec![None; self.subs.len()];
        for (m, node) in self.nodes.iter().enumerate() {
            if candidate.get(m).copied().flatten().is_some() {
                continue;
            }
            let mut cur = node.subgraph;
            let mut steps = 0usize;
            while let Some(s) = cur {
                if steps > self.subs.len() {
                    break; // parent links always point backwards; this guards regardless
                }
                steps += 1;
                if let Some(slot) = first_member.get_mut(s) {
                    if slot.is_none() {
                        *slot = Some(m);
                    }
                }
                cur = self.subs.get(s).and_then(|sub| sub.parent);
            }
        }
        let redirect: Vec<Option<usize>> = candidate
            .iter()
            .map(|c| c.and_then(|s| first_member.get(s).copied().flatten()))
            .collect();
        // Whether node `m` lies inside subgraph `x` (at any depth).
        let inside = |m: usize, x: usize| -> bool {
            let cur = self.nodes.get(m).and_then(|node| node.subgraph);
            ancestor_or_self(&self.subs, x, cur)
        };
        // An edge between a subgraph and one of its own members is dropped, as mermaid
        // drops it.
        let keep_edge = |a: usize, b: usize| -> bool {
            let into = |sub: usize, other: usize| {
                candidate.get(sub).copied().flatten().is_some_and(|x| {
                    redirect.get(sub).copied().flatten().is_some()
                        && candidate.get(other).copied().flatten().is_none()
                        && inside(other, x)
                })
            };
            !into(a, b) && !into(b, a)
        };
        let keep: Vec<bool> = self.edges.iter().map(|e| keep_edge(e.from, e.to)).collect();
        // A bare endpoint naming a subgraph is not a typo, so it never gets `R005`,
        // even when the subgraph is empty and the endpoint stays a node.
        let names_subgraph: Vec<bool> = candidate.iter().map(Option::is_some).collect();

        // R005 for nodes that only ever appear as bare edge endpoints.
        for (n, node) in self.nodes.iter().enumerate() {
            if node.declared || names_subgraph.get(n).copied().unwrap_or(false) {
                continue;
            }
            let (s, e) = node.first_id;
            let span = self.idx.span(s, e);
            let fix_span = self.idx.span(e, e);
            self.diags.push(Diagnostic {
                severity: Severity::Repair,
                code: "R005",
                span,
                message: alloc::format!(
                    "node `{}` is never declared; declared with its id as the label",
                    node.id
                ),
                fix: Some(Fix {
                    span: fix_span,
                    replacement: alloc::format!("[{}]", node.id),
                }),
            });
        }

        // Final node list and index map.
        let mut new_index: Vec<Option<usize>> = alloc::vec![None; self.nodes.len()];
        let mut nodes: Vec<Node> = Vec::new();
        for (n, b) in core::mem::take(&mut self.nodes).into_iter().enumerate() {
            if redirect.get(n).copied().flatten().is_some() || absorbed.get(n) == Some(&true) {
                continue;
            }
            if let Some(slot) = new_index.get_mut(n) {
                *slot = Some(nodes.len());
            }
            nodes.push(Node {
                id: b.id,
                label: b.label,
                shape: b.shape,
                classes: Vec::new(),
                style: Style::default(),
                link: None,
                subgraph: b.subgraph,
                span: b.span,
            });
        }
        let resolve = |n: usize| -> Option<usize> {
            let target = redirect.get(n).copied().flatten().unwrap_or(n);
            new_index.get(target).copied().flatten()
        };
        let mut edges: Vec<Edge> = Vec::new();
        for (e, _) in self.edges.iter().zip(&keep).filter(|(_, &k)| k) {
            if let (Some(from), Some(to)) = (resolve(e.from), resolve(e.to)) {
                edges.push(Edge {
                    from,
                    to,
                    label: e.label.clone(),
                    stroke: e.stroke,
                    arrow_start: e.arrow_start,
                    arrow_end: e.arrow_end,
                    min_len: e.min_len,
                    style: Style::default(),
                    span: e.span,
                });
            }
        }

        // `class`, `:::`, `style` and `click`, in source order.
        let by_id: BTreeMap<String, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        // Subgraph ids that no node took: `class` and `style` apply to the cluster.
        let mut sub_look: BTreeMap<String, (Vec<String>, Style)> = BTreeMap::new();
        for op in core::mem::take(&mut self.ops) {
            match op {
                Op::Class { id, class } => {
                    if let Some(node) = by_id.get(&id).and_then(|&i| nodes.get_mut(i)) {
                        if !node.classes.contains(&class) {
                            node.classes.push(class);
                        }
                    } else if sub_by_id.contains_key(&id) {
                        let classes = &mut sub_look.entry(id).or_default().0;
                        if !classes.contains(&class) {
                            classes.push(class);
                        }
                    }
                }
                Op::Style { id, span, style } => {
                    match by_id.get(&id).and_then(|&i| nodes.get_mut(i)) {
                        Some(node) => node.style.merge(&style),
                        None if sub_by_id.contains_key(&id) => {
                            sub_look.entry(id).or_default().1.merge(&style);
                        }
                        None => self.diags.emit(
                            Severity::Warning,
                            "W010",
                            span,
                            alloc::format!("style target `{}` is not a node; ignored", id),
                        ),
                    }
                }
                Op::Click { id, link } => {
                    if let Some(node) = by_id.get(&id).and_then(|&i| nodes.get_mut(i)) {
                        node.link = Some(link);
                    }
                }
            }
        }

        // `linkStyle`: indices first, then the default underneath.
        let edge_count = edges.len();
        for (list, style) in &self.link_styles {
            for &(i, span) in list {
                match edges.get_mut(i) {
                    Some(e) => e.style.merge(style),
                    None => self.diags.emit(
                        Severity::Warning,
                        "W010",
                        span,
                        alloc::format!(
                            "linkStyle index {} is out of range: the diagram has {} edges; ignored",
                            i,
                            edge_count
                        ),
                    ),
                }
            }
        }
        for e in &mut edges {
            let mut s = self.default_link_style.clone();
            s.merge(&e.style);
            e.style = s;
        }

        // Parents precede their children (nesting can point forwards after the
        // resolution above): declaration order, each subgraph after its ancestors.
        let k = self.subs.len();
        let mut order: Vec<usize> = Vec::with_capacity(k);
        let mut placed = alloc::vec![false; k];
        for i in 0..k {
            let mut chain = Vec::new();
            let mut cur = Some(i);
            while let Some(c) = cur {
                if placed.get(c) != Some(&false) || chain.contains(&c) || chain.len() > k {
                    break;
                }
                chain.push(c);
                cur = self.subs.get(c).and_then(|s| s.parent);
            }
            for &c in chain.iter().rev() {
                placed[c] = true;
                order.push(c);
            }
        }
        let mut new_sub = alloc::vec![0usize; k];
        for (at, &old) in order.iter().enumerate() {
            new_sub[old] = at;
        }
        for node in nodes.iter_mut() {
            node.subgraph = node.subgraph.and_then(|s| new_sub.get(s).copied());
        }
        let mut old_subs: Vec<Option<SubB>> = self.subs.into_iter().map(Some).collect();
        let mut subgraphs: Vec<Subgraph> = order
            .iter()
            .filter_map(|&old| old_subs.get_mut(old).and_then(Option::take))
            .map(|s| {
                let (classes, style) = sub_look.remove(&s.id).unwrap_or_default();
                Subgraph {
                    id: s.id,
                    title: s.title,
                    parent: s.parent.and_then(|p| new_sub.get(p).copied()),
                    nodes: Vec::new(),
                    direction: s.direction,
                    classes,
                    style,
                    span: s.span,
                }
            })
            .collect();
        for (i, n) in nodes.iter().enumerate() {
            if let Some(sub) = n.subgraph.and_then(|s| subgraphs.get_mut(s)) {
                sub.nodes.push(i);
            }
        }

        Flowchart {
            meta: self.meta,
            direction: self.direction,
            nodes,
            edges,
            subgraphs,
            class_defs: self.class_defs,
            default_link_style: self.default_link_style,
        }
    }
}
