//! Text measurement from committed Inter metric tables (specs/text-measurement.md).
//! STUB: owned by the text workstream. The approximation below exists only so the
//! other stages compile and test before the tables land.

use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::Diagnostics;

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

/// Parses `<br>` and Markdown (`**bold**`, `*italic*`, `` `code` ``), strips bidi controls
/// (`W014`) and dropped characters, wraps at `max_width` on whitespace (hard break inside
/// longer words) and measures every run. `I010` once per diagram for unmeasured glyphs.
pub fn layout_label(
    text: &str,
    style: &TextStyle,
    max_width: f64,
    _diags: &mut Diagnostics,
) -> LabelLayout {
    let _ = max_width;
    let lh = style.font_size * 1.2102;
    let lines: Vec<Line> = text
        .split("<br>")
        .map(|l| {
            let w = l.chars().count() as f64 * style.font_size * 0.55;
            Line {
                runs: alloc::vec![Run {
                    text: String::from(l),
                    weight: style.weight,
                    italic: style.italic,
                    code: false,
                    width: w
                }],
                width: w,
            }
        })
        .collect();
    let width = lines
        .iter()
        .fold(0.0, |a: f64, l| if l.width > a { l.width } else { a });
    LabelLayout {
        height: lh * lines.len() as f64,
        width,
        lines,
        line_height: lh,
        ascent: style.font_size * 0.9688,
    }
}
