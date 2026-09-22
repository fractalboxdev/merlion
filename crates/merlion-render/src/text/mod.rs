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

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub runs: Vec<Run>,
    pub width: f64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct LabelLayout {
    pub lines: Vec<Line>,
    /// Widest line.
    pub width: f64,
    /// `lines.len() * line_height`.
    pub height: f64,
    pub line_height: f64,
    /// Distance from a line's top to its baseline.
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
        Line { runs, width }
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
/// (specs/text-measurement.md#measuring).
pub fn layout_label(
    text: &str,
    style: &TextStyle,
    max_width: f64,
    diags: &mut Diagnostics,
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
    let mut lines = Vec::new();
    for hard in parsed.lines {
        let glyphs: Vec<Glyph> = hard.into_iter().map(|s| m.glyph(s)).collect();
        wrap(&m, glyphs, max_width, &mut lines);
    }
    if let Some(c) = m.unmeasured {
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
    LabelLayout {
        height: line_height * lines.len() as f64,
        width,
        lines,
        line_height,
        ascent,
    }
}
