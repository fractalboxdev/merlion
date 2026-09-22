//! The `sequenceDiagram` parser (specs/sequence.md#syntax), following the grammar
//! mermaid 12 documents: participant and `box` declarations, messages, notes,
//! activations, fragments, `autonumber`, the accessibility statements, and the four
//! statements the SVG never carries (`link`, `links`, `properties`, `details`).
//!
//! The parser is statement-oriented and never recurses: fragments and boxes nest on an
//! explicit stack bounded by `Limits::nesting` (`E010`), and items land in the innermost
//! open fragment section, so the model comes out as a tree with balanced sections. Every
//! scan is bounded by the end of the current statement, which ends at the first `;` that
//! does not close an entity code (`#59;`) or at the end of the line, so parsing stays
//! linear even on a single 1 MiB line.
//!
//! Diagnostics this module adds (specs/sequence.md#diagnostics): `W021` dropped
//! statement, `W022` rejected participant type, `W023` unbalanced activation, `R009`
//! unclosed fragment, `R010` section outside a fragment, `R011` unmatched `end`, `R012`
//! `destroy` without a message, `R013` message text without its `:`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::config::Value;
use super::cursor::LineIndex;
use super::directive::{self, JsonError};
use super::{label, style, ParseOptions, Stop};
use crate::diag::{excerpt, Diagnostic, Diagnostics, Fix, Severity, Span};
use crate::model::sequence::{
    Autonumber, Central, Fragment, FragmentKind, Head, Item, Message, MessageLine, Note,
    Participant, ParticipantBox, ParticipantKind, Placement, Section, Sequence,
};
use crate::model::{Color, Meta};

/// Every arrow token, longest first so the first match is the right one
/// (specs/sequence.md#messages): text, line, tail head, target head.
const ARROWS: &[(&str, MessageLine, Head, Head)] = &[
    ("<<-->>", MessageLine::Dotted, Head::Filled, Head::Filled),
    ("<<->>", MessageLine::Solid, Head::Filled, Head::Filled),
    ("-->>", MessageLine::Dotted, Head::None, Head::Filled),
    ("--|\\", MessageLine::Dotted, Head::None, Head::HalfTop),
    ("--|/", MessageLine::Dotted, Head::None, Head::HalfBottom),
    ("--\\\\", MessageLine::Dotted, Head::None, Head::StickTop),
    ("--//", MessageLine::Dotted, Head::None, Head::StickBottom),
    ("/|--", MessageLine::Dotted, Head::HalfTop, Head::None),
    ("\\|--", MessageLine::Dotted, Head::HalfBottom, Head::None),
    ("//--", MessageLine::Dotted, Head::StickTop, Head::None),
    ("\\\\--", MessageLine::Dotted, Head::StickBottom, Head::None),
    ("-->", MessageLine::Dotted, Head::None, Head::None),
    ("--x", MessageLine::Dotted, Head::None, Head::Cross),
    ("--)", MessageLine::Dotted, Head::None, Head::Open),
    ("->>", MessageLine::Solid, Head::None, Head::Filled),
    ("-|\\", MessageLine::Solid, Head::None, Head::HalfTop),
    ("-|/", MessageLine::Solid, Head::None, Head::HalfBottom),
    ("-\\\\", MessageLine::Solid, Head::None, Head::StickTop),
    ("-//", MessageLine::Solid, Head::None, Head::StickBottom),
    ("/|-", MessageLine::Solid, Head::HalfTop, Head::None),
    ("\\|-", MessageLine::Solid, Head::HalfBottom, Head::None),
    ("//-", MessageLine::Solid, Head::StickTop, Head::None),
    ("\\\\-", MessageLine::Solid, Head::StickBottom, Head::None),
    ("->", MessageLine::Solid, Head::None, Head::None),
    ("-x", MessageLine::Solid, Head::None, Head::Cross),
    ("-)", MessageLine::Solid, Head::None, Head::Open),
];

/// Longest `@{ … }` block scanned for its closing `}`: twice the directive parser's
/// own string limit, so every block that parser accepts is reachable while the scan
/// costs a statement a fixed amount whatever follows it.
const BRACE_SCAN_BYTES: usize = 2 * super::DIRECTIVE_STRING_BYTES;

/// The largest offset at or below `at` that starts a character.
fn floor_boundary(src: &str, at: usize) -> usize {
    let mut at = at.min(src.len());
    while at > 0 && !src.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The arrow token starting at `at`, if any.
fn arrow_at(src: &str, at: usize) -> Option<&'static (&'static str, MessageLine, Head, Head)> {
    let rest = src.get(at..)?;
    ARROWS.iter().find(|(t, ..)| rest.starts_with(t))
}

/// Length of the entity code starting just after a `#`, through its `;`
/// (`35;`, `x2665;`, `hearts;`), or `None` when the `#` opens a comment instead.
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

/// Cuts `text` at the `#` that opens a comment, keeping entity codes
/// (specs/sequence.md#accessibility-title-and-comments).
fn cut_comment(text: &str) -> &str {
    let mut i = 0usize;
    while let Some(j) = text.get(i..).and_then(|r| r.find('#')) {
        let at = i + j;
        match entity_len(text.get(at + 1..).unwrap_or("")) {
            Some(len) => i = at + 1 + len,
            None => return text.get(..at).unwrap_or(""),
        }
    }
    text
}

/// Whether the whole statement is a comment line: `%%` or a `#` that opens no entity.
fn is_comment_start(rest: &str) -> bool {
    rest.starts_with("%%") || (rest.starts_with('#') && cut_comment(rest).is_empty())
}

/// The leading colour of a `box` or `rect` header: an `rgb()`, `rgba()`, `hsl()` or
/// `hsla()` call, or a CSS named colour. `transparent` parses with no colour, which
/// lets a label that is itself a colour name through (specs/sequence.md#boxes).
fn leading_color(text: &str) -> Option<(usize, Option<Color>)> {
    let trimmed = text.trim_start();
    let off = text.len() - trimmed.len();
    let lower = trimmed.to_ascii_lowercase();
    for name in ["rgba", "rgb", "hsla", "hsl"] {
        if !lower.starts_with(name) {
            continue;
        }
        let close = trimmed.find(')')?;
        let call = trimmed.get(..close + 1)?;
        return style::parse_color(call).map(|c| (off + close + 1, Some(c)));
    }
    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let word = trimmed.get(..end)?;
    if word.eq_ignore_ascii_case("transparent") {
        return Some((off + end, None));
    }
    if style::is_named_color(word) {
        return style::parse_color(word).map(|c| (off + end, Some(c)));
    }
    None
}

/// `autonumber` values are hundredths of a unit, so a badge formats as an integer
/// (specs/sequence.md#autonumber). At most two decimals.
fn hundredths(s: &str) -> Option<i64> {
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (int_part, frac) = match body.split_once('.') {
        Some((a, b)) => (a, b),
        None => (body, ""),
    };
    if int_part.is_empty() && frac.is_empty() {
        return None;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if frac.len() > 2 {
        return None;
    }
    let whole: i64 = if int_part.is_empty() {
        0
    } else {
        int_part.parse().ok()?
    };
    let cents: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        _ => frac.parse().ok()?,
    };
    let v = whole.checked_mul(100)?.checked_add(cents)?;
    Some(if neg { -v } else { v })
}

/// An open `loop`, `alt`, …, `rect` or `box`.
struct Frame {
    kind: FrameKind,
    /// Byte offset of the opening keyword, for the fragment's span.
    start: usize,
    span: Span,
}

enum FrameKind {
    Fragment {
        kind: FragmentKind,
        /// Sections already closed by `else`, `and` or `option`.
        sections: Vec<Section>,
        /// Label and items of the section being read.
        label: String,
        items: Vec<Item>,
        span: Span,
    },
    Box {
        index: usize,
    },
}

impl FrameKind {
    fn word(&self) -> &'static str {
        match self {
            FrameKind::Fragment { kind, .. } => kind.as_str(),
            FrameKind::Box { .. } => "box",
        }
    }
}

struct P<'a, 'd> {
    src: &'a str,
    idx: &'a LineIndex<'a>,
    /// Offset of the last `}` in the source; a block opening at or after it closes
    /// nowhere, so [`P::brace_end`] answers without scanning.
    last_brace: usize,
    pos: usize,
    opts: &'a ParseOptions,
    diags: &'d mut Diagnostics,
    meta: Meta,
    participants: Vec<Participant>,
    /// Open activations per participant, for `W023`.
    open: Vec<u32>,
    boxes: Vec<ParticipantBox>,
    items: Vec<Item>,
    stack: Vec<Frame>,
    autonumber: Option<Autonumber>,
    messages: u32,
    /// Items that take a row: messages, notes, fragments and activations. `limits.edges`
    /// bounds them together, so a document cannot draw more rows than it declares
    /// messages (specs/sequence.md#diagnostics).
    rows: u32,
    /// `create`d participants waiting for the message that names them.
    creating: Vec<usize>,
    /// `destroy`ed participants waiting for the message that ends them, with the
    /// statement's span and the byte range `R012` deletes.
    destroying: Vec<(usize, Span, usize, usize)>,
}

/// Parses the body of a `sequenceDiagram` starting at `pos`, which is the offset just
/// after the header word. Repairs, warnings and infos go to `diags`.
pub(crate) fn parse_sequence(
    idx: &LineIndex,
    pos: usize,
    meta: Meta,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Result<Sequence, Stop> {
    let mut p = P {
        src: idx.src(),
        idx,
        last_brace: idx.src().rfind('}').unwrap_or(0),
        pos,
        opts,
        diags,
        meta,
        participants: Vec::new(),
        open: Vec::new(),
        boxes: Vec::new(),
        items: Vec::new(),
        stack: Vec::new(),
        autonumber: None,
        messages: 0,
        rows: 0,
        creating: Vec::new(),
        destroying: Vec::new(),
    };
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
        while matches!(
            self.src.as_bytes().get(self.pos),
            Some(b' ' | b'\t' | b'\r')
        ) {
            self.pos += 1;
        }
    }

    fn skip_hws_from(&self, mut at: usize) -> usize {
        while matches!(self.src.as_bytes().get(at), Some(b' ' | b'\t' | b'\r')) {
            at += 1;
        }
        at
    }

    /// Offset of the end of the line containing `at`, answered from the line index in
    /// amortised constant time so a statement never rescans its line.
    fn eol_from(&self, at: usize) -> usize {
        let next = self.idx.next_line_start(at);
        if next > at && self.src.as_bytes().get(next - 1) == Some(&b'\n') {
            next - 1
        } else {
            next
        }
    }

    /// End of the statement starting at `at`: the first `;` that closes no entity code,
    /// or the end of the line.
    fn stmt_end(&self, at: usize) -> usize {
        let eol = self.eol_from(at);
        let text = self.src.get(at..eol).unwrap_or("");
        let mut i = 0usize;
        while i < text.len() {
            match text.as_bytes().get(i) {
                Some(b';') => return at + i,
                Some(b'#') => match entity_len(text.get(i + 1..).unwrap_or("")) {
                    Some(len) => i += 1 + len,
                    None => i += 1,
                },
                _ => i += 1,
            }
        }
        eol
    }

    /// The run of id characters at `at`: what a keyword is read from.
    fn word_end(&self, at: usize) -> usize {
        let mut end = at;
        for (i, c) in self.rest_at(at).char_indices() {
            if c.is_alphanumeric() || c == '_' {
                end = at + i + c.len_utf8();
            } else {
                break;
            }
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
        let span = self.span(start, end);
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

    // ------------------------------------------------------------ statements

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
            self.statement()?;
            self.end_statement()?;
        }
        self.close_open_frames();
        Ok(())
    }

    /// After a statement: an optional comment, then a newline, `;` or the end of input.
    fn end_statement(&mut self) -> Result<(), Stop> {
        self.skip_hws();
        let eol = self.eol_from(self.pos);
        if is_comment_start(self.src.get(self.pos..eol).unwrap_or("")) {
            self.pos = eol;
        }
        if matches!(self.char_at(self.pos), None | Some('\n' | ';')) {
            Ok(())
        } else {
            Err(self.fail_here("expected `;` or a newline after the statement"))
        }
    }

    /// Whether the keyword ending at `word_end` applies: it ends the statement or is
    /// followed by whitespace and no arrow, so `loop->>B: hi` stays a message.
    fn keyword_applies(&self, word_end: usize) -> bool {
        match self.char_at(word_end) {
            None | Some('\n' | ';') => true,
            Some(' ' | '\t' | '\r') => arrow_at(self.src, self.skip_hws_from(word_end)).is_none(),
            _ => false,
        }
    }

    fn statement(&mut self) -> Result<(), Stop> {
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
            "participant" if applies => {
                self.pos = word_end;
                self.participant_stmt(start, Some(ParticipantKind::Participant), false)
            }
            "actor" if applies => {
                self.pos = word_end;
                self.participant_stmt(start, Some(ParticipantKind::Actor), false)
            }
            "create" if applies => {
                self.pos = word_end;
                self.create_stmt(start)
            }
            "destroy" if applies => {
                self.pos = word_end;
                self.destroy_stmt(start)
            }
            "box" if applies => {
                self.pos = word_end;
                self.box_stmt(start)
            }
            "end" if applies => {
                self.pos = word_end;
                self.end_frame(start, word_end);
                Ok(())
            }
            "loop" | "alt" | "opt" | "par" | "par_over" | "critical" | "break" | "rect"
                if applies =>
            {
                self.pos = word_end;
                self.fragment_stmt(start, &word)
            }
            "else" | "and" | "option" if applies => {
                self.pos = word_end;
                self.section_stmt(start, &word)
            }
            "autonumber" if applies => {
                self.pos = word_end;
                self.autonumber_stmt(start)
            }
            "activate" | "deactivate" if applies => {
                self.pos = word_end;
                self.activation_stmt(start, word == "activate")
            }
            "note" if applies => {
                self.pos = word_end;
                self.note_stmt(start)
            }
            "link" | "links" | "properties" | "details" if applies || next == Some(':') => {
                self.pos = word_end;
                self.dropped_stmt(start, &word);
                Ok(())
            }
            "title" if applies || next == Some(':') => {
                self.pos = word_end;
                self.title_stmt();
                Ok(())
            }
            "acctitle" if next == Some(':') => {
                self.pos = word_end;
                self.acc_line(true);
                Ok(())
            }
            "accdescr" if next == Some(':') => {
                self.pos = word_end;
                self.acc_line(false);
                Ok(())
            }
            "accdescr" if next == Some('{') => {
                self.pos = word_end;
                self.acc_block()
            }
            _ => self.message_stmt(start),
        }
    }

    // ------------------------------------------------------------ participants

    /// The participant with `id`, declared implicitly when it is new
    /// (specs/sequence.md#participants): naming one in a message is idiomatic, so it
    /// carries no diagnostic.
    fn participant(&mut self, raw: &str, start: usize, end: usize) -> Result<usize, Stop> {
        let id = label::decode_entities(raw.trim());
        if let Some(i) = self.participants.iter().position(|p| p.id == id) {
            return Ok(i);
        }
        if self.participants.len() >= self.opts.limits.nodes {
            return Err(Stop::TooLarge("participants"));
        }
        let label = self.finish_label(&id, start, end);
        self.participants.push(Participant {
            id,
            label,
            implicit: true,
            span: self.span(start, end),
            ..Participant::default()
        });
        self.open.push(0);
        Ok(self.participants.len() - 1)
    }

    /// `participant A`, `actor B`, either with `@{ … }` and `as <label>`.
    fn participant_stmt(
        &mut self,
        kw_start: usize,
        kind: Option<ParticipantKind>,
        created: bool,
    ) -> Result<(), Stop> {
        self.skip_hws();
        let id_start = self.pos;
        // `@{ … }` may hold a `;`, so the statement ends after the block; the `@{`
        // itself opens before the `;` that would otherwise end the statement, so the
        // search reads the statement and not the rest of the line.
        let scan_end = self.stmt_end(id_start);
        let at = self
            .src
            .get(id_start..scan_end)
            .and_then(|l| l.find("@{"))
            .map(|i| id_start + i);
        let (id_end, mut kind, mut alias, rest_start, stmt_end) = match at {
            Some(at) => {
                let (block_end, k, a) = self.at_block(at)?;
                (at, k.or(kind), a, block_end, self.stmt_end(block_end))
            }
            None => {
                let end = self.stmt_end(id_start);
                (end, kind, None, end, end)
            }
        };
        let id_text = cut_comment(self.src.get(id_start..id_end).unwrap_or(""));
        let tail = cut_comment(self.src.get(rest_start..stmt_end).unwrap_or(""));
        // `as <label>` runs to the end of the statement; an external alias wins over an
        // inline one.
        let (id_text, id_end) = match alias_split(id_text) {
            Some((head, label_start)) => {
                alias = Some((
                    id_start + label_start,
                    id_start + id_text.trim_end().len(),
                    id_text.get(label_start..).unwrap_or("").to_string(),
                ));
                (head, id_start + head.len())
            }
            None => (id_text, id_start + id_text.trim_end().len()),
        };
        if let Some((head, label_start)) = alias_split(tail) {
            let _ = head;
            alias = Some((
                rest_start + label_start,
                rest_start + tail.trim_end().len(),
                tail.get(label_start..).unwrap_or("").to_string(),
            ));
        }
        if id_text.trim().is_empty() {
            self.pos = id_start;
            return Err(self.fail_here("expected a participant id"));
        }
        let i = self.participant(id_text, id_start, id_end)?;
        if kind.is_none() {
            kind = Some(ParticipantKind::Participant);
        }
        let span = self.stmt_span(kw_start, stmt_end);
        let mut wrap = None;
        let label = alias.map(|(s, e, raw)| {
            let (w, text, at) = if let Some(r) = strip_prefix_ci(&raw, "wrap:") {
                (Some(true), r.to_string(), s + 5)
            } else if let Some(r) = strip_prefix_ci(&raw, "nowrap:") {
                (Some(false), r.to_string(), s + 7)
            } else {
                (None, raw, s)
            };
            wrap = w;
            self.finish_label(&text, at, e)
        });
        if let Some(p) = self.participants.get_mut(i) {
            p.implicit = false;
            p.span = span;
            if let Some(k) = kind {
                p.kind = k;
            }
            if let Some(l) = label {
                p.label = l;
            }
            if wrap.is_some() {
                p.wrap = wrap;
            }
        }
        if let Some(b) = self.open_box() {
            if let Some(bx) = self.boxes.get_mut(b) {
                if !bx.participants.contains(&i) {
                    bx.participants.push(i);
                }
            }
            if let Some(p) = self.participants.get_mut(i) {
                p.group = Some(b);
            }
        }
        if created {
            self.creating.push(i);
        }
        self.pos = stmt_end;
        Ok(())
    }

    /// `@{ "type": …, "alias": … }` at `at`: the offset after the block, the kind and
    /// the inline alias. A key, a value or a type outside the eight kinds is `W022`.
    #[allow(clippy::type_complexity)]
    fn at_block(
        &mut self,
        at: usize,
    ) -> Result<
        (
            usize,
            Option<ParticipantKind>,
            Option<(usize, usize, String)>,
        ),
        Stop,
    > {
        let body_start = at + 1; // `@`; the JSON value starts at `{`
        let Some(close) = self.brace_end(body_start) else {
            let eol = self.eol_from(at);
            self.warn("W022", at, eol, "`@{` is not closed by `}`; ignored");
            return Ok((eol, None, None));
        };
        let text = self.src.get(body_start..close).unwrap_or("");
        let parsed = directive::parse_value(
            text,
            body_start,
            self.opts.limits.nesting,
            super::DIRECTIVE_STRING_BYTES,
        );
        let entries = match parsed {
            Ok(Value::Map(entries)) => entries,
            Ok(_) => {
                self.warn(
                    "W022",
                    body_start,
                    close,
                    "`@{…}` expects a JSON object; ignored",
                );
                return Ok((close, None, None));
            }
            Err(JsonError::TooLarge {
                start,
                end,
                message,
            }) => {
                let span = self.span(start, end);
                self.diags.emit(Severity::Error, "E012", span, message);
                return Err(Stop::Failed);
            }
            Err(JsonError::Malformed { start, end, .. }) => {
                self.warn(
                    "W022",
                    start.min(body_start),
                    end.max(close),
                    "`@{…}` is not a JSON object; ignored",
                );
                return Ok((close, None, None));
            }
        };
        let mut kind = None;
        let mut alias = None;
        for e in entries {
            match (e.key.as_str(), &e.value) {
                ("type", Value::Str(v)) => {
                    match ParticipantKind::from_type_name(v.trim().to_ascii_lowercase().as_str()) {
                        Some(k) => kind = Some(k),
                        None => self.warn(
                            "W022",
                            e.start,
                            e.end,
                            alloc::format!(
                                "participant type `{}` is not one of the eight kinds; drawn plain",
                                excerpt(v)
                            ),
                        ),
                    }
                }
                ("alias", Value::Str(v)) => alias = Some((e.start, e.end, v.clone())),
                (key, _) => self.warn(
                    "W022",
                    e.start,
                    e.end,
                    alloc::format!("`@{{…}}` key `{}` is not supported; ignored", excerpt(key)),
                ),
            }
        }
        Ok((close, kind, alias))
    }

    /// The offset just past the `}` closing the block that opens at `at`, quotes
    /// honoured.
    ///
    /// The scan reads at most [`BRACE_SCAN_BYTES`] and stops at once past the last `}`
    /// in the source, so a statement whose block is never closed costs a fixed amount
    /// rather than the tail of the input.
    fn brace_end(&self, at: usize) -> Option<usize> {
        if at >= self.last_brace {
            return None;
        }
        let stop = floor_boundary(self.src, self.src.len().min(at + 1 + BRACE_SCAN_BYTES));
        let window = self.src.get(at + 1..stop).unwrap_or("");
        let mut quote: Option<char> = None;
        for (i, c) in window.char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '}') => return Some(at + 1 + i + 1),
                _ => {}
            }
        }
        None
    }

    /// `create participant B`, `create actor D as Donald`.
    fn create_stmt(&mut self, kw_start: usize) -> Result<(), Stop> {
        self.skip_hws();
        let word_end = self.word_end(self.pos);
        let word = self
            .src
            .get(self.pos..word_end)
            .unwrap_or("")
            .to_ascii_lowercase();
        let kind = match word.as_str() {
            "participant" => {
                self.pos = word_end;
                Some(ParticipantKind::Participant)
            }
            "actor" => {
                self.pos = word_end;
                Some(ParticipantKind::Actor)
            }
            _ => None,
        };
        self.participant_stmt(kw_start, kind, true)
    }

    /// `destroy X`: the lifeline ends at the next message naming `X` (`R012` otherwise).
    fn destroy_stmt(&mut self, kw_start: usize) -> Result<(), Stop> {
        self.skip_hws();
        let start = self.pos;
        let end = self.stmt_end(start);
        let raw = cut_comment(self.src.get(start..end).unwrap_or(""));
        if raw.trim().is_empty() {
            return Err(self.fail_here("expected a participant after `destroy`"));
        }
        let i = self.participant(raw, start, start + raw.trim_end().len())?;
        let span = self.stmt_span(kw_start, end);
        // `R012` deletes the whole statement, its line ending included.
        let del_end = self.delete_to(kw_start, end);
        self.destroying.push((i, span, kw_start, del_end));
        self.pos = end;
        Ok(())
    }

    // ------------------------------------------------------------ boxes

    /// `box <colour?> <label>`: a tinted rectangle behind the participants it holds.
    fn box_stmt(&mut self, kw_start: usize) -> Result<(), Stop> {
        let start = self.pos;
        let end = self.stmt_end(start);
        let text = cut_comment(self.src.get(start..end).unwrap_or(""));
        let (label_start, color) = match leading_color(text) {
            Some((n, c)) => (n, c),
            None => (0, None),
        };
        let raw = text.get(label_start..).unwrap_or("");
        let label = self.finish_label(raw, start + label_start, end);
        if color.is_some() {
            let span = self.stmt_span(kw_start, end);
            self.diags
                .emit_once(Severity::Info, "I030", span, style::FIXED_COLOUR_MESSAGE);
        }
        let span = self.stmt_span(kw_start, end);
        self.boxes.push(ParticipantBox {
            label,
            color,
            participants: Vec::new(),
            span,
        });
        let index = self.boxes.len() - 1;
        self.stack.push(Frame {
            kind: FrameKind::Box { index },
            start: kw_start,
            span,
        });
        self.pos = end;
        Ok(())
    }

    /// The innermost open `box`, which a participant declaration joins.
    fn open_box(&self) -> Option<usize> {
        self.stack.iter().rev().find_map(|f| match f.kind {
            FrameKind::Box { index } => Some(index),
            _ => None,
        })
    }

    // ------------------------------------------------------------ fragments

    fn fragment_stmt(&mut self, kw_start: usize, word: &str) -> Result<(), Stop> {
        self.charge_row()?;
        let start = self.pos;
        let end = self.stmt_end(start);
        let text = cut_comment(self.src.get(start..end).unwrap_or(""));
        let (kind, label_start) = if word == "rect" {
            // A colour is optional: `rect` alone takes the theme's cluster tint. A hex
            // colour is unavailable whichever way, because `#` opens a comment.
            match leading_color(text) {
                Some((n, c)) => (FragmentKind::Rect(Some(c.unwrap_or(Color::Transparent))), n),
                None => (FragmentKind::Rect(None), 0),
            }
        } else {
            let kind = match word {
                "loop" => FragmentKind::Loop,
                "alt" => FragmentKind::Alt,
                "opt" => FragmentKind::Opt,
                "par" => FragmentKind::Par,
                "par_over" => FragmentKind::ParOver,
                "critical" => FragmentKind::Critical,
                _ => FragmentKind::Break,
            };
            (kind, 0)
        };
        let depth = self
            .stack
            .iter()
            .filter(|f| matches!(f.kind, FrameKind::Fragment { .. }))
            .count();
        if depth >= self.opts.limits.nesting {
            let span = self.span(kw_start, end);
            self.diags.emit(
                Severity::Error,
                "E010",
                span,
                alloc::format!("fragments nest deeper than {}", self.opts.limits.nesting),
            );
            return Err(Stop::Failed);
        }
        if matches!(kind, FragmentKind::Rect(Some(_))) {
            let span = self.stmt_span(kw_start, end);
            self.diags
                .emit_once(Severity::Info, "I030", span, style::FIXED_COLOUR_MESSAGE);
        }
        let raw = text.get(label_start..).unwrap_or("");
        let label = self.finish_label(raw, start + label_start, end);
        let span = self.stmt_span(kw_start, end);
        self.stack.push(Frame {
            kind: FrameKind::Fragment {
                kind,
                sections: Vec::new(),
                label,
                items: Vec::new(),
                span,
            },
            start: kw_start,
            span,
        });
        self.pos = end;
        Ok(())
    }

    /// `else`, `and`, `option`: a new section of the innermost open fragment.
    fn section_stmt(&mut self, kw_start: usize, word: &str) -> Result<(), Stop> {
        let start = self.pos;
        let end = self.stmt_end(start);
        let raw = cut_comment(self.src.get(start..end).unwrap_or(""));
        let label = self.finish_label(raw, start, end);
        let span = self.stmt_span(kw_start, end);
        let at = self
            .stack
            .iter()
            .rposition(|f| matches!(f.kind, FrameKind::Fragment { .. }));
        match at.and_then(|i| self.stack.get_mut(i)) {
            Some(Frame {
                kind:
                    FrameKind::Fragment {
                        sections,
                        label: cur_label,
                        items,
                        span: cur_span,
                        ..
                    },
                ..
            }) => {
                sections.push(Section {
                    label: core::mem::take(cur_label),
                    items: core::mem::take(items),
                    span: *cur_span,
                });
                *cur_label = label;
                *cur_span = span;
            }
            _ => {
                let del_end = self.delete_to(kw_start, end);
                self.repair(
                    "R010",
                    span,
                    alloc::format!("`{word}` with no open fragment; dropped"),
                    Fix {
                        span: self.idx.span(kw_start, del_end),
                        replacement: String::new(),
                    },
                );
            }
        }
        self.pos = end;
        Ok(())
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

    /// `end`: closes the innermost fragment or box, or `R011` when none is open.
    fn end_frame(&mut self, kw_start: usize, word_end: usize) {
        let Some(frame) = self.stack.pop() else {
            let span = self.stmt_span(kw_start, word_end);
            let del_end = self.delete_to(kw_start, word_end);
            self.repair(
                "R011",
                span,
                String::from("`end` with no open fragment or box; dropped"),
                Fix {
                    span: self.idx.span(kw_start, del_end),
                    replacement: String::new(),
                },
            );
            return;
        };
        self.close_frame(frame, word_end);
        self.pos = word_end;
    }

    fn close_frame(&mut self, frame: Frame, end: usize) {
        let start = frame.start;
        match frame.kind {
            FrameKind::Fragment {
                kind,
                mut sections,
                label,
                items,
                span,
            } => {
                sections.push(Section { label, items, span });
                let fragment = Fragment {
                    kind,
                    sections,
                    span: self.stmt_span(start, end),
                };
                self.push_item(Item::Fragment(fragment));
            }
            FrameKind::Box { index } => {
                if let Some(b) = self.boxes.get_mut(index) {
                    b.span = self.idx.span(start, end);
                }
            }
        }
    }

    /// Closes every frame left open at the end of input with `R009`, innermost first.
    fn close_open_frames(&mut self) {
        let insert_at = self.src.len();
        let text = if self.src.ends_with('\n') {
            "end\n"
        } else {
            "\nend"
        };
        while let Some(frame) = self.stack.pop() {
            let span = frame.span;
            let word = frame.kind.word();
            self.repair(
                "R009",
                span,
                alloc::format!("`{word}` has no matching `end`; closed at end of input"),
                Fix {
                    span: self.idx.span(insert_at, insert_at),
                    replacement: String::from(text),
                },
            );
            self.close_frame(frame, insert_at);
        }
    }

    /// Opens one activation on `p`, or reports that `limits.nesting` is reached. Each
    /// level offsets the drawn bar, so an uncapped stack widens the drawing without
    /// bound (specs/sequence.md#activation).
    fn open_activation(&mut self, p: usize) -> bool {
        let depth = self.open.get(p).copied().unwrap_or(0) as usize;
        if depth >= self.opts.limits.nesting {
            return true;
        }
        if let Some(n) = self.open.get_mut(p) {
            *n += 1;
        }
        false
    }

    /// Counts one row-producing item against `limits.edges`.
    fn charge_row(&mut self) -> Result<(), Stop> {
        self.rows = self.rows.saturating_add(1);
        if self.rows as usize > self.opts.limits.edges {
            return Err(Stop::TooLarge("items"));
        }
        Ok(())
    }

    /// Adds an item to the innermost open fragment section, else to the diagram.
    fn push_item(&mut self, item: Item) {
        for f in self.stack.iter_mut().rev() {
            if let FrameKind::Fragment { items, .. } = &mut f.kind {
                items.push(item);
                return;
            }
        }
        self.items.push(item);
    }

    // ------------------------------------------------------------ messages

    /// `[participant][arrow][participant]: text`, with `+` / `-` activation and `()`
    /// central-connection markers.
    fn message_stmt(&mut self, start: usize) -> Result<(), Stop> {
        let end = self.stmt_end(start);
        let text = self.src.get(start..end).unwrap_or("");
        // The arrow is the leftmost token before the `:` that marks the text.
        let limit = text.find(':').map_or(text.len(), |i| i);
        let Some((at, arrow)) = (start..start + limit).find_map(|i| {
            if self.src.is_char_boundary(i) {
                arrow_at(self.src, i).map(|a| (i, a))
            } else {
                None
            }
        }) else {
            self.pos = start;
            return Err(self.fail(
                start,
                end,
                "expected a message such as `A->>B: text`, a keyword or a comment",
            ));
        };
        let (_, line, tail, head) = *arrow;
        let from_raw = self.src.get(start..at).unwrap_or("");
        if from_raw.trim().is_empty() {
            return Err(self.fail(start, end, "expected a participant before the arrow"));
        }
        let mut central = Central::None;
        let from_text = match from_raw.trim_end().strip_suffix("()") {
            Some(t) => {
                central = Central::Source;
                t
            }
            None => from_raw,
        };
        let from = self.participant(from_text, start, start + from_text.trim_end().len())?;

        let mut cur = at + arrow.0.len();
        let (activate, deactivate) = match self.char_at(cur) {
            Some('+') => {
                cur += 1;
                (true, false)
            }
            Some('-') => {
                cur += 1;
                (false, true)
            }
            _ => (false, false),
        };
        if self.rest_at(cur).starts_with("()") {
            cur += 2;
            central = match central {
                Central::Source => Central::Both,
                _ => Central::Target,
            };
        }
        // `Alice->>Bob Hello`: without a `:`, the first word is the target and `R013`
        // inserts the colon before the text.
        let colon = self
            .src
            .get(cur..end)
            .and_then(|t| t.find(':'))
            .map(|i| cur + i);
        let (to_start, to_end, label_start) = match colon {
            Some(c) => (cur, c, Some(c + 1)),
            None => {
                let tail_text = self.src.get(cur..end).unwrap_or("");
                let lead = tail_text.len() - tail_text.trim_start().len();
                let word_end = tail_text
                    .get(lead..)
                    .and_then(|t| t.find(char::is_whitespace))
                    .map_or(end, |i| cur + lead + i);
                if word_end < end && !self.src.get(word_end..end).unwrap_or("").trim().is_empty() {
                    let span = self.stmt_span(start, end);
                    self.repair(
                        "R013",
                        span,
                        String::from("message text without its `:`; inserted"),
                        Fix {
                            span: self.idx.span(word_end, word_end),
                            replacement: String::from(":"),
                        },
                    );
                    (cur, word_end, Some(word_end))
                } else {
                    (cur, end, None)
                }
            }
        };
        let to_text = self.src.get(to_start..to_end).unwrap_or("");
        if to_text.trim().is_empty() {
            return Err(self.fail(start, end, "expected a participant after the arrow"));
        }
        let to = self.participant(to_text, to_start, to_start + to_text.trim_end().len())?;

        let (wrap, label) = match label_start {
            None => (None, String::new()),
            Some(at) => {
                let raw = self.src.get(at..end).unwrap_or("");
                let (wrap, raw, at) = if let Some(r) = strip_prefix_ci(raw, "wrap:") {
                    (Some(true), r, at + 5)
                } else if let Some(r) = strip_prefix_ci(raw, "nowrap:") {
                    (Some(false), r, at + 7)
                } else {
                    (None, raw, at)
                };
                (wrap, self.finish_label(raw, at, end))
            }
        };

        if self.messages as usize >= self.opts.limits.edges {
            return Err(Stop::TooLarge("messages"));
        }
        self.charge_row()?;
        let index = self.messages;
        self.messages += 1;
        let mut deactivate = deactivate;
        let mut activate = activate;
        if activate && self.open_activation(to) {
            self.warn(
                "W023",
                start,
                end,
                alloc::format!(
                    "activations nested deeper than {}; the bar is dropped",
                    self.opts.limits.nesting
                ),
            );
            activate = false;
        }
        if deactivate {
            match self.open.get_mut(from) {
                Some(n) if *n > 0 => *n -= 1,
                _ => {
                    deactivate = false;
                    let span = self.stmt_span(start, end);
                    self.diags.emit(
                        Severity::Warning,
                        "W023",
                        span,
                        "`-` closes an activation that is not open; ignored",
                    );
                }
            }
        }
        self.resolve_lifelines(index, from, to);
        let span = self.stmt_span(start, end);
        self.push_item(Item::Message(Message {
            index,
            from,
            to,
            label,
            line,
            head,
            tail,
            activate,
            deactivate,
            central,
            wrap,
            span,
        }));
        self.pos = end;
        Ok(())
    }

    /// `create` starts a lifeline at the message that targets it; `destroy` ends one at
    /// the message naming either of its ends (specs/sequence.md#participants).
    fn resolve_lifelines(&mut self, index: u32, from: usize, to: usize) {
        if let Some(at) = self.creating.iter().position(|&i| i == to) {
            self.creating.remove(at);
            if let Some(p) = self.participants.get_mut(to) {
                p.created_by = Some(index);
            }
        }
        if let Some(at) = self
            .destroying
            .iter()
            .position(|&(i, ..)| i == from || i == to)
        {
            let (i, ..) = self.destroying.remove(at);
            if let Some(p) = self.participants.get_mut(i) {
                p.destroyed_by = Some(index);
            }
        }
    }

    // ------------------------------------------------------------ notes and activation

    /// `Note left of A: …`, `Note right of A: …`, `Note over A[,B]: …`.
    fn note_stmt(&mut self, kw_start: usize) -> Result<(), Stop> {
        self.charge_row()?;
        self.skip_hws();
        let word_end = self.word_end(self.pos);
        let word = self
            .src
            .get(self.pos..word_end)
            .unwrap_or("")
            .to_ascii_lowercase();
        let placement = match word.as_str() {
            "left" => Placement::LeftOf,
            "right" => Placement::RightOf,
            "over" => Placement::Over,
            _ => {
                return Err(self.fail_here("expected `left of`, `right of` or `over` after `Note`"))
            }
        };
        self.pos = word_end;
        if placement != Placement::Over {
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
        }
        self.skip_hws();
        let list_start = self.pos;
        let end = self.stmt_end(list_start);
        let Some(colon) = self
            .src
            .get(list_start..end)
            .and_then(|t| t.find(':'))
            .map(|i| list_start + i)
        else {
            return Err(self.fail(kw_start, end, "expected `:` and the note text"));
        };
        let list = self.src.get(list_start..colon).unwrap_or("");
        let mut ends = Vec::new();
        let mut at = list_start;
        for part in list.split(',') {
            let p_start = at;
            at += part.len() + 1;
            if part.trim().is_empty() {
                continue;
            }
            let i = self.participant(part, p_start, p_start + part.trim_end().len())?;
            ends.push(i);
        }
        let Some(&first) = ends.first() else {
            return Err(self.fail(kw_start, end, "expected a participant in the note"));
        };
        let last = if placement == Placement::Over {
            ends.get(1).copied().unwrap_or(first)
        } else {
            first
        };
        let raw = self.src.get(colon + 1..end).unwrap_or("");
        let (wrap, raw, at) = if let Some(r) = strip_prefix_ci(raw, "wrap:") {
            (Some(true), r, colon + 6)
        } else if let Some(r) = strip_prefix_ci(raw, "nowrap:") {
            (Some(false), r, colon + 8)
        } else {
            (None, raw, colon + 1)
        };
        let text = self.finish_label(raw, at, end);
        let span = self.stmt_span(kw_start, end);
        self.push_item(Item::Note(Note {
            placement,
            from: first,
            to: last,
            text,
            wrap,
            span,
        }));
        self.pos = end;
        Ok(())
    }

    /// `activate X` / `deactivate X`; a `deactivate` with nothing open is `W023`.
    fn activation_stmt(&mut self, kw_start: usize, activate: bool) -> Result<(), Stop> {
        self.skip_hws();
        let start = self.pos;
        let end = self.stmt_end(start);
        let raw = cut_comment(self.src.get(start..end).unwrap_or(""));
        if raw.trim().is_empty() {
            return Err(self.fail_here("expected a participant"));
        }
        let i = self.participant(raw, start, start + raw.trim_end().len())?;
        let span = self.stmt_span(kw_start, end);
        self.pos = end;
        if activate {
            if self.open_activation(i) {
                self.diags.emit(
                    Severity::Warning,
                    "W023",
                    span,
                    alloc::format!(
                        "activations nested deeper than {}; the bar is dropped",
                        self.opts.limits.nesting
                    ),
                );
                return Ok(());
            }
            self.charge_row()?;
            self.push_item(Item::Activate {
                participant: i,
                span,
            });
            return Ok(());
        }
        match self.open.get_mut(i) {
            Some(n) if *n > 0 => {
                *n -= 1;
                self.charge_row()?;
                self.push_item(Item::Deactivate {
                    participant: i,
                    span,
                });
            }
            _ => self.diags.emit(
                Severity::Warning,
                "W023",
                span,
                "`deactivate` with no open activation; dropped",
            ),
        }
        Ok(())
    }

    // ------------------------------------------------------------ other statements

    fn autonumber_stmt(&mut self, kw_start: usize) -> Result<(), Stop> {
        let start = self.pos;
        let end = self.stmt_end(start);
        let text = cut_comment(self.src.get(start..end).unwrap_or("")).trim();
        self.pos = end;
        if text.is_empty() {
            self.autonumber = Some(Autonumber::default());
            return Ok(());
        }
        if text.eq_ignore_ascii_case("off") {
            let mut a = self.autonumber.unwrap_or_default();
            a.visible = false;
            self.autonumber = Some(a);
            return Ok(());
        }
        let mut words = text.split_whitespace();
        let (first, second, extra) = (words.next(), words.next(), words.next());
        let start_v = first.and_then(hundredths);
        let step_v = match second {
            None => Some(100),
            Some(w) => hundredths(w),
        };
        match (start_v, step_v, extra) {
            (Some(s), Some(step), None) => {
                self.autonumber = Some(Autonumber {
                    start: s,
                    step,
                    visible: true,
                });
                Ok(())
            }
            _ => Err(self.fail(
                kw_start,
                end,
                "expected `autonumber`, `autonumber off`, or one or two numbers with at most two decimals",
            )),
        }
    }

    /// `link`, `links`, `properties`, `details`: parsed and dropped with `W021`, because
    /// they drive a popup the SVG never carries (specs/security.md#output).
    fn dropped_stmt(&mut self, kw_start: usize, word: &str) {
        let end = self.stmt_end(self.pos);
        self.warn(
            "W021",
            kw_start,
            end,
            alloc::format!("`{word}` drives a popup the SVG never carries; dropped"),
        );
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
        let raw = self.src.get(start..end).unwrap_or("");
        let text = self.finish_label(raw, start, end);
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

    fn finish(mut self) -> Sequence {
        // `destroy X` with no later message: the statement is dropped and `X` runs to
        // the last row (`R012`).
        let pending = core::mem::take(&mut self.destroying);
        for (i, span, start, end) in pending {
            let id = self
                .participants
                .get(i)
                .map(|p| excerpt(&p.id))
                .unwrap_or_default();
            self.repair(
                "R012",
                span,
                alloc::format!("`destroy {id}` has no later message naming it; dropped"),
                Fix {
                    span: self.idx.span(start, end),
                    replacement: String::new(),
                },
            );
        }
        // An activation still open at the end closes at the last row (`W023`).
        for i in 0..self.participants.len() {
            let open = self.open.get(i).copied().unwrap_or(0);
            if open == 0 {
                continue;
            }
            let (span, id) = self
                .participants
                .get(i)
                .map(|p| (p.span, excerpt(&p.id)))
                .unwrap_or_default();
            self.diags.emit(
                Severity::Warning,
                "W023",
                span,
                alloc::format!(
                    "{open} activation{} on `{id}` still open at the end; closed at the last row",
                    if open == 1 { " is" } else { "s are" }
                ),
            );
        }
        Sequence {
            meta: self.meta,
            participants: self.participants,
            boxes: self.boxes,
            items: self.items,
            autonumber: self.autonumber,
            messages: self.messages,
        }
    }
}

/// `head as <label>`: the text before the alias keyword and where the label starts.
fn alias_split(text: &str) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while let Some(j) = text.get(i..).and_then(|r| {
        r.char_indices()
            .find(|&(k, _)| {
                r.get(k..k + 2)
                    .is_some_and(|w| w.eq_ignore_ascii_case("as"))
            })
            .map(|(k, _)| k)
    }) {
        let at = i + j;
        let before_ok = at > 0 && matches!(bytes.get(at - 1), Some(b' ' | b'\t' | b'\r'));
        let after = at + 2;
        let after_ok = matches!(bytes.get(after), Some(b' ' | b'\t' | b'\r'));
        if before_ok && after_ok {
            let mut label = after;
            while matches!(bytes.get(label), Some(b' ' | b'\t' | b'\r')) {
                label += 1;
            }
            return Some((text.get(..at).unwrap_or("").trim_end(), label));
        }
        i = at + 1;
    }
    None
}

/// `text` without a case-insensitive `prefix`.
fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        text.get(prefix.len()..)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_are_ordered_longest_first() {
        for w in ARROWS.windows(2) {
            assert!(w[0].0.len() >= w[1].0.len(), "{:?}", w[0].0);
        }
        // No token is a prefix of a later one, so the first match is the longest.
        for (i, (a, ..)) in ARROWS.iter().enumerate() {
            for (b, ..) in ARROWS.iter().skip(i + 1) {
                assert!(!b.starts_with(a), "{a} shadows {b}");
            }
        }
    }

    #[test]
    fn entities_survive_comment_cutting() {
        assert_eq!(cut_comment("Alice # the buyer"), "Alice ");
        assert_eq!(cut_comment("#hearts; and #35;1"), "#hearts; and #35;1");
        assert_eq!(cut_comment("#hearts; # note"), "#hearts; ");
        assert_eq!(cut_comment("# only a comment"), "");
        assert_eq!(cut_comment("plain"), "plain");
    }

    #[test]
    fn hundredths_take_two_decimals() {
        assert_eq!(hundredths("1"), Some(100));
        assert_eq!(hundredths("1.05"), Some(105));
        assert_eq!(hundredths("0.1"), Some(10));
        assert_eq!(hundredths("10"), Some(1000));
        assert_eq!(hundredths("1.005"), None);
        assert_eq!(hundredths("x"), None);
        assert_eq!(hundredths(""), None);
    }

    #[test]
    fn leading_colours_parse() {
        assert!(matches!(leading_color("Aqua Group"), Some((4, Some(_)))));
        assert!(matches!(
            leading_color("transparent Aqua"),
            Some((11, None))
        ));
        assert_eq!(leading_color("Mobile clients"), None);
        assert!(matches!(
            leading_color("rgb(1, 2, 3) Ingest"),
            Some((12, Some(_)))
        ));
    }

    #[test]
    fn alias_splitting_needs_a_whole_word() {
        assert_eq!(alias_split("A as Alice"), Some(("A", 5)));
        assert_eq!(alias_split("Cast as Bob"), Some(("Cast", 8)));
        assert_eq!(alias_split("Alice"), None);
        assert_eq!(alias_split("fast lane"), None);
    }
}
