//! Block structure of a stylesheet: rules, at-rules and declarations, with comments and
//! strings skipped. Brace depth is an explicit counter; nothing recurses.

use alloc::string::String;
use alloc::vec::Vec;

/// A `{…}` block or a `;`-terminated statement at one level.
pub struct Item {
    /// Byte range of the prelude (selector list or at-rule), trimmed.
    pub prelude: (usize, usize),
    /// Byte range of the block content, without the braces; `None` for a statement.
    pub body: Option<(usize, usize)>,
}

/// The input exceeds the block depth limit.
pub struct TooDeep;

struct Scan<'a> {
    b: &'a [u8],
    pos: usize,
    end: usize,
}

impl Scan<'_> {
    /// Skips a comment or a string starting at `pos`; returns true when it did.
    fn skip_opaque(&mut self) -> bool {
        let b = self.b;
        match b.get(self.pos) {
            Some(b'/') if b.get(self.pos + 1) == Some(&b'*') => {
                let mut i = self.pos + 2;
                while i + 1 < self.end && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                self.pos = if i + 1 < self.end { i + 2 } else { self.end };
                true
            }
            Some(&q @ (b'"' | b'\'')) => {
                let mut i = self.pos + 1;
                while i < self.end && b[i] != q && b[i] != b'\n' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                self.pos = (i + 1).min(self.end);
                true
            }
            _ => false,
        }
    }

    /// From just after an opening brace at `depth`, the index of its closing brace (or
    /// the end of input) and whether a nested block occurs inside.
    fn block_end(&mut self, depth: usize, max_depth: usize) -> Result<(usize, bool), TooDeep> {
        let mut d = depth;
        let mut nested = false;
        while self.pos < self.end {
            if self.skip_opaque() {
                continue;
            }
            match self.b[self.pos] {
                b'{' => {
                    d += 1;
                    nested = true;
                    if d > max_depth {
                        return Err(TooDeep);
                    }
                }
                b'}' => {
                    if d == depth {
                        let close = self.pos;
                        self.pos += 1;
                        return Ok((close, nested));
                    }
                    d -= 1;
                }
                _ => {}
            }
            self.pos += 1;
        }
        Ok((self.end, nested))
    }
}

fn trim(b: &[u8], mut s: usize, mut e: usize) -> (usize, usize) {
    while s < e && b[s].is_ascii_whitespace() {
        s += 1;
    }
    while e > s && b[e - 1].is_ascii_whitespace() {
        e -= 1;
    }
    (s, e)
}

/// The items of `src[start..end]`, a level whose blocks open at `depth` (1 at the top).
/// A block opening beyond `max_depth` fails the whole stylesheet.
pub fn items(
    src: &str,
    start: usize,
    end: usize,
    depth: usize,
    max_depth: usize,
) -> Result<Vec<(Item, bool)>, TooDeep> {
    let mut s = Scan {
        b: src.as_bytes(),
        pos: start,
        end,
    };
    let mut out = Vec::new();
    loop {
        // Skip whitespace and comments before a prelude.
        while s.pos < s.end {
            if s.b[s.pos].is_ascii_whitespace() {
                s.pos += 1;
            } else if s.b[s.pos] == b'/' && s.b.get(s.pos + 1) == Some(&b'*') {
                s.skip_opaque();
            } else {
                break;
            }
        }
        if s.pos >= s.end {
            break;
        }
        let p0 = s.pos;
        let mut paren = 0usize;
        let mut stop = None;
        while s.pos < s.end {
            if s.skip_opaque() {
                continue;
            }
            match s.b[s.pos] {
                b'(' | b'[' => paren += 1,
                b')' | b']' => paren = paren.saturating_sub(1),
                b'{' => {
                    stop = Some(b'{');
                    break;
                }
                b';' if paren == 0 => {
                    stop = Some(b';');
                    break;
                }
                b'}' => {
                    stop = Some(b'}');
                    break;
                }
                _ => {}
            }
            s.pos += 1;
        }
        let prelude = trim(s.b, p0, s.pos);
        match stop {
            Some(b'{') => {
                if depth > max_depth {
                    return Err(TooDeep);
                }
                s.pos += 1;
                let body_start = s.pos;
                let (close, nested) = s.block_end(depth, max_depth)?;
                out.push((
                    Item {
                        prelude,
                        body: Some((body_start, close)),
                    },
                    nested,
                ));
            }
            Some(_) => {
                s.pos += 1;
                if prelude.1 > prelude.0 {
                    out.push((
                        Item {
                            prelude,
                            body: None,
                        },
                        false,
                    ));
                }
            }
            None => {
                if prelude.1 > prelude.0 {
                    out.push((
                        Item {
                            prelude,
                            body: None,
                        },
                        false,
                    ));
                }
            }
        }
    }
    Ok(out)
}

/// Declarations of a block body: `(name range, value range)` pairs split at top-level
/// `;`, each trimmed; a piece without `:` has an empty name.
pub fn declarations(src: &str, start: usize, end: usize) -> Vec<((usize, usize), (usize, usize))> {
    let b = src.as_bytes();
    let mut s = Scan { b, pos: start, end };
    let mut out = Vec::new();
    let mut piece = start;
    let mut paren = 0usize;
    let mut colon: Option<usize> = None;
    let mut push = |from: usize, to: usize, colon: Option<usize>| {
        let (f, t) = trim(b, from, to);
        if f == t {
            return;
        }
        match colon {
            Some(c) if c >= f && c < t => out.push((trim(b, f, c), trim(b, c + 1, t))),
            _ => out.push(((f, f), (f, t))),
        }
    };
    while s.pos < s.end {
        if s.skip_opaque() {
            continue;
        }
        match b[s.pos] {
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b':' if colon.is_none() => colon = Some(s.pos),
            b';' if paren == 0 => {
                push(piece, s.pos, colon);
                piece = s.pos + 1;
                colon = None;
            }
            _ => {}
        }
        s.pos += 1;
    }
    push(piece, end, colon);
    out
}

/// `text` with comments removed and whitespace runs collapsed to one space.
pub fn normalise(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let mut space = false;
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut j = i + 2;
            while j + 1 < b.len() && !(b[j] == b'*' && b[j + 1] == b'/') {
                j += 1;
            }
            i = (j + 2).min(b.len());
            space = true;
            continue;
        }
        if b[i].is_ascii_whitespace() {
            space = true;
            i += 1;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        // Copy one UTF-8 scalar.
        let len = match b[i] {
            0x00..=0x7f => 1,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            _ => 4,
        };
        let j = (i + len).min(b.len());
        if let Some(t) = text.get(i..j) {
            out.push_str(t);
        }
        i = j;
    }
    out
}

/// Splits a selector list at top-level commas.
pub fn split_list(text: &str) -> Vec<&str> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut from = 0;
    for (i, &c) in b.iter().enumerate() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => {
                    out.push(text.get(from..i).unwrap_or("").trim());
                    from = i + 1;
                }
                _ => {}
            },
        }
    }
    out.push(text.get(from..).unwrap_or("").trim());
    out
}
