//! Tests for `layout_label`. Expected widths are recomputed here straight from the
//! tables, independently of the measuring code.

use super::*;
use crate::diag::Severity;
use alloc::string::String;
use alloc::vec::Vec;

const EPS: f64 = 1e-9;

fn style(size: f64) -> TextStyle {
    TextStyle {
        font_size: size,
        ..TextStyle::default()
    }
}

fn lay(text: &str) -> (LabelLayout, Diagnostics) {
    let mut d = Diagnostics::new(false);
    let l = layout_label(text, &TextStyle::default(), 200.0, &mut d);
    (l, d)
}

/// Width in px computed straight from the table, with kerning between neighbours.
fn expected(text: &str, weight: Weight, size: f64) -> f64 {
    let t = table(weight);
    let chars: Vec<char> = text.chars().collect();
    let mut units = 0i64;
    for (i, c) in chars.iter().enumerate() {
        units += t.advance(*c).expect("covered") as i64;
        if let Some(n) = chars.get(i + 1) {
            units += t.kerning(*c, *n) as i64;
        }
    }
    units as f64 * size / t.units_per_em as f64
}

fn texts(l: &LabelLayout) -> Vec<String> {
    l.lines
        .iter()
        .map(|line| line.runs.iter().map(|r| r.text.as_str()).collect())
        .collect()
}

fn line_height(size: f64) -> f64 {
    let t = table(Weight::Regular);
    (t.ascender as f64 - t.descender as f64 + t.line_gap as f64) * size / t.units_per_em as f64
}

#[test]
fn single_word_width_matches_table() {
    let (l, d) = lay("Hello");
    assert_eq!(texts(&l), ["Hello"]);
    assert!((l.width - expected("Hello", Weight::Regular, 14.0)).abs() < EPS);
    assert!(d.items.is_empty());
}

#[test]
fn known_width_of_hello_in_font_units() {
    // H e l l o advances from the Regular table, plus any kerning pairs.
    let t = table(Weight::Regular);
    let units: i64 = "Hello"
        .chars()
        .map(|c| t.advance(c).unwrap() as i64)
        .sum::<i64>()
        + t.kerning('H', 'e') as i64
        + t.kerning('e', 'l') as i64
        + t.kerning('l', 'l') as i64
        + t.kerning('l', 'o') as i64;
    let (l, _) = lay("Hello");
    assert!((l.width - units as f64 * 14.0 / 2048.0).abs() < EPS);
}

#[test]
fn width_scales_with_font_size() {
    let mut d = Diagnostics::new(false);
    let a = layout_label("Merlion", &style(10.0), 1e9, &mut d);
    let b = layout_label("Merlion", &style(20.0), 1e9, &mut d);
    assert!((b.width - 2.0 * a.width).abs() < EPS);
    assert!((a.width - expected("Merlion", Weight::Regular, 10.0)).abs() < EPS);
}

#[test]
fn kerning_narrows_av() {
    let t = table(Weight::Regular);
    let (av, _) = lay("AV");
    let sum =
        (t.advance('A').unwrap() + t.advance('V').unwrap()) as f64 * 14.0 / t.units_per_em as f64;
    assert!(av.width < sum - 0.1);
    assert!((av.width - expected("AV", Weight::Regular, 14.0)).abs() < EPS);
}

#[test]
fn height_and_ascent_from_vertical_metrics() {
    let t = table(Weight::Regular);
    let (l, _) = lay("x");
    assert!((l.line_height - line_height(14.0)).abs() < EPS);
    assert!((l.height - l.line_height).abs() < EPS);
    let ascent = (t.ascender as f64 + t.line_gap as f64 / 2.0) * 14.0 / t.units_per_em as f64;
    assert!((l.ascent - ascent).abs() < EPS);
}

#[test]
fn empty_label_is_one_empty_line() {
    let (l, d) = lay("");
    assert_eq!(l.lines.len(), 1);
    assert!(l.lines[0].runs.is_empty());
    assert_eq!(l.width, 0.0);
    assert!((l.height - line_height(14.0)).abs() < EPS);
    assert!(d.items.is_empty());
}

#[test]
fn br_forces_lines_and_height() {
    for s in ["a<br>b", "a<br/>b", "a<br />b", "a<BR>b"] {
        let (l, _) = lay(s);
        assert_eq!(texts(&l), ["a", "b"], "{s}");
        assert!((l.height - 2.0 * l.line_height).abs() < EPS);
    }
    let (l, _) = lay("<br>");
    assert_eq!(l.lines.len(), 2);
}

#[test]
fn width_is_widest_line() {
    let (l, _) = lay("i<br>WWW");
    assert!((l.width - l.lines[1].width).abs() < EPS);
    assert!(l.lines[0].width < l.lines[1].width);
}

#[test]
fn wraps_on_whitespace() {
    let mut d = Diagnostics::new(false);
    let w = expected("alpha beta", Weight::Regular, 14.0);
    let l = layout_label("alpha beta gamma", &style(14.0), w + 0.01, &mut d);
    assert_eq!(texts(&l), ["alpha beta", "gamma"]);
    assert!((l.lines[0].width - w).abs() < EPS);
    assert!((l.height - 2.0 * l.line_height).abs() < EPS);
}

#[test]
fn exact_fit_does_not_wrap() {
    let mut d = Diagnostics::new(false);
    let w = expected("alpha beta", Weight::Regular, 14.0);
    let l = layout_label("alpha beta", &style(14.0), w, &mut d);
    assert_eq!(texts(&l), ["alpha beta"]);
}

#[test]
fn whitespace_collapses_and_trims() {
    let (l, _) = lay("  a \t  b  ");
    assert_eq!(texts(&l), ["a b"]);
    assert!((l.width - expected("a b", Weight::Regular, 14.0)).abs() < EPS);
}

#[test]
fn long_word_breaks_hard() {
    let mut d = Diagnostics::new(false);
    let max = expected("abcd", Weight::Regular, 14.0) + 0.01;
    let l = layout_label("x abcdefghij", &style(14.0), max, &mut d);
    let t = texts(&l);
    assert_eq!(t[0], "x");
    assert_eq!(t.concat(), "xabcdefghij");
    assert!(l.lines.iter().all(|line| line.width <= max + EPS));
    assert!(t.len() >= 4);
}

#[test]
fn char_wider_than_max_still_progresses() {
    let mut d = Diagnostics::new(false);
    let l = layout_label("WWW", &style(14.0), 1.0, &mut d);
    assert_eq!(texts(&l), ["W", "W", "W"]);
}

#[test]
fn non_positive_max_width_disables_wrapping() {
    let mut d = Diagnostics::new(false);
    for max in [0.0, -5.0, f64::NAN, f64::INFINITY] {
        let l = layout_label("a b c", &style(14.0), max, &mut d);
        assert_eq!(texts(&l), ["a b c"]);
    }
}

#[test]
fn bold_is_wider_and_uses_semibold_table() {
    let (plain, _) = lay("Wide label");
    let (bold, _) = lay("**Wide label**");
    assert!(bold.width > plain.width);
    assert!((bold.width - expected("Wide label", Weight::SemiBold, 14.0)).abs() < EPS);
    let run = &bold.lines[0].runs[0];
    assert_eq!(run.weight, Weight::SemiBold);
    assert_eq!(run.text, "Wide label");
}

#[test]
fn style_weight_applies_to_whole_label() {
    let mut d = Diagnostics::new(false);
    let s = TextStyle {
        weight: Weight::SemiBold,
        ..TextStyle::default()
    };
    let l = layout_label("Node", &s, 200.0, &mut d);
    assert!((l.width - expected("Node", Weight::SemiBold, 14.0)).abs() < EPS);
    assert_eq!(l.lines[0].runs[0].weight, Weight::SemiBold);
}

#[test]
fn style_italic_applies_to_whole_label() {
    let mut d = Diagnostics::new(false);
    let s = TextStyle {
        italic: true,
        ..TextStyle::default()
    };
    let l = layout_label("Node", &s, 200.0, &mut d);
    assert!(l.lines[0].runs[0].italic);
}

#[test]
fn runs_split_on_formatting() {
    let (l, _) = lay("a **b** *c* `d`");
    let runs = &l.lines[0].runs;
    let shape: Vec<(&str, Weight, bool, bool)> = runs
        .iter()
        .map(|r| (r.text.as_str(), r.weight, r.italic, r.code))
        .collect();
    assert_eq!(
        shape,
        [
            ("a ", Weight::Regular, false, false),
            ("b", Weight::SemiBold, false, false),
            (" ", Weight::Regular, false, false),
            ("c", Weight::Regular, true, false),
            (" ", Weight::Regular, false, false),
            ("d", Weight::Regular, false, true),
        ]
    );
    let sum: f64 = runs.iter().map(|r| r.width).sum();
    assert!((l.lines[0].width - sum).abs() < EPS);
}

#[test]
fn kerning_does_not_cross_runs() {
    // "A" regular next to "V" bold: two fonts, so no pair adjustment between them.
    let (l, _) = lay("A**V**");
    let want = expected("A", Weight::Regular, 14.0) + expected("V", Weight::SemiBold, 14.0);
    assert!((l.width - want).abs() < EPS);
}

#[test]
fn code_measures_at_the_monospace_advance() {
    // Code spans draw in a monospace family: 0.6 em per glyph, 1 em for wide glyphs,
    // whatever the weight (specs/svg-output.md#text).
    let mut d = Diagnostics::new(false);
    let s = TextStyle {
        weight: Weight::SemiBold,
        ..TextStyle::default()
    };
    let l = layout_label("`illegal_fill()`", &s, 400.0, &mut d);
    assert!((l.width - 14.0 * 0.6 * 14.0).abs() < 0.05, "{}", l.width);
    assert_eq!(l.lines[0].runs[0].weight, Weight::Regular);
    assert!(l.lines[0].runs[0].code);
    let l = layout_label("`中i`", &s, 400.0, &mut d);
    assert!((l.width - 1.6 * 14.0).abs() < 0.05, "{}", l.width);
    // The run after a code span starts where the monospace text ends.
    let l = layout_label(
        "`illegal_fill()` then",
        &TextStyle::default(),
        400.0,
        &mut d,
    );
    let want = 14.0 * 0.6 * 14.0 + expected(" then", Weight::Regular, 14.0);
    assert!((l.width - want).abs() < 0.05, "{} vs {}", l.width, want);
}

#[test]
fn italic_keeps_upright_advances() {
    let (a, _) = lay("*Text*");
    assert!((a.width - expected("Text", Weight::Regular, 14.0)).abs() < EPS);
    assert!(a.lines[0].runs[0].italic);
}

#[test]
fn unclosed_markdown_is_measured_literally() {
    let (l, _) = lay("**a");
    assert_eq!(texts(&l), ["**a"]);
    assert!((l.width - expected("**a", Weight::Regular, 14.0)).abs() < EPS);
}

#[test]
fn html_other_than_br_is_literal() {
    let (l, _) = lay("<b>x</b>");
    assert_eq!(texts(&l), ["<b>x</b>"]);
}

#[test]
fn cjk_fallback_is_one_em_with_i010_once() {
    let mut d = Diagnostics::new(false);
    let l = layout_label("中文", &style(14.0), 1e9, &mut d);
    assert!((l.width - 28.0).abs() < EPS);
    let _ = layout_label("한", &style(14.0), 1e9, &mut d);
    let i010: Vec<_> = d.items.iter().filter(|x| x.code == "I010").collect();
    assert_eq!(i010.len(), 1);
    assert_eq!(i010[0].severity, Severity::Info);
}

#[test]
fn fullwidth_fallback_is_one_em() {
    let (l, _) = lay("ＡＢ");
    assert!((l.width - 28.0).abs() < EPS);
}

#[test]
fn other_fallback_is_average_advance() {
    let t = table(Weight::Regular);
    let (l, d) = lay("א");
    assert!((l.width - t.avg_advance as f64 * 14.0 / t.units_per_em as f64).abs() < EPS);
    assert!(d.items.iter().any(|x| x.code == "I010"));
}

#[test]
fn combining_marks_add_no_width_and_no_i010() {
    let (l, d) = lay("e\u{0301}");
    assert!((l.width - expected("e", Weight::Regular, 14.0)).abs() < EPS);
    assert!(d.items.is_empty());
}

#[test]
fn bidi_controls_stripped_with_w014_once() {
    let mut d = Diagnostics::new(false);
    let l = layout_label("ab\u{202E}c", &style(14.0), 200.0, &mut d);
    assert_eq!(texts(&l), ["abc"]);
    assert!((l.width - expected("abc", Weight::Regular, 14.0)).abs() < EPS);
    let _ = layout_label("\u{2066}x", &style(14.0), 200.0, &mut d);
    let w014: Vec<_> = d.items.iter().filter(|x| x.code == "W014").collect();
    assert_eq!(w014.len(), 1);
    assert_eq!(w014[0].severity, Severity::Warning);
}

#[test]
fn dropped_characters_are_not_measured() {
    let (l, d) = lay("a\u{7}b\u{FFFF}");
    assert_eq!(texts(&l), ["ab"]);
    assert!(d.items.is_empty());
}

#[test]
fn invalid_font_size_falls_back_to_default() {
    let mut d = Diagnostics::new(false);
    let good = layout_label("abc", &style(14.0), 200.0, &mut d);
    for size in [f64::NAN, 0.0, -3.0, f64::INFINITY] {
        let l = layout_label("abc", &style(size), 200.0, &mut d);
        assert_eq!(l, good);
    }
}

#[test]
fn deterministic() {
    let s = "**Deploy** the *service* to `prod`<br>then verify AV Ta Wo";
    let (a, _) = lay(s);
    let (b, _) = lay(s);
    assert_eq!(a, b);
    assert_eq!(a.width.to_bits(), b.width.to_bits());
}

#[test]
fn long_label_wraps_within_max() {
    let s: String = "word ".repeat(1000);
    let (l, _) = lay(&s);
    assert!(l.lines.len() > 10);
    assert!(l.lines.iter().all(|x| x.width <= 200.0 + EPS));
}

#[test]
fn width_tolerance_per_mode() {
    assert_eq!(width_tolerance(FontMode::System), 1.06);
    assert_eq!(width_tolerance(FontMode::Link), 1.0);
    assert_eq!(width_tolerance(FontMode::Embed), 1.0);
}
