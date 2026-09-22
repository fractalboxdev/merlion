//! ```` ```mermaid ```` blocks in Markdown input (specs/integrations.md#cli).
//!
//! Fences follow CommonMark 0.31 §4.5: an opening fence is 0–3 spaces of indentation and
//! at least three backticks or tildes; a backtick fence's info string has no backtick; the
//! closing fence uses the same character, is at least as long, and has only whitespace
//! after it; an unclosed fence runs to the end of the document. Content lines lose up to
//! as many leading spaces as the opening fence had. Fences inside block quotes and list
//! items are not recognised.

/// One content line of a block, for mapping positions back into the file.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LineMap {
    /// Byte offset of the line in the block source.
    block_start: usize,
    /// Byte offset in the file of the first byte kept from the line.
    file_start: usize,
    /// Leading spaces removed from the line.
    stripped: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub source: String,
    /// 1-based file line of the first content line.
    pub first_line: u32,
    lines: Vec<LineMap>,
    /// Byte offset in the file just after the opening fence line.
    content_start: usize,
}

impl Block {
    /// Maps a 1-based (line, column) in the block to the file. Line 0 (no location) maps
    /// to the fence line itself.
    pub fn map_position(&self, line: u32, column: u32) -> (u32, u32) {
        if line == 0 {
            return (self.first_line.saturating_sub(1), 1);
        }
        let stripped = usize::try_from(line - 1)
            .ok()
            .and_then(|i| self.lines.get(i))
            .map_or(0, |l| l.stripped);
        (
            self.first_line.saturating_add(line - 1),
            column.max(1).saturating_add(stripped),
        )
    }

    /// Maps a byte offset in the block source to the file.
    pub fn map_byte(&self, offset: usize) -> usize {
        match self.lines.iter().rev().find(|l| l.block_start <= offset) {
            Some(l) => l.file_start.saturating_add(offset - l.block_start),
            None => self.content_start,
        }
    }

    /// Maps the exclusive end of a byte range. An end at the start of a line maps to the
    /// end of the previous line, before the next line's stripped indentation.
    pub fn map_byte_end(&self, offset: usize) -> usize {
        match self.lines.iter().rev().find(|l| l.block_start < offset) {
            Some(l) => l.file_start.saturating_add(offset - l.block_start),
            None => self.map_byte(offset),
        }
    }
}

pub fn is_markdown_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("mdx"))
}

/// An opening or closing fence: indentation, fence character, run length, rest of line.
struct Fence<'a> {
    indent: usize,
    ch: u8,
    len: usize,
    rest: &'a str,
}

fn fence(line: &str) -> Option<Fence<'_>> {
    let indent = line.bytes().take_while(|&b| b == b' ').count();
    if indent > 3 {
        return None;
    }
    let body = &line[indent..];
    let ch = *body.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = body.bytes().take_while(|&b| b == ch).count();
    if len < 3 {
        return None;
    }
    Some(Fence {
        indent,
        ch,
        len,
        rest: &body[len..],
    })
}

/// Every mermaid block in document order.
pub fn mermaid_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    // The open fence: character, run length, indentation, and the block being collected
    // (None for a fence of another language).
    let mut open: Option<(u8, usize, usize, Option<Block>)> = None;
    let mut offset = 0usize;
    for (idx, raw) in text.split_inclusive('\n').enumerate() {
        let line_no = u32::try_from(idx).unwrap_or(u32::MAX).saturating_add(1);
        let start = offset;
        offset += raw.len();
        let trimmed = raw.trim_end_matches(['\n', '\r']);
        match open.as_mut() {
            None => {
                let Some(f) = fence(trimmed) else { continue };
                if f.ch == b'`' && f.rest.contains('`') {
                    continue;
                }
                let info = f.rest.split_whitespace().next().unwrap_or("");
                let block = (info == "mermaid").then(|| Block {
                    source: String::new(),
                    first_line: line_no.saturating_add(1),
                    lines: Vec::new(),
                    content_start: offset,
                });
                open = Some((f.ch, f.len, f.indent, block));
            }
            Some((ch, len, indent, block)) => {
                if let Some(f) = fence(trimmed) {
                    if f.ch == *ch && f.len >= *len && f.rest.trim().is_empty() {
                        if let Some(b) = block.take() {
                            blocks.push(b);
                        }
                        open = None;
                        continue;
                    }
                }
                if let Some(b) = block.as_mut() {
                    let strip = raw.bytes().take(*indent).take_while(|&c| c == b' ').count();
                    b.lines.push(LineMap {
                        block_start: b.source.len(),
                        file_start: start + strip,
                        stripped: strip as u32,
                    });
                    b.source.push_str(&raw[strip..]);
                }
            }
        }
    }
    // An unclosed fence runs to the end of the document.
    if let Some((_, _, _, Some(b))) = open {
        blocks.push(b);
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_backtick_and_tilde_blocks_in_order() {
        let md =
            "# T\n\n```mermaid\nflowchart LR\nA-->B\n```\n\ntext\n\n~~~~ mermaid title\nB\n~~~~\n";
        let b = mermaid_blocks(md);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].source, "flowchart LR\nA-->B\n");
        assert_eq!(b[0].first_line, 4);
        assert_eq!(b[1].source, "B\n");
        assert_eq!(b[1].first_line, 11);
    }

    #[test]
    fn skips_other_languages_and_nested_fences() {
        let md = "````md\n```mermaid\nX\n```\n````\n```js\ny\n```\n";
        assert!(mermaid_blocks(md).is_empty());
    }

    #[test]
    fn closing_fence_must_match_char_and_length() {
        let md = "````mermaid\nA\n```\n~~~~\n````\n";
        let b = mermaid_blocks(md);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].source, "A\n```\n~~~~\n");
    }

    #[test]
    fn unclosed_fence_runs_to_end() {
        let b = mermaid_blocks("```mermaid\nA\nB");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].source, "A\nB");
    }

    #[test]
    fn four_space_indent_is_not_a_fence_and_backtick_info_cannot_hold_backticks() {
        assert!(mermaid_blocks("    ```mermaid\nA\n    ```\n").is_empty());
        assert!(mermaid_blocks("```mermaid`x\nA\n```\n").is_empty());
    }

    #[test]
    fn strips_fence_indentation_and_maps_positions_back() {
        let md = "x\n  ```mermaid\n  flowchart\n A\n  ```\n";
        let b = &mermaid_blocks(md)[0];
        assert_eq!(b.source, "flowchart\nA\n");
        assert_eq!(b.map_position(1, 1), (3, 3));
        assert_eq!(b.map_position(2, 1), (4, 2));
        assert_eq!(b.map_position(0, 0), (2, 1));
        // "A" is byte 10 of the block and byte 28 of the file.
        assert_eq!(b.map_byte(10), 28);
        assert_eq!(&md[b.map_byte(0)..b.map_byte(9)], "flowchart");
        // The end of the source maps to the end of the last kept line.
        assert_eq!(b.map_byte(b.source.len()), md.len() - "  ```\n".len());
        // An end offset at a line start stays on the previous line, so a replacement
        // never swallows the next line's stripped indentation.
        assert_eq!(b.map_byte(10), 28);
        assert_eq!(b.map_byte_end(10), 27);
        assert_eq!(b.map_byte_end(3), b.map_byte(3));
    }

    #[test]
    fn crlf_lines_are_fences_too() {
        let b = mermaid_blocks("```mermaid\r\nA\r\n```\r\n");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].source, "A\r\n");
    }

    #[test]
    fn recognises_markdown_extensions() {
        use std::path::Path;
        assert!(is_markdown_path(Path::new("a/b.md")));
        assert!(is_markdown_path(Path::new("b.MDX")));
        assert!(!is_markdown_path(Path::new("b.mmd")));
        assert!(!is_markdown_path(Path::new("md")));
    }
}
