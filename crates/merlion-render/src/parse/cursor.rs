//! Source positions and a byte cursor shared by the hand-written parsers.
//!
//! Spans follow specs/parser.md#diagnostics: 1-based line, 1-based column counted in
//! Unicode scalar values, UTF-8 byte offsets.

use alloc::vec::Vec;
use core::cell::Cell;

use crate::diag::Span;

/// Maps byte offsets to lines and columns. Column lookups are memoised per line so
/// that the mostly increasing offsets a parser asks for cost amortised O(1), even on
/// a single 1 MiB line.
pub struct LineIndex<'a> {
    src: &'a str,
    starts: Vec<usize>,
    /// (byte offset, line index, column) of the last lookup.
    memo: Cell<(usize, usize, u32)>,
}

fn to_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

impl<'a> LineIndex<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut starts = Vec::new();
        starts.push(0);
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        LineIndex {
            src,
            starts,
            memo: Cell::new((0, 0, 1)),
        }
    }

    pub fn src(&self) -> &'a str {
        self.src
    }

    /// Clamps `byte` into the source and back onto a character boundary.
    fn clamp(&self, byte: usize) -> usize {
        let mut b = byte.min(self.src.len());
        while b > 0 && !self.src.is_char_boundary(b) {
            b -= 1;
        }
        b
    }

    fn line_of(&self, byte: usize) -> usize {
        self.starts
            .partition_point(|&s| s <= byte)
            .saturating_sub(1)
    }

    /// 1-based line and column of `byte`.
    pub fn line_col(&self, byte: usize) -> (u32, u32) {
        let byte = self.clamp(byte);
        let line = self.line_of(byte);
        let line_start = self.starts.get(line).copied().unwrap_or(0);
        let (m_byte, m_line, m_col) = self.memo.get();
        let chars =
            |a: usize, b: usize| to_u32(self.src.get(a..b).map_or(0, |s| s.chars().count()));
        // Count from the memo in either direction when it is on the same line, so a
        // lookup costs the distance to the previous one, not to the line start.
        let col = if m_line == line && m_byte >= line_start {
            if byte >= m_byte {
                m_col.saturating_add(chars(m_byte, byte))
            } else {
                m_col.saturating_sub(chars(byte, m_byte)).max(1)
            }
        } else {
            1u32.saturating_add(chars(line_start, byte))
        };
        self.memo.set((byte, line, col));
        (to_u32(line + 1), col)
    }

    pub fn span(&self, start: usize, end: usize) -> Span {
        let start = self.clamp(start);
        let end = self.clamp(end.max(start));
        let (line, column) = self.line_col(start);
        Span {
            line,
            column,
            byte_start: to_u32(start),
            byte_end: to_u32(end),
        }
    }

    /// Byte offset of the start of the line containing `byte`.
    pub fn line_start(&self, byte: usize) -> usize {
        let line = self.line_of(self.clamp(byte));
        self.starts.get(line).copied().unwrap_or(0)
    }

    /// Byte offset just past the `\n` ending the line containing `byte`, or the end of input.
    pub fn next_line_start(&self, byte: usize) -> usize {
        let line = self.line_of(self.clamp(byte));
        self.starts.get(line + 1).copied().unwrap_or(self.src.len())
    }
}

/// A position in the source with character-level helpers. Every method stays on
/// character boundaries and never indexes out of bounds.
#[derive(Clone, Copy)]
pub struct Cursor<'a> {
    pub src: &'a str,
    pub pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(src: &'a str, pos: usize) -> Self {
        Cursor { src, pos }
    }

    pub fn rest(&self) -> &'a str {
        self.src.get(self.pos..).unwrap_or("")
    }

    pub fn at_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    pub fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    pub fn peek_nth(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    pub fn byte_at(&self, off: usize) -> Option<u8> {
        self.src
            .as_bytes()
            .get(self.pos.saturating_add(off))
            .copied()
    }

    pub fn starts_with(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    pub fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    pub fn eat(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    /// Skips spaces, tabs and carriage returns (not newlines). Returns whether any were skipped.
    pub fn skip_hws(&mut self) -> bool {
        let start = self.pos;
        while matches!(self.peek(), Some(' ' | '\t' | '\r')) {
            self.pos += 1;
        }
        self.pos != start
    }

    /// Skips any whitespace, newlines included.
    pub fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    /// Moves to the `\n` ending the current line (or the end of input).
    pub fn skip_to_eol(&mut self) {
        match self.rest().find('\n') {
            Some(i) => self.pos += i,
            None => self.pos = self.src.len(),
        }
    }

    /// Byte offset of the end of the current line (the `\n` or the end of input).
    pub fn eol(&self) -> usize {
        match self.rest().find('\n') {
            Some(i) => self.pos + i,
            None => self.src.len(),
        }
    }

    /// True when only horizontal whitespace precedes `pos` on its line.
    /// Walks back over horizontal whitespace only, so the cost is the indentation.
    pub fn at_line_start(&self) -> bool {
        at_line_start(self.src, self.pos)
    }
}

/// True when only spaces, tabs or carriage returns precede `pos` on its line.
pub fn at_line_start(src: &str, pos: usize) -> bool {
    let bytes = src.as_bytes();
    let mut i = pos.min(bytes.len());
    while i > 0 {
        match bytes.get(i - 1) {
            Some(b' ' | b'\t' | b'\r') => i -= 1,
            Some(b'\n') => return true,
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_count_columns_in_scalars() {
        let src = "ab\ncé d\n\nx";
        let idx = LineIndex::new(src);
        assert_eq!(idx.line_col(0), (1, 1));
        assert_eq!(idx.line_col(1), (1, 2));
        assert_eq!(idx.line_col(3), (2, 1));
        // `é` is two bytes; `d` is byte 7 and column 4.
        assert_eq!(idx.line_col(7), (2, 4));
        assert_eq!(idx.line_col(9), (3, 1));
        assert_eq!(idx.line_col(10), (4, 1));
        // Out-of-order lookups do not reuse a stale memo.
        assert_eq!(idx.line_col(4), (2, 2));
        assert_eq!(idx.line_col(1), (1, 2));
    }

    #[test]
    fn span_clamps_and_keeps_boundaries() {
        let src = "é";
        let idx = LineIndex::new(src);
        let s = idx.span(1, 99);
        assert_eq!((s.byte_start, s.byte_end), (0, 2));
        assert_eq!((s.line, s.column), (1, 1));
    }

    #[test]
    fn cursor_helpers() {
        let mut c = Cursor::new("  a\tb\nc", 0);
        assert!(c.at_line_start());
        assert!(c.skip_hws());
        assert_eq!(c.peek(), Some('a'));
        assert!(c.at_line_start());
        c.bump();
        assert!(!c.at_line_start());
        c.skip_to_eol();
        assert_eq!(c.peek(), Some('\n'));
        c.skip_ws();
        assert_eq!(c.peek(), Some('c'));
        assert!(c.at_line_start());
        assert_eq!(c.bump(), Some('c'));
        assert_eq!(c.bump(), None);
        assert!(c.at_eof());
    }

    #[test]
    fn line_bounds() {
        let idx = LineIndex::new("ab\ncd");
        assert_eq!(idx.line_start(4), 3);
        assert_eq!(idx.next_line_start(1), 3);
        assert_eq!(idx.next_line_start(4), 5);
    }
}
