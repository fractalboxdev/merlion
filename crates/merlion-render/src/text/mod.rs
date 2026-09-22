//! Text measurement from committed Inter metric tables (specs/text-measurement.md).

pub mod embed;
pub mod font;
pub mod markup;
#[rustfmt::skip]
mod tables;
#[cfg(test)]
mod tests;

use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::{Diagnostics, Severity, Span};
use crate::options::FontMode;

pub use embed::{base64_encode, embedded_font_css, ofl_xml_comment};
pub use font::{table, FontTable};

/// Width factor the layout multiplies label widths by before adding padding: the
/// system-font stack is measured with Inter's tables at a declared ±6% tolerance
/// (specs/text-measurement.md#serving-the-font), so boxes get 6% extra room.
/// `Link` and `Embed` draw in the measured font, so they need no extra room.
pub fn width_tolerance(mode: FontMode) -> f64 {
    match mode {
        FontMode::System => 1.06,
        FontMode::Link | FontMode::Embed => 1.0,
    }
}

/// Font size used when the requested one is not a positive finite number.
const DEFAULT_FONT_SIZE: f64 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    SemiBold,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub font_size: f64,
    pub weight: Weight,
    pub italic: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            font_size: 14.0,
            weight: Weight::Regular,
            italic: false,
        }
    }
}

/// A run of text with uniform formatting inside one line.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub weight: Weight,
    pub italic: bool,
    /// From `` `code` ``: drawn in a monospace family (measured with the Regular table).
    pub code: bool,
    pub width: f64,
}

/// Detail lines of a title + detail node label are measured and drawn at this
/// fraction of the font size (specs/text-measurement.md#measuring).
pub const DETAIL_SCALE: f64 = 0.8;
/// Extra space below the last title line of a title + detail label, as a fraction of
/// the font size (2 px at 14 px).
pub const DETAIL_GAP_EM: f64 = 1.0 / 7.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub runs: Vec<Run>,
    pub width: f64,
    /// Font size the line is measured and drawn at, in px.
    pub size: f64,
    /// A detail line of a title + detail node label, drawn as `merlion-detail`.
    pub detail: bool,
    /// Height of the line box; the last title line of a title + detail label includes
    /// the gap below it.
    pub height: f64,
    /// Distance from the line's top to its baseline.
    pub ascent: f64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct LabelLayout {
    pub lines: Vec<Line>,
    /// Widest line.
    pub width: f64,
    /// Sum of the line heights.
    pub height: f64,
    /// Line height at the label's font size.
    pub line_height: f64,
    /// Distance from a line's top to its baseline at the label's font size.
    pub ascent: f64,
}

/// A character ready to measure: its font, formatting and table metrics.
#[derive(Clone, Copy, Debug)]
struct Glyph {
    c: char,
    weight: Weight,
    italic: bool,
    code: bool,
    /// Advance in font units.
    advance: i64,
    left: u16,
    right: u16,
}

impl Glyph {
    /// Characters with the same key are drawn in one `<tspan>` with one font.
    fn key(&self) -> (Weight, bool, bool) {
        (self.weight, self.italic, self.code)
    }
}

/// Advance of `next` placed after `prev`, in font units. Pair kerning applies only
/// between neighbours in the same run: a run boundary changes the font (weight,
/// synthesized oblique or monospace), and browsers shape each font separately.
fn step(prev: Option<&Glyph>, next: &Glyph) -> i64 {
    let kern = match prev {
        Some(p) if p.key() == next.key() => {
            font::table(next.weight).class_kerning(p.left, next.right) as i64
        }
        _ => 0,
    };
    next.advance.saturating_add(kern)
}

/// Width of `seq` appended after `prev`, in font units.
fn extent(prev: Option<&Glyph>, seq: &[Glyph]) -> i64 {
    let mut units = 0i64;
    let mut last = prev;
    for g in seq {
        units = units.saturating_add(step(last, g));
        last = Some(g);
    }
    units
}

struct Measurer {
    size: f64,
    units_per_em: f64,
    base: TextStyle,
    unmeasured: Option<char>,
}

impl Measurer {
    /// Font units to px. Both weights share `unitsPerEm` (2048 for Inter), so one scale
    /// serves every run; the expression order is fixed so equal inputs give equal bits.
    fn px(&self, units: i64) -> f64 {
        units as f64 * self.size / self.units_per_em
    }

    fn glyph(&mut self, s: markup::Styled) -> Glyph {
        // Code spans use the Regular table (specs/svg-output.md#text); `**` and a
        // SemiBold base style use SemiBold.
        let weight = if s.code {
            Weight::Regular
        } else if s.bold || self.base.weight == Weight::SemiBold {
            Weight::SemiBold
        } else {
            Weight::Regular
        };
        let (advance, left, right) = match font::table(weight).glyph(s.c) {
            font::Glyph::Known {
                advance,
                left,
                right,
            } => (advance, left, right),
            font::Glyph::Fallback { advance } => {
                self.unmeasured.get_or_insert(s.c);
                (advance, 0, 0)
            }
            font::Glyph::ZeroWidth => (0, 0, 0),
        };
        Glyph {
            c: s.c,
            weight,
            italic: s.italic || self.base.italic,
            code: s.code,
            advance: advance as i64,
            left,
            right,
        }
    }

    /// Groups a line's glyphs into runs of uniform formatting and measures each.
    fn finish_line(&self, glyphs: &[Glyph]) -> Line {
        let mut runs: Vec<Run> = Vec::new();
        let mut start = 0;
        while start < glyphs.len() {
            let key = glyphs.get(start).map(Glyph::key);
            let len = glyphs
                .get(start..)
                .unwrap_or(&[])
                .iter()
                .take_while(|g| Some(g.key()) == key)
                .count();
            let run = glyphs.get(start..start + len).unwrap_or(&[]);
            if let Some(first) = run.first() {
                runs.push(Run {
                    text: run.iter().map(|g| g.c).collect(),
                    weight: first.weight,
                    italic: first.italic,
                    code: first.code,
                    width: self.px(extent(None, run)),
                });
            }
            start += len.max(1);
        }
        let width = runs.iter().fold(0.0, |a, r| a + r.width);
        let t = font::table(Weight::Regular);
        Line {
            runs,
            width,
            size: self.size,
            detail: false,
            height: self.px(t.ascender as i64 - t.descender as i64 + t.line_gap as i64),
            // CSS places half the line gap above the ascender.
            ascent: (t.ascender as f64 + t.line_gap as f64 / 2.0) * self.size
                / t.units_per_em as f64,
        }
    }
}

/// A word and the space that precedes it on its line (absent for a line's first word).
struct Word {
    space: Option<Glyph>,
    glyphs: Vec<Glyph>,
}

/// Splits a hard line at spaces. Runs of spaces collapse to their first space and
/// leading/trailing spaces are dropped, as SVG's default white-space handling draws them.
fn words(glyphs: Vec<Glyph>) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    let mut space: Option<Glyph> = None;
    let mut current: Vec<Glyph> = Vec::new();
    for g in glyphs {
        if g.c == ' ' {
            if !current.is_empty() {
                out.push(Word {
                    space: space.take(),
                    glyphs: core::mem::take(&mut current),
                });
            }
            if space.is_none() && !out.is_empty() {
                space = Some(g);
            }
        } else {
            current.push(g);
        }
    }
    if !current.is_empty() {
        out.push(Word {
            space,
            glyphs: current,
        });
    }
    out
}

/// Greedy wrapping of one hard line at `max_width` px (specs/text-measurement.md#measuring):
/// a word moves to a new line when it does not fit, and a word wider than the maximum
/// breaks between characters. Every line holds at least one character, so the loop
/// always progresses.
fn wrap(m: &Measurer, glyphs: Vec<Glyph>, max_width: f64, out: &mut Vec<Line>) {
    let fits = |units: i64| m.px(units) <= max_width;
    let mut line: Vec<Glyph> = Vec::new();
    let mut units = 0i64;
    for word in words(glyphs) {
        if !line.is_empty() {
            let mut seq = Vec::with_capacity(word.glyphs.len() + 1);
            seq.extend(word.space);
            seq.extend_from_slice(&word.glyphs);
            let add = extent(line.last(), &seq);
            if fits(units.saturating_add(add)) {
                line.extend(seq);
                units = units.saturating_add(add);
                continue;
            }
            out.push(m.finish_line(&line));
            line.clear();
            units = 0;
        }
        // The word starts a line.
        let whole = extent(None, &word.glyphs);
        if fits(whole) {
            line = word.glyphs;
            units = whole;
            continue;
        }
        for g in word.glyphs {
            let add = step(line.last(), &g);
            if !line.is_empty() && !fits(units.saturating_add(add)) {
                out.push(m.finish_line(&line));
                line.clear();
                units = 0;
                units = units.saturating_add(step(None, &g));
            } else {
                units = units.saturating_add(add);
            }
            line.push(g);
        }
    }
    out.push(m.finish_line(&line));
}

/// Parses `<br>` and Markdown (`**bold**`, `*italic*`, `_italic_`, `` `code` ``), strips
/// bidi controls (`W014`) and dropped characters, wraps at `max_width` on whitespace
/// (hard break inside longer words) and measures every run. `I010` once per diagram for
/// unmeasured glyphs. A `max_width` that is not a positive finite number disables
/// wrapping; a `font_size` that is not one measures at 14 px.
///
/// Width is the sum of advances plus in-run pair kerning, scaled by
/// `font_size / unitsPerEm`; each line is `ascender − descender + lineGap` tall
/// (specs/text-measurement.md#measuring). Every line is measured at the font size;
/// node labels go through [`layout_node_label`].
pub fn layout_label(
    text: &str,
    style: &TextStyle,
    max_width: f64,
    diags: &mut Diagnostics,
) -> LabelLayout {
    layout(text, style, max_width, diags, false)
}

/// [`layout_label`] for a node label. A title + detail label ([`is_title_detail`])
/// measures its first hard line, wrapped, at the font size and every later line at
/// [`DETAIL_SCALE`] × the font size, with line height and ascent scaled the same way
/// and [`DETAIL_GAP_EM`] × the font size added below the last title line. Any other
/// label lays out exactly as [`layout_label`].
pub fn layout_node_label(
    text: &str,
    style: &TextStyle,
    max_width: f64,
    diags: &mut Diagnostics,
) -> LabelLayout {
    layout(text, style, max_width, diags, true)
}

/// Whether a line is one `**bold**` span, ignoring surrounding spaces: it holds a
/// non-space character and every character between the first and last non-space is bold.
fn is_bold_line(line: &[markup::Styled]) -> bool {
    let first = line.iter().position(|s| s.c != ' ');
    let last = line.iter().rposition(|s| s.c != ' ');
    match (first, last) {
        (Some(a), Some(b)) => line.get(a..=b).is_some_and(|l| l.iter().all(|s| s.bold)),
        _ => false,
    }
}

fn parsed_is_title_detail(p: &markup::Parsed) -> bool {
    match p.lines.split_first() {
        Some((title, rest)) => {
            is_bold_line(title) && rest.iter().any(|l| l.iter().any(|s| s.c != ' '))
        }
        None => false,
    }
}

/// Whether a node label renders as title + detail (specs/svg-output.md#text): its first
/// hard line is one `**bold**` span and a later hard line holds text.
pub fn is_title_detail(text: &str) -> bool {
    parsed_is_title_detail(&markup::parse(text))
}

fn layout(
    text: &str,
    style: &TextStyle,
    max_width: f64,
    diags: &mut Diagnostics,
    node: bool,
) -> LabelLayout {
    let size = if style.font_size.is_finite() && style.font_size > 0.0 {
        style.font_size
    } else {
        DEFAULT_FONT_SIZE
    };
    let max_width = if max_width.is_finite() && max_width > 0.0 {
        max_width
    } else {
        f64::INFINITY
    };
    let regular = font::table(Weight::Regular);
    let mut m = Measurer {
        size,
        units_per_em: regular.units_per_em as f64,
        base: *style,
        unmeasured: None,
    };

    let parsed = markup::parse(text);
    if parsed.bidi_stripped {
        diags.emit_once(
            Severity::Warning,
            "W014",
            Span::default(),
            "bidirectional formatting characters removed from a label",
        );
    }
    let tiered = node && parsed_is_title_detail(&parsed);
    let mut dm = Measurer {
        size: size * DETAIL_SCALE,
        units_per_em: m.units_per_em,
        base: *style,
        unmeasured: None,
    };
    let mut lines: Vec<Line> = Vec::new();
    for (i, hard) in parsed.lines.into_iter().enumerate() {
        if !tiered || i == 0 {
            let glyphs: Vec<Glyph> = hard.into_iter().map(|s| m.glyph(s)).collect();
            wrap(&m, glyphs, max_width, &mut lines);
            continue;
        }
        if i == 1 {
            if let Some(last) = lines.last_mut() {
                last.height += size * DETAIL_GAP_EM;
            }
        }
        let from = lines.len();
        let glyphs: Vec<Glyph> = hard.into_iter().map(|s| dm.glyph(s)).collect();
        wrap(&dm, glyphs, max_width, &mut lines);
        for l in lines.iter_mut().skip(from) {
            l.detail = true;
        }
    }
    if let Some(c) = m.unmeasured.or(dm.unmeasured) {
        diags.emit_once(
            Severity::Info,
            "I010",
            Span::default(),
            alloc::format!(
                "U+{:04X} is outside the font table; its width is estimated",
                c as u32
            ),
        );
    }

    let line_height =
        m.px(regular.ascender as i64 - regular.descender as i64 + regular.line_gap as i64);
    // CSS places half the line gap above the ascender.
    let ascent = (regular.ascender as f64 + regular.line_gap as f64 / 2.0) * size
        / regular.units_per_em as f64;
    let width = lines
        .iter()
        .fold(0.0, |a: f64, l| if l.width > a { l.width } else { a });
    // Uniform labels keep the product form, so their bits never depend on summation order.
    let height = if tiered {
        lines.iter().fold(0.0, |a: f64, l| a + l.height)
    } else {
        line_height * lines.len() as f64
    };
    LabelLayout {
        height,
        width,
        lines,
        line_height,
        ascent,
    }
}
