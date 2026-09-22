//! Re-serialisation of typed source styles (specs/svg-output.md#source-styles-classdef-style-linkstyle,
//! specs/security.md#output rule 2). Only typed values reach the output; every number is
//! clamped to its accepted range and printed with `numfmt`, and a named colour is re-checked
//! to be plain lower-case ASCII letters.

use alloc::string::String;
use core::fmt::Write;

use crate::model::{Color, FontStyle, FontWeight, Style};
use crate::numfmt::push_num;

fn clamp(v: f64, lo: f64, hi: f64) -> Option<f64> {
    if !v.is_finite() {
        return None;
    }
    Some(if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    })
}

/// CSS text for a colour, or `None` when the value cannot be written safely.
pub fn color_css(c: &Color) -> Option<String> {
    let mut s = String::new();
    match *c {
        Color::Rgba { r, g, b, a } => {
            let _ = write!(s, "#{:02x}{:02x}{:02x}", r, g, b);
            if a != 255 {
                let _ = write!(s, "{:02x}", a);
            }
        }
        Color::Hsla { h, s: sat, l, a } => {
            let h = clamp(h, 0.0, 360.0)?;
            let sat = clamp(sat, 0.0, 100.0)?;
            let l = clamp(l, 0.0, 100.0)?;
            let a = clamp(a, 0.0, 1.0)?;
            // Legacy comma syntax: understood by every browser and by librsvg.
            s.push_str(if a < 1.0 { "hsla(" } else { "hsl(" });
            push_num(&mut s, h);
            s.push_str(", ");
            push_num(&mut s, sat);
            s.push_str("%, ");
            push_num(&mut s, l);
            s.push('%');
            if a < 1.0 {
                s.push_str(", ");
                push_num(&mut s, a);
            }
            s.push(')');
        }
        Color::Named(name) => {
            if name.is_empty() || name.len() > 32 || !name.bytes().all(|b| b.is_ascii_lowercase()) {
                return None;
            }
            s.push_str(name);
        }
        Color::Transparent => s.push_str("transparent"),
        Color::None => s.push_str("none"),
    }
    Some(s)
}

/// True when the colour ignores the theme (`I030 FixedColour`): anything but `none`.
pub fn is_fixed(c: &Option<Color>) -> bool {
    matches!(c, Some(c) if *c != Color::None)
}

fn decl(out: &mut String, prop: &str, value: &str) {
    out.push_str(prop);
    out.push(':');
    out.push_str(value);
    out.push(';');
}

fn decl_num(out: &mut String, prop: &str, v: f64, lo: f64, hi: f64, unit: &str) {
    if let Some(v) = clamp(v, lo, hi) {
        out.push_str(prop);
        out.push(':');
        push_num(out, v);
        out.push_str(unit);
        out.push(';');
    }
}

/// Declarations for a shape (node shape or edge path): `fill`, `stroke`, stroke width,
/// dash array and the opacities. `with_fill` is false for edge paths, which stay unfilled.
pub fn shape_decls(style: &Style, with_fill: bool) -> String {
    let mut out = String::new();
    if with_fill {
        if let Some(v) = style.fill.as_ref().and_then(color_css) {
            decl(&mut out, "fill", &v);
        }
    }
    if let Some(v) = style.stroke.as_ref().and_then(color_css) {
        decl(&mut out, "stroke", &v);
    }
    if let Some(w) = style.stroke_width {
        decl_num(&mut out, "stroke-width", w, 0.0, 20.0, "px");
    }
    if let Some(d) = &style.stroke_dasharray {
        let mut v = String::new();
        for n in d.iter().take(8) {
            let Some(n) = clamp(*n, 0.0, 100.0) else {
                continue;
            };
            if !v.is_empty() {
                v.push(' ');
            }
            push_num(&mut v, n);
        }
        if !v.is_empty() {
            decl(&mut out, "stroke-dasharray", &v);
        }
    }
    if with_fill {
        if let Some(o) = style.fill_opacity {
            decl_num(&mut out, "fill-opacity", o, 0.0, 1.0, "");
        }
    }
    if let Some(o) = style.stroke_opacity {
        decl_num(&mut out, "stroke-opacity", o, 0.0, 1.0, "");
    }
    out
}

/// Declarations for the `text` inside a styled element: `color` becomes the text fill.
pub fn text_decls(style: &Style) -> String {
    let mut out = String::new();
    if let Some(v) = style
        .color
        .as_ref()
        .filter(|c| **c != Color::None)
        .and_then(color_css)
    {
        decl(&mut out, "fill", &v);
    }
    match style.font_weight {
        Some(FontWeight::SemiBold) => decl(&mut out, "font-weight", "600"),
        Some(FontWeight::Regular) => decl(&mut out, "font-weight", "400"),
        None => {}
    }
    match style.font_style {
        Some(FontStyle::Italic) => decl(&mut out, "font-style", "italic"),
        Some(FontStyle::Normal) => decl(&mut out, "font-style", "normal"),
        None => {}
    }
    out
}

/// Declarations for the element group itself: `opacity`.
pub fn group_decls(style: &Style) -> String {
    let mut out = String::new();
    if let Some(o) = style.opacity {
        decl_num(&mut out, "opacity", o, 0.0, 1.0, "");
    }
    out
}

/// `[A-Za-z_][A-Za-z0-9_-]{0,63}`: the accepted `classDef` name (specs/svg-output.md).
pub fn is_valid_class_name(name: &str) -> bool {
    let b = name.as_bytes();
    match b.first() {
        Some(c) if c.is_ascii_alphabetic() || *c == b'_' => {}
        _ => return false,
    }
    b.len() <= 64
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn rgba_serialises_as_hex() {
        let c = Color::Rgba {
            r: 255,
            g: 0,
            b: 16,
            a: 255,
        };
        assert_eq!(color_css(&c).as_deref(), Some("#ff0010"));
        let c = Color::Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 128,
        };
        assert_eq!(color_css(&c).as_deref(), Some("#01020380"));
    }

    #[test]
    fn hsla_serialises_with_legacy_syntax() {
        let c = Color::Hsla {
            h: 210.0,
            s: 50.0,
            l: 40.5,
            a: 1.0,
        };
        assert_eq!(color_css(&c).as_deref(), Some("hsl(210, 50%, 40.5%)"));
        let c = Color::Hsla {
            h: 10.0,
            s: 150.0,
            l: -3.0,
            a: 0.25,
        };
        assert_eq!(color_css(&c).as_deref(), Some("hsla(10, 100%, 0%, 0.25)"));
        let c = Color::Hsla {
            h: f64::NAN,
            s: 1.0,
            l: 1.0,
            a: 1.0,
        };
        assert_eq!(color_css(&c), None);
    }

    #[test]
    fn named_transparent_none() {
        assert_eq!(
            color_css(&Color::Named("rebeccapurple")).as_deref(),
            Some("rebeccapurple")
        );
        assert_eq!(
            color_css(&Color::Transparent).as_deref(),
            Some("transparent")
        );
        assert_eq!(color_css(&Color::None).as_deref(), Some("none"));
    }

    #[test]
    fn hostile_named_colour_is_dropped() {
        assert_eq!(color_css(&Color::Named("red;}body{x:url(a)")), None);
        assert_eq!(color_css(&Color::Named("")), None);
    }

    #[test]
    fn shape_declarations_clamp_numbers() {
        let s = Style {
            fill: Some(Color::Named("red")),
            stroke_width: Some(50.0),
            stroke_dasharray: Some(vec![5.0, 200.0, f64::NAN, 1.5]),
            fill_opacity: Some(2.0),
            ..Style::default()
        };
        assert_eq!(
            shape_decls(&s, true),
            "fill:red;stroke-width:20px;stroke-dasharray:5 100 1.5;fill-opacity:1;"
        );
        assert_eq!(
            shape_decls(&s, false),
            "stroke-width:20px;stroke-dasharray:5 100 1.5;"
        );
    }

    #[test]
    fn dasharray_keeps_at_most_eight_numbers() {
        let s = Style {
            stroke_dasharray: Some(vec![1.0; 12]),
            ..Style::default()
        };
        assert_eq!(shape_decls(&s, false), "stroke-dasharray:1 1 1 1 1 1 1 1;");
    }

    #[test]
    fn text_declarations() {
        let s = Style {
            color: Some(Color::Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
            font_weight: Some(FontWeight::SemiBold),
            font_style: Some(FontStyle::Italic),
            ..Style::default()
        };
        assert_eq!(
            text_decls(&s),
            "fill:#000000;font-weight:600;font-style:italic;"
        );
    }

    #[test]
    fn class_name_grammar() {
        assert!(is_valid_class_name("ok_Name-1"));
        assert!(is_valid_class_name("_x"));
        assert!(!is_valid_class_name("1x"));
        assert!(!is_valid_class_name("a b"));
        assert!(!is_valid_class_name("x\"><script>"));
        assert!(!is_valid_class_name(""));
        let long: String = core::iter::repeat_n('a', 65).collect();
        assert!(!is_valid_class_name(&long));
    }

    #[test]
    fn fixed_colour_detection() {
        assert!(is_fixed(&Some(Color::Transparent)));
        assert!(!is_fixed(&Some(Color::None)));
        assert!(!is_fixed(&None));
    }
}
