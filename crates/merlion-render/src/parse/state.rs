//! The `stateDiagram` / `stateDiagram-v2` parser (specs/state.md#syntax), following the
//! grammar mermaid 12 documents for state machines: states and descriptions, transitions,
//! `[*]`, composite states, concurrency regions, notes, `direction`, the style statements
//! and the accessibility statements.
//!
//! Both headers parse to the same model: mermaid keeps two renderers and points
//! `stateDiagram` at the older one, which is a difference of appearance, not of meaning.
//!
//! The parser is statement-oriented and never recurses: composite states nest on an
//! explicit stack bounded by `Limits::nesting` (`E010`), and every scan is bounded by the
//! end of the current statement, which ends at the first `;` or unmatched `}` that closes
//! no entity code (`#59;`) or at the end of the line. The end of the line is found once
//! per line and cached, not once per statement, so a statement never pays for the rest
//! of its line and parsing stays linear even on a single 1 MiB line.
//!
//! `[*]` resolves per scope: the top level is one scope, each composite state is another,
//! and a composite split by `--` gives one scope per region. A scope's start and end
//! states are allocated where the first `[*]` of that scope stands, which keeps
//! `states` in declaration order, and their ids are assigned once the whole source is
//! read, so a generated id never shadows a declared one
//! (specs/state.md#what-the-lowering-guarantees).
//!
//! Diagnostics this module adds (specs/state.md#diagnostics): `W024` dropped statement,
//! `W025` rejected state kind, `R014` unclosed composite state, `R015` unmatched `}`,
//! `R016` `--` outside a composite state, `R017` transition text without its `:`,
//! `R018` an arrow other than `-->`, `R019` a block note without its `end note`.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::Cell;

use super::cursor::LineIndex;
use super::flowchart::MAX_CLASSES;
use super::label::{self, QuoteScan};
use super::{link, style, ParseOptions, Stop};
use crate::diag::{excerpt, Diagnostic, Diagnostics, Fix, Severity, Span};
use crate::model::state::{
    Note, NotePlacement, Region, State, StateKind, StateMachine, Transition,
};
use crate::model::{ClassDef, Link, Meta, Style};
use crate::options::Direction;

/// The one arrow the grammar defines (specs/state.md#transitions).
const CANONICAL: &str = "-->";

/// Every arrow token, longest first so the first match is the longest. Each one other
/// than [`CANONICAL`] is read as `-->` with `R018`: they come from flowchart and sequence
/// habits and mean one thing here.
const ARROWS: &[&str] = &[
    "<-->", "-.->", "--->", "-->>", "===>", "<->", "-.-", "->>", "==>", "-->", "->", "=>",
];

/// The arrow token starting at `at`, if any.
fn arrow_at(src: &str, at: usize) -> Option<&'static str> {
    let rest = src.get(at..)?;
    ARROWS.iter().copied().find(|t| rest.starts_with(t))
}

/// Length of the entity code starting just after a `#` or `&`, through its `;` (`35;`,
/// `x2665;`, `hearts;`), or `None` when the `#` opens a comment instead.
fn entity_len(after_hash: &str) -> Option<usize> {
    for (i, c) in after_hash.char_indices() {
        if c == ';' {
            return if i == 0 { None } else { Some(i + 1) };
        }
        if !c.is_ascii_alphanumeric() || i > 8 {
            return None;
        }
    }
    None
}

/// Keywords that open a statement of their own, so a line starting with one cannot be
/// the text of a note block left open.
const STATEMENT_KEYWORDS: &[&str] = &[
    "state",
    "note",
    "direction",
    "class",
    "classdef",
    "style",
    "click",
    "link",
    "acctitle",
    "accdescr",
];

/// Whether a line can only be the next statement rather than more note text: it carries
/// a transition arrow, or it opens with `}`, `[*]`, `--` or a keyword the grammar
/// reserves. An `end note` line is the block's own terminator and is matched before this
/// (specs/state.md#notes).
fn starts_a_statement(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    // Every arrow in `ARROWS` but `-.-` holds `->` or `=>`.
    if t.contains("->") || t.contains("=>") || t.contains("-.-") {
        return true;
    }
    if t.starts_with('}') || t.starts_with("[*]") || t.starts_with("--") {
        return true;
    }
    let word = t.split_whitespace().next().unwrap_or("");
    STATEMENT_KEYWORDS
        .iter()
        .any(|k| word.eq_ignore_ascii_case(k))
}

/// Cuts `text` at the comment it holds: `%%`, or a `#` that opens no entity code
/// (specs/state.md#accessibility-title-and-comments).
fn cut_comment(text: &str) -> &str {
    let mut i = 0usize;
    while let Some(j) = text.get(i..).and_then(|r| r.find(['%', '#'])) {
        let at = i + j;
        if text.as_bytes().get(at) == Some(&b'%') {
            if text.get(at..at + 2) == Some("%%") {
                return text.get(..at).unwrap_or("");
            }
            i = at + 1;
            continue;
        }
        match entity_len(text.get(at + 1..).unwrap_or("")) {
            Some(len) => i = at + 1 + len,
            None => return text.get(..at).unwrap_or(""),
        }
    }
    text
}

/// Whether the whole statement is a comment: `%%` or a `#` that opens no entity.
fn is_comment_start(rest: &str) -> bool {
    rest.starts_with("%%") || (rest.starts_with('#') && cut_comment(rest).is_empty())
}

/// Whether the statement is a concurrency divider: two or more `-` and nothing else
/// (specs/state.md#concurrency).
fn is_divider(text: &str) -> bool {
    let t = text.trim();
    t.len() >= 2 && t.bytes().all(|b| b == b'-')
}

fn is_id_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
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

/// Offset of the `:` that marks a label, skipping the `:::` role shorthand so
/// `A:::role --> B` still finds its arrow (specs/state.md#styling).
fn label_colon(text: &str) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b.get(i) == Some(&b':') {
            if b.get(i + 1) == Some(&b':') && b.get(i + 2) == Some(&b':') {
                i += 3;
                continue;
            }
            return Some(i);
        }
        i += 1;
    }
    None
}

/// One state under construction.
struct StateB {
    id: String,
    label: String,
    kind: StateKind,
    parent: Option<usize>,
    children: Vec<usize>,
    direction: Option<Direction>,
    classes: Vec<String>,
    style: Style,
    link: Option<Link>,
    implicit: bool,
    /// Whether a description already names the state, so the next one adds a line
    /// rather than replacing the id (specs/state.md#states).
    described: bool,
    span: Span,
    /// Concurrency regions, filled when the composite closes with two or more of them.
    regions: Vec<RegionB>,
    /// For `[*]`: the scope it belongs to, and whether it is the start of that scope.
    pseudo: Option<(usize, bool)>,
}

struct RegionB {
    states: Vec<usize>,
    span: Span,
}

/// One `[*]` scope: the top level, a composite state, or one of its regions.
struct Scope {
    /// The composite that owns the scope; `None` for the top level.
    owner: Option<usize>,
    /// Final region index within the owner, or `None` once the composite turns out to
    /// have fewer than two non-empty regions.
    region: Option<usize>,
    start: Option<usize>,
    end: Option<usize>,
}

/// An open composite state.
struct Frame {
    state: usize,
    /// One scope per `--`-separated bucket, in source order.
    scopes: Vec<usize>,
    /// Direct members per bucket, in declaration order.
    buckets: Vec<Vec<usize>>,
    /// The span each bucket's region takes: the composite header, then each divider.
    spans: Vec<Span>,
    span: Span,
}

/// Statements about states that apply once every state and every generated id is known,
/// in source order.
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

/// What an endpoint of a transition, or a bare statement, names.
enum End {
    /// `[*]`: the scope's start or end, depending on the side it stands on.
    Pseudo,
    Ident {
        id: String,
        classes: Vec<String>,
        span: Span,
    },
}

struct P<'a, 'd> {
    src: &'a str,
    idx: &'a LineIndex<'a>,
    pos: usize,
    /// `(from, end)` of the last [`P::eol_from`]: `src[from..end]` holds no `\n`, so
    /// every offset in `from..=end` ends its line at `end`.
    line: Cell<(usize, usize)>,
    opts: &'a ParseOptions,
    diags: &'d mut Diagnostics,
    meta: Meta,
    direction: Direction,
    states: Vec<StateB>,
    by_id: BTreeMap<String, usize>,
    transitions: Vec<Transition>,
    notes: Vec<Note>,
    scopes: Vec<Scope>,
    stack: Vec<Frame>,
    class_defs: Vec<ClassDef>,
    ops: Vec<Op>,
}

/// Parses the body of a state diagram starting at `pos`, which is the offset just after
/// the header word. Repairs, warnings and infos go to `diags`.
pub(crate) fn parse_state(
    idx: &LineIndex,
    pos: usize,
    meta: Meta,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Result<StateMachine, Stop> {
    let mut p = P {
        src: idx.src(),
        idx,
        pos,
        // An empty range no offset falls in, so the first lookup misses and fills it.
        line: Cell::new((1, 0)),
        opts,
        diags,
        meta,
        direction: Direction::TB,
        states: Vec::new(),
        by_id: BTreeMap::new(),
        transitions: Vec::new(),
        notes: Vec::new(),
        scopes: alloc::vec![Scope {
            owner: None,
            region: None,
            start: None,
            end: None,
        }],
        stack: Vec::new(),
        class_defs: Vec::new(),
        ops: Vec::new(),
    };
    p.header_direction();
    p.statements()?;
    Ok(p.finish())
}

impl P<'_, '_> {
    // ------------------------------------------------------------ cursor helpers

    fn rest_at(&self, at: usize) -> &str {
        self.src.get(at..).unwrap_or("")
    }

    fn char_at(&self, at: usize) -> Option<char> {
        self.rest_at(at).chars().next()
    }

    fn skip_hws(&mut self) {
        self.pos = self.skip_hws_from(self.pos);
    }

    fn skip_hws_from(&self, mut at: usize) -> usize {
        while matches!(self.src.as_bytes().get(at), Some(b' ' | b'\t' | b'\r')) {
            at += 1;
        }
        at
    }

    /// Offset of the end of the line containing `at`: the `\n` that ends it, or the end
    /// of input.
    ///
    /// The last answer is cached with the range `from..=end` it holds for: `src[from..end]`
    /// carries no `\n`, so every offset in that range ends its line at `end`.
    /// A statement therefore scans to the `\n` once per line rather than once per
    /// statement: without the cache a source separated by `;` instead of newlines pays
    /// for the rest of its line at every statement, which is quadratic in the length of
    /// the line.
    fn eol_from(&self, at: usize) -> usize {
        let (from, end) = self.line.get();
        if at >= from && at <= end {
            return end;
        }
        let end = self
            .rest_at(at)
            .find('\n')
            .map_or(self.src.len(), |i| at + i);
        self.line.set((at, end));
        end
    }

    /// End of the statement starting at `at`: the first `;` or unmatched `}` that closes
    /// no entity code, or the end of the line. A `}` inside a balanced `{…}` stays text,
    /// so a label may hold one.
    fn stmt_end(&self, at: usize) -> usize {
        let eol = self.eol_from(at);
        let text = self.src.get(at..eol).unwrap_or("");
        let mut depth = 0usize;
        let mut i = 0usize;
        while i < text.len() {
            match text.as_bytes().get(i) {
                Some(b';') if depth == 0 => return at + i,
                Some(b'{') => {
                    depth += 1;
                    i += 1;
                }
                Some(b'}') => {
                    if depth == 0 {
                        return at + i;
                    }
                    depth -= 1;
                    i += 1;
                }
                // Both spellings of an entity carry a `;` that separates nothing: the
                // `#lt;` the grammar defines, and the `&lt;` an HTML source escapes to.
                Some(b'#') | Some(b'&') => match entity_len(text.get(i + 1..).unwrap_or("")) {
                    Some(len) => i += 1 + len,
                    None => i += 1,
                },
                _ => i += 1,
            }
        }
        eol
    }

    /// End of the current statement's text for a style list: the next `;` or newline.
    fn stmt_text_end(&self) -> usize {
        self.rest_at(self.pos)
            .find([';', '\n'])
            .map_or(self.src.len(), |i| self.pos + i)
    }

    /// The run of id characters at `at`: what a keyword is read from.
    fn word_end(&self, at: usize) -> usize {
        let mut end = at;
        for (i, c) in self.rest_at(at).char_indices() {
            if is_id_char(c) {
                end = at + i + c.len_utf8();
            } else {
                break;
            }
        }
        end
    }

    /// The run an id occupies in a `class` / `style` / `click` list: `-` and `.` join id
    /// parts (`my-state`, `v1.2`) but never start an arrow.
    fn scan_id(&self, from: usize) -> usize {
        let mut end = from;
        let mut chars = self.rest_at(from).char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let ok = if is_id_char(c) {
                true
            } else if (c == '-' || c == '.') && end > from {
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

    /// The span of a statement, without its trailing whitespace.
    fn stmt_span(&self, start: usize, end: usize) -> Span {
        let text = self.src.get(start..end).unwrap_or("");
        self.span(start, start + text.trim_end().len())
    }

    /// The byte range a repair deletes for the statement `start..end`: the whole line
    /// when the statement owns it, the statement alone otherwise.
    fn delete_to(&self, start: usize, end: usize) -> usize {
        if super::cursor::at_line_start(self.src, start) {
            self.idx.next_line_start(start)
        } else {
            end
        }
    }

    fn fail(&mut self, start: usize, end: usize, message: impl Into<String>) -> Stop {
        let span = self.span(start, end);
        self.diags.emit(Severity::Error, "E002", span, message);
        Stop::Failed
    }

    fn fail_here(&mut self, expected: &str) -> Stop {
        let (found, len) = match self.char_at(self.pos) {
            None => (String::from("end of input"), 0),
            Some('\n') => (String::from("end of line"), 0),
            Some(c) => (alloc::format!("`{}`", c), c.len_utf8()),
        };
        let pos = self.pos;
        self.fail(pos, pos + len, alloc::format!("{expected}, found {found}"))
    }

    fn warn(&mut self, code: &'static str, start: usize, end: usize, message: impl Into<String>) {
        let span = self.stmt_span(start, end);
        self.diags.emit(Severity::Warning, code, span, message);
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
                alloc::format!("label longer than {max} bytes; truncated"),
            );
        }
        text
    }

    // ------------------------------------------------------------ scopes and states

    /// The composite state the current statement lands in.
    fn container(&self) -> Option<usize> {
        self.stack.last().map(|f| f.state)
    }

    /// The `[*]` scope the current statement resolves in.
    fn scope(&self) -> usize {
        self.stack
            .last()
            .and_then(|f| f.scopes.last().copied())
            .unwrap_or(0)
    }

    /// Records `i` as a direct member of the open composite state, if any.
    fn add_member(&mut self, i: usize) {
        if let Some(bucket) = self.stack.last_mut().and_then(|f| f.buckets.last_mut()) {
            bucket.push(i);
        }
    }

    /// The state with `id`, declared where it first stands. Naming a state in a
    /// transition is idiomatic, so an implicit declaration carries no diagnostic
    /// (specs/state.md#diagnostics).
    fn state_ref(&mut self, id: &str, span: Span) -> Result<usize, Stop> {
        if let Some(&i) = self.by_id.get(id) {
            return Ok(i);
        }
        if self.states.len() >= self.opts.limits.nodes {
            return Err(Stop::TooLarge("states"));
        }
        let parent = self.container();
        self.states.push(StateB {
            id: id.to_string(),
            label: id.to_string(),
            kind: StateKind::Simple,
            parent,
            children: Vec::new(),
            direction: None,
            classes: Vec::new(),
            style: Style::default(),
            link: None,
            implicit: true,
            described: false,
            span,
            regions: Vec::new(),
            pseudo: None,
        });
        let i = self.states.len() - 1;
        self.by_id.insert(id.to_string(), i);
        self.add_member(i);
        Ok(i)
    }

    /// The scope's start or end state, allocated where the first `[*]` of that scope
    /// stands (specs/state.md#start-and-end).
    fn pseudo_state(&mut self, start: bool, span: Span) -> Result<usize, Stop> {
        let scope = self.scope();
        let existing = self
            .scopes
            .get(scope)
            .and_then(|s| if start { s.start } else { s.end });
        if let Some(i) = existing {
            return Ok(i);
        }
        if self.states.len() >= self.opts.limits.nodes {
            return Err(Stop::TooLarge("states"));
        }
        let parent = self.container();
        self.states.push(StateB {
            id: String::new(),
            label: String::new(),
            kind: if start {
                StateKind::Start
            } else {
                StateKind::End
            },
            parent,
            children: Vec::new(),
            direction: None,
            classes: Vec::new(),
            style: Style::default(),
            link: None,
            implicit: true,
            described: false,
            span,
            regions: Vec::new(),
            pseudo: Some((scope, start)),
        });
        let i = self.states.len() - 1;
        if let Some(s) = self.scopes.get_mut(scope) {
            if start {
                s.start = Some(i);
            } else {
                s.end = Some(i);
            }
        }
        self.add_member(i);
        Ok(i)
    }

    /// Resolves an endpoint on the given side of a transition.
    fn resolve(&mut self, e: End, start_side: bool, span: Span) -> Result<usize, Stop> {
        match e {
            End::Pseudo => self.pseudo_state(start_side, span),
            End::Ident { id, classes, span } => {
                let i = self.state_ref(&id, span)?;
                for c in classes {
                    self.ops.push(Op::Class {
                        id: id.clone(),
                        class: c,
                    });
                }
                Ok(i)
            }
        }
    }

    /// Reads one endpoint out of `raw`, whose first byte is at `at`. `[*]` is the scope's
    /// pseudo state; anything else is an id with any `:::role` shorthand split off.
    fn endpoint(&mut self, raw: &str, at: usize) -> Result<End, Stop> {
        let lead = raw.len() - raw.trim_start().len();
        let trimmed = raw.trim();
        let (s, e) = (at + lead, at + lead + trimmed.len());
        if trimmed.is_empty() {
            self.pos = at;
            return Err(self.fail(at, at, "expected a state id"));
        }
        if trimmed == "[*]" {
            return Ok(End::Pseudo);
        }
        let (head, classes) = match trimmed.find(":::") {
            Some(i) => {
                let mut names = Vec::new();
                for name in trimmed.get(i + 3..).unwrap_or("").split(":::") {
                    let name = name.trim();
                    if valid_class_name(name) {
                        names.push(name.to_string());
                    } else {
                        self.warn(
                            "W011",
                            s + i + 3,
                            e,
                            alloc::format!(
                                "class name `{}` is outside `[A-Za-z_][A-Za-z0-9_-]{{0,63}}`; dropped",
                                excerpt(name)
                            ),
                        );
                    }
                }
                (trimmed.get(..i).unwrap_or("").trim(), names)
            }
            None => (trimmed, Vec::new()),
        };
        if head.is_empty() || head.chars().any(char::is_whitespace) {
            return Err(self.fail(
                s,
                e,
                alloc::format!("expected a state id, found `{}`", excerpt(head)),
            ));
        }
        Ok(End::Ident {
            id: label::decode_entities(head),
            classes,
            span: self.span(s, e),
        })
    }

    // ------------------------------------------------------------ statements

    /// A direction token on the header line sets the diagram's direction; anything else
    /// is left for the first statement.
    fn header_direction(&mut self) {
        let at = self.skip_hws_from(self.pos);
        let mut end = at;
        while self
            .char_at(end)
            .is_some_and(|c| !c.is_whitespace() && c != ';')
        {
            end += self.char_at(end).map_or(1, char::len_utf8);
        }
        if end == at {
            return;
        }
        if let Some(d) = parse_direction(self.src.get(at..end).unwrap_or("")) {
            self.direction = d;
            self.pos = end;
        }
    }

    fn statements(&mut self) -> Result<(), Stop> {
        loop {
            loop {
                while self.char_at(self.pos).is_some_and(char::is_whitespace) {
                    self.pos += self.char_at(self.pos).map_or(1, char::len_utf8);
                }
                if self.char_at(self.pos) == Some(';') {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos >= self.src.len() {
                break;
            }
            let line_end = self.eol_from(self.pos);
            let line = self.src.get(self.pos..line_end).unwrap_or("");
            if super::cursor::at_line_start(self.src, self.pos)
                && super::repair::is_fence_line(line)
            {
                self.pos = super::strip_fence_line(self.idx, self.pos, self.diags);
                continue;
            }
            if self.rest_at(self.pos).starts_with("%%{") {
                self.pos =
                    super::directive(self.idx, self.pos, &mut self.meta, self.opts, self.diags);
                continue;
            }
            if is_comment_start(line) {
                self.pos = line_end;
                continue;
            }
            if self.char_at(self.pos) == Some('}') {
                self.close_brace();
                continue;
            }
            let end = self.stmt_end(self.pos);
            if is_divider(cut_comment(self.src.get(self.pos..end).unwrap_or(""))) {
                self.divider(self.pos, end);
                continue;
            }
            if self.statement()? {
                self.end_statement()?;
            }
        }
        self.close_open_frames();
        Ok(())
    }

    /// After a statement: an optional comment, then a newline, `;`, `}` or the end of
    /// input. A `}` stays for the statement loop, which closes the composite state.
    fn end_statement(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        let eol = self.eol_from(self.pos);
        if is_comment_start(self.src.get(self.pos..eol).unwrap_or("")) {
            self.pos = eol;
        }
        if matches!(self.char_at(self.pos), None | Some('\n' | ';' | '}')) {
            Ok(())
        } else {
            Err(self.fail_here("expected `;` or a newline after the statement"))
        }
    }

    /// Whether the keyword ending at `word_end` applies: it ends the statement or is
    /// followed by whitespace and no arrow, so `state --> B` stays a transition.
    fn keyword_applies(&self, word_end: usize) -> bool {
        match self.char_at(word_end) {
            None | Some('\n' | ';' | '}') => true,
            Some(' ' | '\t' | '\r') => arrow_at(self.src, self.skip_hws_from(word_end)).is_none(),
            _ => false,
        }
    }

    /// Reads one statement. The result is whether [`Self::end_statement`] applies: a
    /// statement that opens a composite state leaves the cursor inside its body.
    fn statement(&mut self) -> Result<bool, Stop> {
        let start = self.pos;
        let word_end = self.word_end(start);
        let word = self
            .src
            .get(start..word_end)
            .unwrap_or("")
            .to_ascii_lowercase();
        let next = self.char_at(self.skip_hws_from(word_end));
        let applies = word_end > start && self.keyword_applies(word_end);
        match word.as_str() {
            "state" if applies => {
                self.pos = word_end;
                self.state_stmt(start)
            }
            "direction" if applies => {
                self.pos = word_end;
                self.direction_stmt().map(|()| true)
            }
            "note" if applies => {
                self.pos = word_end;
                self.note_stmt(start).map(|()| true)
            }
            "classdef" if applies => {
                self.pos = word_end;
                self.class_def().map(|()| true)
            }
            "class" if applies => {
                self.pos = word_end;
                self.class_stmt().map(|()| true)
            }
            "style" if applies => {
                self.pos = word_end;
                self.style_stmt().map(|()| true)
            }
            "click" if applies => {
                self.pos = word_end;
                self.click().map(|()| true)
            }
            "hide" | "scale" if applies => {
                self.pos = word_end;
                self.dropped_stmt(start, &word);
                Ok(true)
            }
            "title" if applies || next == Some(':') => {
                self.pos = word_end;
                self.title_stmt();
                Ok(true)
            }
            "acctitle" if next == Some(':') => {
                self.pos = word_end;
                self.acc_line(true);
                Ok(true)
            }
            "accdescr" if next == Some(':') => {
                self.pos = word_end;
                self.acc_line(false);
                Ok(true)
            }
            "accdescr" if next == Some('{') => {
                self.pos = word_end;
                self.acc_block().map(|()| true)
            }
            _ => self.transition_stmt(start).map(|()| true),
        }
    }

    // ------------------------------------------------------------ transitions and descriptions

    /// `a --> b`, `a --> b : text`, `a : description` and a bare `a`
    /// (specs/state.md#transitions).
    fn transition_stmt(&mut self, start: usize) -> Result<(), Stop> {
        let stop = self.stmt_end(start);
        let text = cut_comment(self.src.get(start..stop).unwrap_or(""));
        let end = start + text.len();
        let colon = label_colon(text);
        let limit = colon.unwrap_or(text.len());
        let arrow = (0..limit).find_map(|i| {
            if self.src.is_char_boundary(start + i) {
                arrow_at(self.src, start + i).map(|t| (i, t))
            } else {
                None
            }
        });
        let Some((at, token)) = arrow else {
            self.pos = end;
            return match colon {
                Some(c) => self.describe(start, c, end, text),
                None => self.declare_bare(start, end, text),
            };
        };
        if token != CANONICAL {
            let span = self.span(start + at, start + at + token.len());
            self.repair(
                "R018",
                span,
                alloc::format!("`{token}` is read as `{CANONICAL}`: the grammar defines one arrow"),
                Fix {
                    span,
                    replacement: String::from(CANONICAL),
                },
            );
        }
        let from_raw = text.get(..at).unwrap_or("");
        let from_end = self.endpoint(from_raw, start)?;
        let after = at + token.len();
        // `s1 --> s2 done`: without a `:`, the first word is the target and `R017`
        // inserts the colon before the text.
        let (to_start, to_end, label_at) = match colon {
            Some(c) => (after, c, Some(c + 1)),
            None => {
                let tail = text.get(after..).unwrap_or("");
                let lead = tail.len() - tail.trim_start().len();
                let word = tail
                    .get(lead..)
                    .and_then(|t| t.find(char::is_whitespace))
                    .map_or(text.len(), |i| after + lead + i);
                if word < text.len() && text.get(word..).unwrap_or("").trim().is_empty() {
                    (after, text.len(), None)
                } else if word < text.len() {
                    let span = self.stmt_span(start, end);
                    self.repair(
                        "R017",
                        span,
                        String::from("transition text without its `:`; inserted"),
                        Fix {
                            span: self.span(start + word, start + word),
                            replacement: String::from(":"),
                        },
                    );
                    (after, word, Some(word))
                } else {
                    (after, text.len(), None)
                }
            }
        };
        let to_raw = text.get(to_start..to_end).unwrap_or("");
        let to_end_ep = self.endpoint(to_raw, start + to_start)?;
        let span = self.stmt_span(start, end);
        let from = self.resolve(from_end, true, span)?;
        let to = self.resolve(to_end_ep, false, span)?;
        if self.transitions.len() >= self.opts.limits.edges {
            return Err(Stop::TooLarge("transitions"));
        }
        let label = label_at.map(|at| {
            let raw = text.get(at..).unwrap_or("");
            self.finish_label(raw, start + at, end)
        });
        self.transitions.push(Transition {
            from,
            to,
            label,
            span,
        });
        self.pos = end;
        Ok(())
    }

    /// `a : description` (specs/state.md#states).
    fn describe(&mut self, start: usize, colon: usize, end: usize, text: &str) -> Result<(), Stop> {
        let head = text.get(..colon).unwrap_or("");
        let e = self.endpoint(head, start)?;
        let span = self.stmt_span(start, end);
        let i = self.resolve(e, true, span)?;
        let raw = text.get(colon + 1..).unwrap_or("");
        let label = self.finish_label(raw, start + colon + 1, end);
        self.add_description(i, label);
        if let Some(s) = self.states.get_mut(i) {
            s.implicit = false;
            s.span = span;
        }
        Ok(())
    }

    /// Adds one description to a state: the first replaces the id the state is named by,
    /// and every later one adds a line, as mermaid's description list does
    /// (specs/state.md#states).
    fn add_description(&mut self, i: usize, text: String) {
        if let Some(s) = self.states.get_mut(i) {
            if s.described {
                s.label.push('\n');
                s.label.push_str(&text);
            } else {
                s.label = text;
                s.described = true;
            }
        }
    }

    /// A bare `a` or `a:::role`, which declares the state.
    fn declare_bare(&mut self, start: usize, end: usize, text: &str) -> Result<(), Stop> {
        let e = self.endpoint(text, start)?;
        if matches!(e, End::Pseudo) {
            return Err(self.fail(start, end, "`[*]` names a state only inside a transition"));
        }
        let span = self.stmt_span(start, end);
        let i = self.resolve(e, true, span)?;
        if let Some(s) = self.states.get_mut(i) {
            s.implicit = false;
        }
        Ok(())
    }

    // ------------------------------------------------------------ state declarations

    /// `state <id>`, `state "<description>" as <id>`, `state <id> <<choice>>`, any of
    /// them followed by `: <description>` or by `{` (specs/state.md#states). Text the
    /// grammar has no place for is dropped with `W024`, as mermaid's lexer drops it.
    fn state_stmt(&mut self, kw_start: usize) -> Result<bool, Stop> {
        self.skip_hws();
        let mut described: Option<(usize, usize)> = None;
        if label::is_quote_start(self.char_at(self.pos).unwrap_or(' ')) {
            match label::scan_quoted(self.src, self.pos) {
                QuoteScan::Found(q) => {
                    self.typographic(&q.typographic);
                    described = Some((q.content_start, q.content_end));
                    self.pos = q.end;
                }
                _ => return Err(self.fail_here("expected a closing quote after the description")),
            }
            self.skip_hws();
            let we = self.word_end(self.pos);
            if !self
                .src
                .get(self.pos..we)
                .unwrap_or("")
                .eq_ignore_ascii_case("as")
            {
                return Err(self.fail_here("expected `as` after the description"));
            }
            self.pos = we;
            self.skip_hws();
        }
        let id_start = self.pos;
        while self
            .char_at(self.pos)
            .is_some_and(|c| !c.is_whitespace() && !matches!(c, ';' | '{' | '}' | '<' | '['))
        {
            // A single `:` ends the id and opens a description; `:::` is the role
            // shorthand and belongs to the endpoint.
            if self.char_at(self.pos) == Some(':') {
                if !self.rest_at(self.pos).starts_with(":::") {
                    break;
                }
                self.pos += 3;
                continue;
            }
            self.pos += self.char_at(self.pos).map_or(1, char::len_utf8);
        }
        let id_end = self.pos;
        if id_end == id_start {
            return Err(self.fail_here("expected a state id"));
        }
        let ep = self.endpoint(self.src.get(id_start..id_end).unwrap_or(""), id_start)?;
        let End::Ident { id, classes, .. } = ep else {
            return Err(self.fail(id_start, id_end, "`[*]` is not a state id"));
        };
        self.skip_hws();
        let kind = self.state_marker();
        self.skip_hws();
        // `{` opens the body where it stands or on a later line: mermaid's lexer reads
        // the two as one statement, and the corpus writes both.
        if self.char_at(self.pos) != Some('{') {
            let mut at = self.pos;
            while self.char_at(at).is_some_and(char::is_whitespace) {
                at += self.char_at(at).map_or(1, char::len_utf8);
            }
            if self.char_at(at) == Some('{') {
                self.pos = at;
            }
        }
        let composite = self.char_at(self.pos) == Some('{');
        // `state <id> : <description>`, the alias form's description repeated on the
        // same line, and any other text the grammar has no place for.
        let mut trailing: Option<(usize, usize)> = None;
        if !composite {
            // A description, and any other text the grammar has no place for, ends where
            // every statement ends: at the first `;` or unmatched `}`. `state A; B --> C`
            // is two statements, and the `}` of `state Outer { state A }` closes `Outer`
            // rather than trailing its member.
            let stop = self.stmt_end(self.pos);
            let rest = cut_comment(self.src.get(self.pos..stop).unwrap_or(""));
            if !rest.trim().is_empty() {
                trailing = Some((self.pos, self.pos + rest.len()));
                self.pos += rest.len();
            }
        }
        let stmt_end = if composite { self.pos + 1 } else { self.pos };
        let span = self.stmt_span(kw_start, stmt_end);
        let i = self.state_ref(&id, span)?;
        for c in classes {
            self.ops.push(Op::Class {
                id: id.clone(),
                class: c,
            });
        }
        if let Some((s, e)) = described {
            let label = self.finish_label(self.src.get(s..e).unwrap_or(""), s, e);
            self.add_description(i, label);
        }
        if let Some((s, e)) = trailing {
            match self.src.get(s..e).unwrap_or("").strip_prefix(':') {
                Some(raw) => {
                    let label = self.finish_label(raw, s + 1, e);
                    self.add_description(i, label);
                }
                None => self.warn(
                    "W024",
                    s,
                    e,
                    alloc::format!(
                        "`{}` follows the state id and is dropped",
                        excerpt(self.src.get(s..e).unwrap_or("").trim())
                    ),
                ),
            }
        }
        if let Some(st) = self.states.get_mut(i) {
            st.implicit = false;
            st.span = span;
            if let Some(k) = kind {
                st.kind = k;
            }
        }
        if !composite {
            return Ok(true);
        }
        if self.stack.len() >= self.opts.limits.nesting {
            let span = self.span(kw_start, stmt_end);
            let n = self.opts.limits.nesting;
            self.diags.emit(
                Severity::Error,
                "E010",
                span,
                alloc::format!("composite states nest deeper than {n}"),
            );
            return Err(Stop::Failed);
        }
        if let Some(st) = self.states.get_mut(i) {
            st.kind = StateKind::Composite;
        }
        self.pos = stmt_end;
        self.scopes.push(Scope {
            owner: Some(i),
            region: Some(0),
            start: None,
            end: None,
        });
        let scope = self.scopes.len() - 1;
        self.stack.push(Frame {
            state: i,
            scopes: alloc::vec![scope],
            buckets: alloc::vec![Vec::new()],
            spans: alloc::vec![span],
            span,
        });
        Ok(false)
    }

    /// `<<choice>>`, `<<fork>>`, `<<join>>` and mermaid's `[[…]]` spelling. A marker
    /// naming anything else leaves the state `Simple` with `W025`.
    fn state_marker(&mut self) -> Option<StateKind> {
        let (open, close) = if self.rest_at(self.pos).starts_with("<<") {
            ("<<", ">>")
        } else if self.rest_at(self.pos).starts_with("[[") {
            ("[[", "]]")
        } else {
            return None;
        };
        let start = self.pos;
        let body_start = start + open.len();
        let limit = self.stmt_end(body_start).max(body_start);
        let body = self.src.get(body_start..limit).unwrap_or("");
        let (name, end) = match body.find(close) {
            Some(i) => (body.get(..i).unwrap_or("").trim(), body_start + i + 2),
            None => (body.trim(), limit),
        };
        self.pos = end;
        match name.to_ascii_lowercase().as_str() {
            "choice" => Some(StateKind::Choice),
            "fork" => Some(StateKind::Fork),
            "join" => Some(StateKind::Join),
            _ => {
                self.warn(
                    "W025",
                    start,
                    end,
                    alloc::format!(
                        "`{}{}{}` names neither `choice`, `fork` nor `join`; the state stays plain",
                        open,
                        excerpt(name),
                        close
                    ),
                );
                None
            }
        }
    }

    // ------------------------------------------------------------ composite frames

    /// `}`: closes the innermost composite state, or `R015` when none is open.
    fn close_brace(&mut self) {
        let start = self.pos;
        let end = start + 1;
        self.pos = end;
        let Some(frame) = self.stack.pop() else {
            let span = self.stmt_span(start, end);
            let del_end = self.delete_to(start, end);
            self.repair(
                "R015",
                span,
                String::from("`}` with no open composite state; dropped"),
                Fix {
                    span: self.span(start, del_end),
                    replacement: String::new(),
                },
            );
            return;
        };
        self.close_frame(frame);
    }

    /// `--`: a new concurrency region of the innermost composite state, or `R016` when
    /// none is open (specs/state.md#concurrency).
    fn divider(&mut self, start: usize, end: usize) {
        self.pos = end;
        let span = self.stmt_span(start, end);
        let Some(&state) = self.stack.last().map(|f| &f.state) else {
            let del_end = self.delete_to(start, end);
            self.repair(
                "R016",
                span,
                String::from("`--` outside a composite state; dropped"),
                Fix {
                    span: self.span(start, del_end),
                    replacement: String::new(),
                },
            );
            return;
        };
        let index = self.stack.last().map_or(0, |f| f.buckets.len());
        self.scopes.push(Scope {
            owner: Some(state),
            region: Some(index),
            start: None,
            end: None,
        });
        let scope = self.scopes.len() - 1;
        if let Some(f) = self.stack.last_mut() {
            f.scopes.push(scope);
            f.buckets.push(Vec::new());
            f.spans.push(span);
        }
    }

    /// Attaches a closed frame's members to its composite state. `k` dividers make
    /// `k + 1` buckets; an empty one is dropped, and a composite left with fewer than two
    /// keeps its members directly (specs/state.md#concurrency).
    fn close_frame(&mut self, frame: Frame) {
        let members: Vec<usize> = frame.buckets.iter().flatten().copied().collect();
        if let Some(s) = self.states.get_mut(frame.state) {
            s.children.extend(members);
        }
        let filled: Vec<usize> = (0..frame.buckets.len())
            .filter(|&k| frame.buckets.get(k).is_some_and(|b| !b.is_empty()))
            .collect();
        if filled.len() < 2 {
            for &sc in &frame.scopes {
                if let Some(s) = self.scopes.get_mut(sc) {
                    s.region = None;
                }
            }
            return;
        }
        for (j, &k) in filled.iter().enumerate() {
            if let Some(s) = frame.scopes.get(k).and_then(|&sc| self.scopes.get_mut(sc)) {
                s.region = Some(j);
            }
        }
        let regions: Vec<RegionB> = filled
            .iter()
            .map(|&k| RegionB {
                states: frame.buckets.get(k).cloned().unwrap_or_default(),
                span: frame.spans.get(k).copied().unwrap_or_default(),
            })
            .collect();
        if let Some(s) = self.states.get_mut(frame.state) {
            s.regions.extend(regions);
        }
    }

    /// Closes every composite state left open at the end of input with `R014`, innermost
    /// first.
    fn close_open_frames(&mut self) {
        let insert_at = self.src.len();
        let text = if self.src.ends_with('\n') {
            "}\n"
        } else {
            "\n}"
        };
        while let Some(frame) = self.stack.pop() {
            let span = frame.span;
            let id = self
                .states
                .get(frame.state)
                .map(|s| excerpt(&s.id))
                .unwrap_or_default();
            self.repair(
                "R014",
                span,
                alloc::format!("composite state `{id}` has no `}}`; closed at end of input"),
                Fix {
                    span: self.span(insert_at, insert_at),
                    replacement: String::from(text),
                },
            );
            self.close_frame(frame);
        }
    }

    // ------------------------------------------------------------ notes

    /// `note left of <id>` and `note right of <id>`, with `: <text>` on the same line or
    /// lines up to an `end note` (specs/state.md#notes).
    fn note_stmt(&mut self, kw_start: usize) -> Result<(), Stop> {
        self.skip_hws();
        if label::is_quote_start(self.char_at(self.pos).unwrap_or(' ')) {
            let end = self.stmt_end(self.pos);
            self.warn(
                "W024",
                kw_start,
                end,
                "a floating note draws nothing; dropped",
            );
            self.pos = end;
            return Ok(());
        }
        let word_end = self.word_end(self.pos);
        let placement = match self
            .src
            .get(self.pos..word_end)
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "left" => NotePlacement::Before,
            "right" => NotePlacement::After,
            _ => return Err(self.fail_here("expected `left of` or `right of` after `note`")),
        };
        self.pos = word_end;
        self.skip_hws();
        let of_end = self.word_end(self.pos);
        if !self
            .src
            .get(self.pos..of_end)
            .unwrap_or("")
            .eq_ignore_ascii_case("of")
        {
            return Err(self.fail_here("expected `of` after `left` or `right`"));
        }
        self.pos = of_end;
        self.skip_hws();
        let id_start = self.pos;
        let line_end = self.stmt_end(id_start);
        let head = cut_comment(self.src.get(id_start..line_end).unwrap_or(""));
        let colon = head.find(':');
        let id_text = head.get(..colon.unwrap_or(head.len())).unwrap_or("");
        let ep = self.endpoint(id_text, id_start)?;
        let End::Ident { id, classes, span } = ep else {
            return Err(self.fail(id_start, line_end, "`[*]` takes no note"));
        };
        let state = self.state_ref(&id, span)?;
        for c in classes {
            self.ops.push(Op::Class {
                id: id.clone(),
                class: c,
            });
        }
        let (raw, text_start, text_end, stmt_end) = match colon {
            Some(c) => {
                let at = id_start + c + 1;
                let e = id_start + head.len();
                (
                    self.src.get(at..e).unwrap_or("").to_string(),
                    at,
                    e,
                    id_start + head.trim_end().len(),
                )
            }
            None => {
                let (body, consumed, unclosed) = self.note_block(line_end);
                if let Some(insert_at) = unclosed {
                    let span = self.span(kw_start, consumed);
                    self.repair(
                        "R019",
                        span,
                        String::from(
                            "block note has no `end note`; closed before the next statement",
                        ),
                        Fix {
                            span: self.span(insert_at, insert_at),
                            replacement: String::from("\nend note"),
                        },
                    );
                }
                (body, line_end, consumed, consumed)
            }
        };
        if self.notes.len() >= self.opts.limits.notes {
            return Err(Stop::TooLarge("notes"));
        }
        let text = self.finish_label(&raw, text_start, text_end);
        let span = self.span(kw_start, stmt_end);
        self.notes.push(Note {
            state,
            placement,
            text,
            span,
        });
        self.pos = stmt_end;
        Ok(())
    }

    /// The lines of a block note after `from`, up to its `end note`, the first line that
    /// can only be the next statement, or the end of input. Returns the joined text, the
    /// offset just past the last line read, and, when the block has no `end note`, where
    /// one belongs.
    fn note_block(&self, from: usize) -> (String, usize, Option<usize>) {
        let mut lines: Vec<&str> = Vec::new();
        let mut at = self.idx.next_line_start(from);
        let mut consumed = from;
        while at < self.src.len() {
            let next = self.idx.next_line_start(at);
            let line = self.src.get(at..next).unwrap_or("");
            let trimmed = line.trim();
            if trimmed.eq_ignore_ascii_case("end note") {
                return (lines.join("\n"), at + line.trim_end().len(), None);
            }
            if starts_a_statement(trimmed) {
                return (lines.join("\n"), consumed, Some(consumed));
            }
            consumed = at + line.trim_end().len();
            at = next;
            lines.push(trimmed);
        }
        let consumed = consumed.max(from);
        (lines.join("\n"), consumed, Some(consumed))
    }

    // ------------------------------------------------------------ direction

    fn direction_stmt(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        let start = self.pos;
        while self
            .char_at(self.pos)
            .is_some_and(|c| !c.is_whitespace() && c != ';')
        {
            self.pos += self.char_at(self.pos).map_or(1, char::len_utf8);
        }
        let tok = self.src.get(start..self.pos).unwrap_or("");
        let Some(d) = parse_direction(tok) else {
            let msg = alloc::format!(
                "expected a direction (TB, TD, BT, LR or RL), found `{}`",
                excerpt(tok)
            );
            return Err(self.fail(start, self.pos, msg));
        };
        match self.container() {
            Some(i) => {
                if let Some(s) = self.states.get_mut(i) {
                    s.direction = Some(d);
                }
            }
            None => self.direction = d,
        }
        Ok(())
    }

    // ------------------------------------------------------------ styles

    /// The style list running to the end of the statement, reporting rejected
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
            self.diags
                .emit_once(Severity::Info, "I030", span, style::FIXED_COLOUR_MESSAGE);
        }
        parsed.style
    }

    /// A comma-separated list of names, each checked against the `classDef` grammar.
    fn name_list(&mut self, what: &str) -> Result<Vec<String>, Stop> {
        self.skip_hws();
        let start = self.pos;
        while self
            .char_at(self.pos)
            .is_some_and(|c| !c.is_whitespace() && c != ';')
        {
            self.pos += self.char_at(self.pos).map_or(1, char::len_utf8);
        }
        if self.pos == start {
            return Err(self.fail_here(&alloc::format!("expected a class name after `{what}`")));
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
                    alloc::format!(
                        "class name `{}` is outside `[A-Za-z_][A-Za-z0-9_-]{{0,63}}`; dropped",
                        excerpt(name)
                    ),
                );
            }
        }
        Ok(valid)
    }

    fn class_def(&mut self) -> Result<(), Stop> {
        let names = self.name_list("classDef")?;
        let style = self.style_list();
        for name in names {
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

    /// `<id>(, <id>)*` for `class` and `style`.
    fn id_list(&mut self, after: &str) -> Result<Vec<(String, Span)>, Stop> {
        let mut ids = Vec::new();
        loop {
            self.skip_hws();
            let s = self.pos;
            let e = self.scan_id(s);
            if e == s {
                return Err(self.fail_here(&alloc::format!("expected a state id after `{after}`")));
            }
            ids.push((
                label::decode_entities(self.src.get(s..e).unwrap_or("")),
                self.span(s, e),
            ));
            self.pos = e;
            self.skip_hws();
            if self.char_at(self.pos) == Some(',') {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(ids)
    }

    fn class_stmt(&mut self) -> Result<(), Stop> {
        let ids = self.id_list("class")?;
        let names = self.name_list("the state ids")?;
        for name in names {
            for (id, _) in &ids {
                self.ops.push(Op::Class {
                    id: id.clone(),
                    class: name.clone(),
                });
            }
        }
        Ok(())
    }

    fn style_stmt(&mut self) -> Result<(), Stop> {
        let ids = self.id_list("style")?;
        let style = self.style_list();
        for (id, span) in ids {
            self.ops.push(Op::Style {
                id,
                span,
                style: style.clone(),
            });
        }
        Ok(())
    }

    // ------------------------------------------------------------ click

    /// One click argument: a quoted string (content range) or a bare word.
    fn click_token(&mut self) -> Option<(usize, usize, bool)> {
        self.skip_hws();
        if matches!(self.char_at(self.pos), None | Some('\n' | ';' | '}')) {
            return None;
        }
        if let QuoteScan::Found(q) = label::scan_quoted(self.src, self.pos) {
            self.typographic(&q.typographic);
            self.pos = q.end;
            return Some((q.content_start, q.content_end, true));
        }
        let s = self.pos;
        while self
            .char_at(self.pos)
            .is_some_and(|c| !c.is_whitespace() && c != ';')
        {
            self.pos += self.char_at(self.pos).map_or(1, char::len_utf8);
        }
        Some((s, self.pos, false))
    }

    /// `click <id> href "<url>"` and `click <id> "<url>" "<tooltip>"`
    /// (specs/state.md#links).
    fn click(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        let s = self.pos;
        let e = self.scan_id(s);
        if e == s {
            return Err(self.fail_here("expected a state id after `click`"));
        }
        let id = label::decode_entities(self.src.get(s..e).unwrap_or(""));
        self.pos = e;
        let Some((ts, te, quoted)) = self.click_token() else {
            return Err(self.fail_here("expected `href` or a URL after the state id"));
        };
        let word = self.src.get(ts..te).unwrap_or("");
        let url = if !quoted && word.eq_ignore_ascii_case("href") {
            match self.click_token() {
                Some(u) => u,
                None => return Err(self.fail_here("expected a URL after `href`")),
            }
        } else if quoted {
            (ts, te, true)
        } else {
            let span = self.span(s, self.pos);
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
            if q {
                self.warn(
                    "W024",
                    as_,
                    ae,
                    "a `click` tooltip is dropped; the SVG carries no page JavaScript",
                );
                continue;
            }
            match self.src.get(as_..ae).unwrap_or("") {
                "_blank" => target_blank = true,
                "_self" | "_parent" | "_top" => target_blank = false,
                other => {
                    return Err(self.fail(
                        as_,
                        ae,
                        alloc::format!(
                            "expected a tooltip or a link target (`_self`, `_blank`), found `{}`",
                            excerpt(other)
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
                "link URL is not relative, `https`, `http` or `mailto`; dropped",
            ),
        }
        Ok(())
    }

    // ------------------------------------------------------------ other statements

    /// `hide empty description` and `scale <n> width`: parsed and dropped
    /// (specs/state.md#accessibility-title-and-comments).
    fn dropped_stmt(&mut self, kw_start: usize, word: &str) {
        let end = self.stmt_end(self.pos);
        let reason = if word == "hide" {
            "`hide empty description` toggles a divider Merlion never draws; dropped"
        } else {
            "`scale … width` sets a fixed width; container fit owns the width; dropped"
        };
        self.warn("W024", kw_start, end, reason);
        self.pos = end;
    }

    /// `title <text>` and the legacy `title: <text>`.
    fn title_stmt(&mut self) {
        self.skip_hws();
        if self.char_at(self.pos) == Some(':') {
            self.pos += 1;
        }
        let start = self.pos;
        let end = self.stmt_end(start);
        let raw = cut_comment(self.src.get(start..end).unwrap_or("")).to_string();
        let text = self.finish_label(&raw, start, end);
        if !text.is_empty() {
            self.meta.title = Some(text);
        }
        self.pos = end;
    }

    /// `accTitle: text` / `accDescr: text`, to the end of the line.
    fn acc_line(&mut self, title: bool) {
        self.skip_hws();
        self.pos += 1; // `:`
        let start = self.pos;
        let end = self.eol_from(start);
        let mut text = self.src.get(start..end).unwrap_or("").trim().to_string();
        let max = self.opts.limits.label_bytes;
        if label::truncate(&mut text, max) {
            self.warn(
                "W012",
                start,
                end,
                alloc::format!("text longer than {max} bytes; truncated"),
            );
        }
        if title {
            self.meta.acc_title = Some(text);
        } else {
            self.meta.acc_descr = Some(text);
        }
        self.pos = end;
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
                alloc::format!("text longer than {max} bytes; truncated"),
            );
        }
        self.meta.acc_descr = Some(text);
        self.pos = close + 1;
        Ok(())
    }

    // ------------------------------------------------------------ model

    /// The id of a scope, which names the start and end states it owns.
    fn scope_name(&self, scope: usize) -> String {
        let Some(s) = self.scopes.get(scope) else {
            return String::from("root");
        };
        let Some(owner) = s.owner else {
            return String::from("root");
        };
        let id = self
            .states
            .get(owner)
            .map(|x| x.id.clone())
            .unwrap_or_default();
        match s.region {
            Some(k) => alloc::format!("{id}_r{k}"),
            None => id,
        }
    }

    fn finish(mut self) -> StateMachine {
        // `[*]` ids come last, so a generated id never shadows a declared one: a clash
        // appends `_` until the id is unused (specs/state.md#what-the-lowering-guarantees).
        let mut used: BTreeSet<String> = self
            .states
            .iter()
            .filter(|s| s.pseudo.is_none())
            .map(|s| s.id.clone())
            .collect();
        let pseudo: Vec<(usize, usize, bool)> = self
            .states
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.pseudo.map(|(scope, start)| (i, scope, start)))
            .collect();
        for (i, scope, start) in pseudo {
            let mut id = alloc::format!(
                "{}_{}",
                self.scope_name(scope),
                if start { "start" } else { "end" }
            );
            while used.contains(&id) {
                id.push('_');
            }
            used.insert(id.clone());
            if let Some(s) = self.states.get_mut(i) {
                s.id = id;
            }
        }
        let by_id: BTreeMap<String, usize> = self
            .states
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.clone(), i))
            .collect();

        // `class`, `:::`, `style` and `click`, in source order.
        let mut capped: BTreeSet<String> = BTreeSet::new();
        for op in core::mem::take(&mut self.ops) {
            match op {
                Op::Class { id, class } => {
                    let Some(s) = by_id.get(&id).and_then(|&i| self.states.get_mut(i)) else {
                        continue;
                    };
                    if s.classes.contains(&class) {
                        continue;
                    }
                    if s.classes.len() < MAX_CLASSES {
                        s.classes.push(class);
                    } else if capped.insert(id.clone()) {
                        self.diags.emit(
                            Severity::Warning,
                            "W020",
                            Span::default(),
                            alloc::format!(
                                "`{}` already has {MAX_CLASSES} classes; further classes dropped",
                                excerpt(&id)
                            ),
                        );
                    }
                }
                Op::Style { id, span, style } => {
                    match by_id.get(&id).and_then(|&i| self.states.get_mut(i)) {
                        Some(s) => s.style.merge(&style),
                        None => self.diags.emit(
                            Severity::Warning,
                            "W010",
                            span,
                            alloc::format!(
                                "style target `{}` is not a state; ignored",
                                excerpt(&id)
                            ),
                        ),
                    }
                }
                Op::Click { id, link } => {
                    if let Some(s) = by_id.get(&id).and_then(|&i| self.states.get_mut(i)) {
                        s.link = Some(link);
                    }
                }
            }
        }

        // Regions in declaration order, a parent before its children: states already
        // hold that order, so walking them emits the regions in it.
        let mut regions: Vec<Region> = Vec::new();
        let mut region_of: Vec<Option<usize>> = alloc::vec![None; self.states.len()];
        for i in 0..self.states.len() {
            let built = core::mem::take(&mut self.states[i].regions);
            for (index, r) in built.into_iter().enumerate() {
                let at = regions.len();
                for &m in &r.states {
                    if let Some(slot) = region_of.get_mut(m) {
                        *slot = Some(at);
                    }
                }
                regions.push(Region {
                    parent: i,
                    index,
                    states: r.states,
                    span: r.span,
                });
            }
        }

        let states: Vec<State> = self
            .states
            .into_iter()
            .enumerate()
            .map(|(i, s)| State {
                label: if s.kind.draws_label() {
                    s.label
                } else {
                    String::new()
                },
                id: s.id,
                kind: s.kind,
                parent: s.parent,
                region: region_of.get(i).copied().flatten(),
                children: s.children,
                direction: s.direction,
                classes: s.classes,
                style: s.style,
                link: s.link,
                implicit: s.implicit,
                span: s.span,
            })
            .collect();

        StateMachine {
            meta: self.meta,
            direction: self.direction,
            states,
            transitions: self.transitions,
            notes: self.notes,
            regions,
            class_defs: self.class_defs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_are_ordered_longest_first() {
        for w in ARROWS.windows(2) {
            assert!(w[0].len() >= w[1].len(), "{:?}", w[0]);
        }
        // No token is a prefix of a later one, so the first match is the longest.
        for (i, a) in ARROWS.iter().enumerate() {
            for b in ARROWS.iter().skip(i + 1) {
                assert!(!b.starts_with(a), "{a} shadows {b}");
            }
        }
        assert!(ARROWS.contains(&CANONICAL));
    }

    #[test]
    fn a_divider_is_dashes_and_nothing_else() {
        for t in ["--", "  ----  ", "---"] {
            assert!(is_divider(t), "{t:?}");
        }
        for t in ["-", "-->", "--x", "a--", ""] {
            assert!(!is_divider(t), "{t:?}");
        }
    }

    #[test]
    fn comments_cut_at_percent_and_at_a_bare_hash() {
        assert_eq!(cut_comment("A --> B %% tail"), "A --> B ");
        assert_eq!(cut_comment("A --> B # tail"), "A --> B ");
        assert_eq!(cut_comment("100#37; done"), "100#37; done");
        assert_eq!(cut_comment("50% off"), "50% off");
        assert_eq!(cut_comment("%% only"), "");
        assert_eq!(cut_comment("plain"), "plain");
    }

    #[test]
    fn the_label_colon_skips_the_role_shorthand() {
        assert_eq!(label_colon("A:::r --> B"), None);
        assert_eq!(label_colon("A --> B : x"), Some(8));
        assert_eq!(label_colon("A : x"), Some(2));
        assert_eq!(label_colon("A:::r --> B:::s : x"), Some(16));
    }
}
