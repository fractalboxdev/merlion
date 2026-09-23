//! Label markup: character filtering, hard line breaks and Markdown emphasis
//! (specs/svg-output.md#text, specs/text-measurement.md#measuring).
//!
//! The parser is a single left-to-right pass per hard line with no recursion, so any
//! input terminates in linear time (specs/security.md). Emphasis follows the
//! CommonMark flanking rules in reduced form (CommonMark 0.31 §6.2): a delimiter opens
//! when followed by a non-space and closes when preceded by one, `_` never opens or
//! closes inside a word, and a delimiter without a partner stays literal text.

use alloc::vec::Vec;

/// One character with the formatting that applies to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Styled {
    pub c: char,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

/// A label split into hard lines (at `\n`), with its filtered characters styled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub lines: Vec<Vec<Styled>>,
    /// At least one bidirectional formatting character was removed (`W014`).
    pub bidi_stripped: bool,
}

/// U+202A–U+202E (embeddings, overrides, PDF) and U+2066–U+2069 (isolates).
pub fn is_bidi_control(c: char) -> bool {
    matches!(c as u32, 0x202A..=0x202E | 0x2066..=0x2069)
}

/// Filters one character: `Some(replacement)` keeps it, `None` drops it. Tab and
/// newline become a space; other C0/C1 controls, DEL and U+FFFE/U+FFFF are dropped so
/// the SVG stays well-formed XML 1.0 (specs/svg-output.md#text). Carriage return is
/// dropped, so CRLF becomes one space.
fn filter(c: char) -> Option<char> {
    match c {
        '\t' => Some(' '),
        '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}' | '\u{FFFE}' | '\u{FFFF}' => None,
        _ => Some(c),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok {
    Ch(char),
    /// `` ` ``
    Tick,
    /// `*`
    Star1,
    /// `**`
    Star2,
    /// `_`
    Under,
}

impl Tok {
    /// The source text of an unpaired token.
    fn literal(self, out: &mut Vec<char>) {
        match self {
            Tok::Ch(c) => out.push(c),
            Tok::Tick => out.push('`'),
            Tok::Star1 => out.push('*'),
            Tok::Star2 => {
                out.push('*');
                out.push('*');
            }
            Tok::Under => out.push('_'),
        }
    }

    /// The character a neighbouring delimiter sees for the flanking rules.
    fn flank_char(self) -> char {
        match self {
            Tok::Ch(c) => c,
            Tok::Tick => '`',
            Tok::Star1 | Tok::Star2 => '*',
            Tok::Under => '_',
        }
    }
}

/// Splits at the hard line breaks, filters and tokenizes.
///
/// `\n` is the only hard break: the parser writes one for every `<br>` the source
/// carries (specs/parser.md#labels-and-entity-codes), so a `<br>` still in the text
/// came from `#lt;br#gt;` and is drawn as the text it is.
fn tokenize(text: &str) -> (Vec<Vec<Tok>>, bool) {
    let mut bidi = false;
    let mut lines = Vec::new();
    for raw in text.split('\n') {
        let chars: Vec<char> = raw
            .chars()
            .filter(|&c| {
                let b = is_bidi_control(c);
                bidi |= b;
                !b
            })
            .filter_map(filter)
            .collect();
        lines.push(tokenize_line(&chars));
    }
    (lines, bidi)
}

fn tokenize_line(chars: &[char]) -> Vec<Tok> {
    let mut line = Vec::new();
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        let (tok, len) = match c {
            '`' => (Tok::Tick, 1),
            '*' if chars.get(i + 1) == Some(&'*') => (Tok::Star2, 2),
            '*' => (Tok::Star1, 1),
            '_' => (Tok::Under, 1),
            _ => (Tok::Ch(c), 1),
        };
        line.push(tok);
        i += len;
    }
    line
}

/// Role of each token after pairing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Literal,
    /// Toggles its formatting on or off.
    Active,
    /// Inside a code span: literal and monospace.
    Code,
}

/// The first and last index of the maximal run of equal tokens each position belongs to:
/// one pass each way, so the whole line costs two steps per token however long its runs.
/// `_a__b` gives `[0,1,2,2,4]` and `[0,1,3,3,4]`.
fn runs(toks: &[Tok], steps: &mut u64) -> (Vec<usize>, Vec<usize>) {
    let n = toks.len();
    let mut lo = alloc::vec![0usize; n];
    let mut hi = alloc::vec![0usize; n];
    let mut start = 0usize;
    for i in 0..n {
        *steps += 1;
        if toks[i] != toks[start] {
            start = i;
        }
        lo[i] = start;
    }
    let mut end = n.saturating_sub(1);
    for i in (0..n).rev() {
        *steps += 1;
        if toks[i] != toks[end] {
            end = i;
        }
        hi[i] = end;
    }
    (lo, hi)
}

fn pair(toks: &[Tok]) -> Vec<Role> {
    let mut steps = 0u64;
    pair_counted(toks, &mut steps)
}

/// [`pair`], counting every token it looks at. The count is what holds the pass linear:
/// a test reads it instead of a clock, so the bound is the same on every machine.
fn pair_counted(toks: &[Tok], steps: &mut u64) -> Vec<Role> {
    let mut roles = alloc::vec![Role::Literal; toks.len()];
    // Code spans first: backticks pair in order; an odd last one stays literal.
    let ticks: Vec<usize> = toks
        .iter()
        .enumerate()
        .inspect(|_| *steps += 1)
        .filter(|(_, t)| **t == Tok::Tick)
        .map(|(i, _)| i)
        .collect();
    for p in ticks.chunks_exact(2) {
        if let [a, b] = *p {
            for r in roles.iter_mut().take(b).skip(a + 1) {
                *steps += 1;
                *r = Role::Code;
            }
            if let Some(r) = roles.get_mut(a) {
                *r = Role::Active;
            }
            if let Some(r) = roles.get_mut(b) {
                *r = Role::Active;
            }
        }
    }
    // A run of the same delimiter flanks as one, so the text on each side of the whole
    // run decides whether each of its delimiters opens or closes: `State1___` neither
    // opens nor closes and every character of it is drawn. The two ends of every run are
    // found once, in one pass, rather than scanned for from each position in it.
    let (lo, hi) = runs(toks, steps);
    // Emphasis: each delimiter kind pairs independently, outside code spans.
    let is_space = |t: Option<&Tok>| t.is_none_or(|t| t.flank_char().is_whitespace());
    let is_alnum = |t: Option<&Tok>| t.is_some_and(|t| t.flank_char().is_alphanumeric());
    for kind in [Tok::Star2, Tok::Star1, Tok::Under] {
        let mut open: Option<usize> = None;
        for (i, t) in toks.iter().enumerate() {
            *steps += 1;
            if *t != kind || roles.get(i) != Some(&Role::Literal) {
                continue;
            }
            let prev = lo[i].checked_sub(1).and_then(|p| toks.get(p));
            let next = toks.get(hi[i] + 1);
            let under = kind == Tok::Under;
            let can_close = !is_space(prev) && !(under && is_alnum(next));
            let can_open = !is_space(next) && !(under && is_alnum(prev));
            match open {
                Some(o) if can_close => {
                    if let Some(r) = roles.get_mut(o) {
                        *r = Role::Active;
                    }
                    if let Some(r) = roles.get_mut(i) {
                        *r = Role::Active;
                    }
                    open = None;
                }
                // A later opener replaces an unclosed earlier one, which stays literal.
                _ if can_open => open = Some(i),
                _ => {}
            }
        }
    }
    roles
}

/// Applies paired delimiters to one hard line.
fn style_line(toks: &[Tok]) -> Vec<Styled> {
    let roles = pair(toks);
    let mut out = Vec::with_capacity(toks.len());
    let (mut bold, mut star, mut under, mut code) = (false, false, false, false);
    let mut lit = Vec::new();
    for (t, role) in toks.iter().zip(roles) {
        match role {
            Role::Active => match t {
                Tok::Tick => code = !code,
                Tok::Star2 => bold = !bold,
                Tok::Star1 => star = !star,
                Tok::Under => under = !under,
                Tok::Ch(_) => {}
            },
            Role::Literal | Role::Code => {
                lit.clear();
                t.literal(&mut lit);
                for &c in &lit {
                    out.push(Styled {
                        c,
                        bold: bold && !code,
                        italic: star || under,
                        code,
                    });
                }
            }
        }
    }
    out
}

/// Parses a label into styled hard lines. An empty label is one empty line.
pub fn parse(text: &str) -> Parsed {
    let (lines, bidi_stripped) = tokenize(text);
    Parsed {
        lines: lines.iter().map(|l| style_line(l)).collect(),
        bidi_stripped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    /// Renders a line as text with `B`/`I`/`C` flags per character, for compact assertions.
    fn show(line: &[Styled]) -> (String, String) {
        let text = line.iter().map(|s| s.c).collect();
        let flags = line
            .iter()
            .map(|s| match (s.bold, s.italic, s.code) {
                (_, _, true) => 'C',
                (true, true, _) => 'X',
                (true, false, _) => 'B',
                (false, true, _) => 'I',
                _ => '.',
            })
            .collect();
        (text, flags)
    }

    fn one(text: &str) -> (String, String) {
        let p = parse(text);
        assert_eq!(p.lines.len(), 1, "{text:?}");
        show(&p.lines[0])
    }

    #[test]
    fn plain_text_is_unstyled() {
        assert_eq!(one("a b"), ("a b".into(), "...".into()));
    }

    #[test]
    fn empty_label_is_one_empty_line() {
        let p = parse("");
        assert_eq!(p.lines.len(), 1);
        assert!(p.lines[0].is_empty());
    }

    #[test]
    fn a_newline_breaks_the_line() {
        let p = parse("a\nb");
        assert_eq!(p.lines.len(), 2);
        assert_eq!(show(&p.lines[0]).0, "a");
        assert_eq!(show(&p.lines[1]).0, "b");
    }

    #[test]
    fn html_is_literal_including_a_br() {
        assert_eq!(one("<b>x</b>").0, "<b>x</b>");
        assert_eq!(one("<brx>").0, "<brx>");
        assert_eq!(one("<br").0, "<br");
        assert_eq!(one("a<br/ x>").0, "a<br/ x>");
        // The parser writes `\n` for a source `<br>`, so this one is `#lt;br#gt;` text.
        assert_eq!(one("a<br>b").0, "a<br>b");
        assert_eq!(one("a<br/>b").0, "a<br/>b");
    }

    #[test]
    fn consecutive_breaks_make_empty_lines() {
        let p = parse("a\n\nb");
        assert_eq!(p.lines.len(), 3);
        assert!(p.lines[1].is_empty());
    }

    #[test]
    fn bold_italic_code() {
        assert_eq!(one("a **b** c"), ("a b c".into(), "..B..".into()));
        assert_eq!(one("*i*"), ("i".into(), "I".into()));
        assert_eq!(one("_i_"), ("i".into(), "I".into()));
        assert_eq!(one("`x*y*`"), ("x*y*".into(), "CCCC".into()));
    }

    #[test]
    fn nested_emphasis() {
        assert_eq!(
            one("**bold *both* b**"),
            ("bold both b".into(), "BBBBBXXXXBB".into())
        );
        assert_eq!(one("*it **bo***"), ("it bo".into(), "IIIXX".into()));
        assert_eq!(one("***x***"), ("x".into(), "X".into()));
    }

    #[test]
    fn unclosed_delimiters_stay_literal() {
        assert_eq!(one("**a"), ("**a".into(), "...".into()));
        assert_eq!(one("a*"), ("a*".into(), "..".into()));
        assert_eq!(one("`a"), ("`a".into(), "..".into()));
        assert_eq!(one("**a **b**"), ("**a b".into(), "....B".into()));
    }

    #[test]
    fn spaced_asterisks_are_literal() {
        assert_eq!(one("a * b * c").0, "a * b * c");
        assert_eq!(one("2 * 3").0, "2 * 3");
    }

    #[test]
    fn intraword_underscore_is_literal() {
        assert_eq!(
            one("snake_case_name"),
            ("snake_case_name".into(), "...............".into())
        );
        assert_eq!(one("a *b*c"), ("a bc".into(), "..I.".into()));
    }

    #[test]
    fn a_run_of_delimiters_flanks_as_one() {
        // A trailing run follows the end of the line, so it opens nothing and every
        // character of it is drawn (CommonMark 0.31 §6.2, the delimiter run).
        assert_eq!(
            one("State1_____________"),
            ("State1_____________".into(), ".".repeat(19))
        );
        assert_eq!(one("a___").0, "a___");
        assert_eq!(one("___a").0, "___a");
        assert_eq!(one("a ___ b").0, "a ___ b");
    }

    #[test]
    fn emphasis_does_not_cross_a_line_break() {
        let p = parse("**a\nb**");
        assert_eq!(show(&p.lines[0]).0, "**a");
        assert_eq!(show(&p.lines[1]).0, "b**");
    }

    #[test]
    fn code_is_never_bold() {
        assert_eq!(one("**a `c` a**"), ("a c a".into(), "BBCBB".into()));
    }

    #[test]
    fn filters_controls_and_noncharacters() {
        assert_eq!(one("a\tb\r").0, "a b");
        assert_eq!(one("a\u{0}\u{7}\u{7F}\u{85}b\u{FFFE}\u{FFFF}").0, "ab");
        assert!(!parse("ab").bidi_stripped);
    }

    #[test]
    fn strips_bidi_controls() {
        let p = parse("a\u{202E}b\u{2066}c\u{2069}\u{202A}");
        assert!(p.bidi_stripped);
        assert_eq!(show(&p.lines[0]).0, "abc");
    }

    #[test]
    fn pathological_input_terminates() {
        let s: String = "*_`<br".repeat(2000);
        let p = parse(&s);
        assert_eq!(p.lines.len(), 1);
    }

    /// Tokens `pair` looks at for one label, the whole hard line in one pass.
    fn pair_work(text: &str) -> u64 {
        let (lines, _) = tokenize(text);
        let mut steps = 0u64;
        for l in &lines {
            pair_counted(l, &mut steps);
        }
        steps
    }

    #[test]
    fn a_delimiter_run_costs_no_more_than_plain_text() {
        // 1 MiB of one delimiter is the worst case for the flanking rules: every token
        // is the same kind, so a scan over the run from each of its positions would be
        // quadratic. Labels are the shared text path, so the pass stays linear and the
        // work a delimiter run costs stays within a small multiple of plain text's.
        const N: usize = 1 << 20;
        let plain = pair_work(&"a".repeat(N));
        for run in ["_", "*", "**", "`"] {
            let work = pair_work(&run.repeat(N / run.len()));
            assert!(
                work <= 4 * plain,
                "{run:?} run costs {work} steps against {plain} for plain text"
            );
        }
    }
}
