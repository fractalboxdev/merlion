//! Typed source styles (specs/svg-output.md#source-styles-classdef-style-linkstyle).
//!
//! `classDef`, `style` and `linkStyle` carry free-form CSS in Mermaid. Each
//! `property:value` pair is parsed into a typed [`Style`] field; anything outside the
//! accepted grammar is reported back to the caller, which drops it with `W010`.

use alloc::string::String;
use alloc::vec::Vec;

use crate::model::{Color, FontStyle, FontWeight, Style};

/// A declaration that was dropped, as a byte range relative to the parsed text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

/// Result of parsing a style list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parsed {
    pub style: Style,
    pub rejected: Vec<Rejected>,
    /// A colour that ignores the theme was set (`I030 FixedColour`).
    pub fixed_colour: bool,
}

/// Parses one colour from the accepted grammar; `None` when it is outside it.
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    let lower = s.to_ascii_lowercase();
    if lower == "transparent" {
        return Some(Color::Transparent);
    }
    if lower == "none" {
        return Some(Color::None);
    }
    if let Some(args) = function_args(&lower, &["rgba", "rgb"]) {
        return parse_rgb(args);
    }
    if let Some(args) = function_args(&lower, &["hsla", "hsl"]) {
        return parse_hsl(args);
    }
    named_color(&lower).map(Color::Named)
}

/// `name(args)` for the first of `names` that prefixes `s` (list longer names first);
/// returns the text inside the parentheses.
fn function_args<'a>(s: &'a str, names: &[&str]) -> Option<&'a str> {
    for name in names {
        if let Some(rest) = s.strip_prefix(name) {
            let rest = rest.trim_start();
            return rest.strip_prefix('(')?.strip_suffix(')');
        }
    }
    None
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    let d: Vec<u8> = hex.bytes().map(hex_digit).collect::<Option<Vec<u8>>>()?;
    let (r, g, b, a) = match d.as_slice() {
        [r, g, b] => (r * 17, g * 17, b * 17, 255),
        [r, g, b, a] => (r * 17, g * 17, b * 17, a * 17),
        [r1, r2, g1, g2, b1, b2] => (r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, 255),
        [r1, r2, g1, g2, b1, b2, a1, a2] => {
            (r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, a1 * 16 + a2)
        }
        _ => return None,
    };
    Some(Color::Rgba { r, g, b, a })
}

/// A plain decimal number: `[+-]?(\d+(\.\d*)?|\.\d+)`. Exponents, `inf` and `NaN`,
/// which `f64::from_str` would accept, are rejected.
pub fn parse_number(s: &str) -> Option<f64> {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    let mut digits = 0usize;
    let mut dots = 0usize;
    for b in body.bytes() {
        match b {
            b'0'..=b'9' => digits += 1,
            b'.' => dots += 1,
            _ => return None,
        }
    }
    if digits == 0 || dots > 1 {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A number or a percentage; a percentage comes back as a fraction of 1 with `true`.
fn parse_component(s: &str) -> Option<(f64, bool)> {
    let s = s.trim();
    match s.strip_suffix('%') {
        Some(p) => parse_number(p.trim_end()).map(|v| (v / 100.0, true)),
        None => parse_number(s).map(|v| (v, false)),
    }
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Splits CSS colour-function arguments: comma-separated, or the space-separated
/// form with an optional `/ alpha`.
fn split_args(args: &str) -> Option<Vec<&str>> {
    if args.contains(',') {
        return Some(args.split(',').map(str::trim).collect());
    }
    let (main, alpha) = match args.split_once('/') {
        Some((m, a)) => (m, Some(a.trim())),
        None => (args, None),
    };
    let mut parts: Vec<&str> = main.split_whitespace().collect();
    if let Some(a) = alpha {
        if parts.len() != 3 {
            return None;
        }
        parts.push(a);
    }
    Some(parts)
}

fn alpha_of(s: Option<&&str>) -> Option<f64> {
    match s {
        None => Some(1.0),
        Some(a) => {
            let (v, _) = parse_component(a)?;
            Some(clamp(v, 0.0, 1.0))
        }
    }
}

fn to_u8(v: f64) -> u8 {
    // After clamping and rounding, `v` is an integer in 0..=255, so the cast is exact.
    crate::math::round(clamp(v, 0.0, 255.0)) as u8
}

fn parse_rgb(args: &str) -> Option<Color> {
    let parts = split_args(args)?;
    if parts.len() != 3 && parts.len() != 4 {
        return None;
    }
    let mut rgb = [0u8; 3];
    for (slot, part) in rgb.iter_mut().zip(parts.iter()) {
        let (v, pct) = parse_component(part)?;
        *slot = to_u8(if pct { v * 255.0 } else { v });
    }
    let a = alpha_of(parts.get(3))?;
    Some(Color::Rgba {
        r: rgb[0],
        g: rgb[1],
        b: rgb[2],
        a: to_u8(a * 255.0),
    })
}

fn parse_hsl(args: &str) -> Option<Color> {
    let parts = split_args(args)?;
    if parts.len() != 3 && parts.len() != 4 {
        return None;
    }
    let hue_text = parts.first()?.trim();
    let hue_text = hue_text.strip_suffix("deg").unwrap_or(hue_text);
    let h = parse_number(hue_text.trim_end())?;
    // Hue wraps into [0, 360).
    let h = h - 360.0 * crate::math::floor(h / 360.0);
    let h = if (0.0..360.0).contains(&h) { h } else { 0.0 };
    let pct = |s: &str| -> Option<f64> {
        let (v, is_pct) = parse_component(s)?;
        Some(clamp(if is_pct { v * 100.0 } else { v }, 0.0, 100.0))
    };
    let s = pct(parts.get(1)?)?;
    let l = pct(parts.get(2)?)?;
    let a = alpha_of(parts.get(3))?;
    Some(Color::Hsla { h, s, l, a })
}

fn named_color(lower: &str) -> Option<&'static str> {
    NAMED_COLORS
        .binary_search(&lower)
        .ok()
        .and_then(|i| NAMED_COLORS.get(i).copied())
}

/// Whether `name` (case-insensitive) is one of the 148 CSS named colours.
pub fn is_named_color(name: &str) -> bool {
    named_color(&name.to_ascii_lowercase()).is_some()
}

/// The CSS Color Module Level 4 named colours, sorted for binary search.
/// `transparent` is a keyword of its own ([`Color::Transparent`]).
#[rustfmt::skip]
pub const NAMED_COLORS: &[&str] = &[
    "aliceblue", "antiquewhite", "aqua", "aquamarine", "azure", "beige", "bisque", "black",
    "blanchedalmond", "blue", "blueviolet", "brown", "burlywood", "cadetblue", "chartreuse",
    "chocolate", "coral", "cornflowerblue", "cornsilk", "crimson", "cyan", "darkblue",
    "darkcyan", "darkgoldenrod", "darkgray", "darkgreen", "darkgrey", "darkkhaki",
    "darkmagenta", "darkolivegreen", "darkorange", "darkorchid", "darkred", "darksalmon",
    "darkseagreen", "darkslateblue", "darkslategray", "darkslategrey", "darkturquoise",
    "darkviolet", "deeppink", "deepskyblue", "dimgray", "dimgrey", "dodgerblue", "firebrick",
    "floralwhite", "forestgreen", "fuchsia", "gainsboro", "ghostwhite", "gold", "goldenrod",
    "gray", "green", "greenyellow", "grey", "honeydew", "hotpink", "indianred", "indigo",
    "ivory", "khaki", "lavender", "lavenderblush", "lawngreen", "lemonchiffon", "lightblue",
    "lightcoral", "lightcyan", "lightgoldenrodyellow", "lightgray", "lightgreen", "lightgrey",
    "lightpink", "lightsalmon", "lightseagreen", "lightskyblue", "lightslategray",
    "lightslategrey", "lightsteelblue", "lightyellow", "lime", "limegreen", "linen",
    "magenta", "maroon", "mediumaquamarine", "mediumblue", "mediumorchid", "mediumpurple",
    "mediumseagreen", "mediumslateblue", "mediumspringgreen", "mediumturquoise",
    "mediumvioletred", "midnightblue", "mintcream", "mistyrose", "moccasin", "navajowhite",
    "navy", "oldlace", "olive", "olivedrab", "orange", "orangered", "orchid", "palegoldenrod",
    "palegreen", "paleturquoise", "palevioletred", "papayawhip", "peachpuff", "peru", "pink",
    "plum", "powderblue", "purple", "rebeccapurple", "red", "rosybrown", "royalblue",
    "saddlebrown", "salmon", "sandybrown", "seagreen", "seashell", "sienna", "silver",
    "skyblue", "slateblue", "slategray", "slategrey", "snow", "springgreen", "steelblue",
    "tan", "teal", "thistle", "tomato", "turquoise", "violet", "wheat", "white", "whitesmoke",
    "yellow", "yellowgreen",
];

/// Splits a style list into trimmed `(start, end)` declaration ranges. Commas at
/// parenthesis depth 0 separate declarations. A comma-separated `stroke-dasharray`
/// (`5, 10`) would otherwise split into a piece without a `:`, so such a piece is
/// merged back into a preceding `stroke-dasharray`.
fn split_decls(text: &str) -> Vec<(usize, usize)> {
    let mut raw = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth = depth.saturating_add(1),
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                raw.push((start, i));
                start = i + 1;
            }
            _ => {}
        }
    }
    raw.push((start, text.len()));
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (s, e) in raw {
        let (s, e) = trim_range(text, s, e);
        if s >= e {
            continue;
        }
        let piece = text.get(s..e).unwrap_or("");
        if !piece.contains(':') {
            if let Some(last) = out.last_mut() {
                let prev = text.get(last.0..last.1).unwrap_or("");
                let prop = prev.split(':').next().unwrap_or("").trim();
                if prop.eq_ignore_ascii_case("stroke-dasharray") {
                    last.1 = e;
                    continue;
                }
            }
        }
        out.push((s, e));
    }
    out
}

fn trim_range(text: &str, s: usize, e: usize) -> (usize, usize) {
    let piece = text.get(s..e).unwrap_or("");
    let lead = piece.len() - piece.trim_start().len();
    let trail = piece.len() - piece.trim_end().len();
    let s = s + lead;
    (s, e.saturating_sub(trail).max(s))
}

/// Parses a `property:value[,property:value…]` list. A trailing `;` is ignored.
pub fn parse_style_list(text: &str) -> Parsed {
    let mut out = Parsed::default();
    let body_end = text.trim_end().trim_end_matches(';').len();
    let body = text.get(..body_end).unwrap_or("");
    for (s, e) in split_decls(body) {
        let decl = body.get(s..e).unwrap_or("");
        if let Err(message) = apply_decl(decl, &mut out) {
            out.rejected.push(Rejected {
                start: s,
                end: e,
                message,
            });
        }
    }
    out
}

fn is_fixed(c: &Color) -> bool {
    matches!(c, Color::Rgba { .. } | Color::Hsla { .. } | Color::Named(_))
}

fn apply_decl(decl: &str, out: &mut Parsed) -> Result<(), String> {
    let Some((prop, value)) = decl.split_once(':') else {
        return Err(alloc::format!(
            "style declaration `{}` has no value; dropped",
            decl.trim()
        ));
    };
    let prop = prop.trim().to_ascii_lowercase();
    let value = value.trim();
    let bad_value = || {
        alloc::format!(
            "style value `{}` is not accepted for `{}`; dropped",
            value,
            prop
        )
    };
    let s = &mut out.style;
    match prop.as_str() {
        "fill" | "stroke" | "color" => {
            let c = parse_color(value).ok_or_else(bad_value)?;
            if prop == "color" && c == Color::None {
                return Err(bad_value());
            }
            out.fixed_colour |= is_fixed(&c);
            match prop.as_str() {
                "fill" => s.fill = Some(c),
                "stroke" => s.stroke = Some(c),
                _ => s.color = Some(c),
            }
        }
        "stroke-width" => {
            let lower = value.to_ascii_lowercase();
            let v = lower.strip_suffix("px").unwrap_or(&lower).trim_end();
            let n = parse_number(v)
                .filter(|n| (0.0..=20.0).contains(n))
                .ok_or_else(bad_value)?;
            s.stroke_width = Some(n);
        }
        "stroke-dasharray" => {
            let nums: Option<Vec<f64>> = value
                .split([' ', ',', '\t'])
                .filter(|t| !t.is_empty())
                .map(|t| parse_number(t).filter(|n| (0.0..=100.0).contains(n)))
                .collect();
            let nums = nums
                .filter(|v| !v.is_empty() && v.len() <= 8)
                .ok_or_else(bad_value)?;
            s.stroke_dasharray = Some(nums);
        }
        "opacity" | "fill-opacity" | "stroke-opacity" => {
            let n = parse_number(value)
                .filter(|n| (0.0..=1.0).contains(n))
                .ok_or_else(bad_value)?;
            match prop.as_str() {
                "opacity" => s.opacity = Some(n),
                "fill-opacity" => s.fill_opacity = Some(n),
                _ => s.stroke_opacity = Some(n),
            }
        }
        "font-weight" => {
            s.font_weight = Some(match value.to_ascii_lowercase().as_str() {
                "normal" | "400" => FontWeight::Regular,
                "bold" | "600" | "700" => FontWeight::SemiBold,
                _ => return Err(bad_value()),
            });
        }
        "font-style" => {
            s.font_style = Some(match value.to_ascii_lowercase().as_str() {
                "normal" => FontStyle::Normal,
                "italic" => FontStyle::Italic,
                _ => return Err(bad_value()),
            });
        }
        _ => {
            return Err(alloc::format!(
                "style property `{}` is not supported; dropped",
                prop
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(r: u8, g: u8, b: u8, a: u8) -> Option<Color> {
        Some(Color::Rgba { r, g, b, a })
    }

    #[test]
    fn colours() {
        let cases: &[(&str, Option<Color>)] = &[
            ("#f9f", rgba(0xff, 0x99, 0xff, 255)),
            ("#F9F8", rgba(0xff, 0x99, 0xff, 0x88)),
            ("#123456", rgba(0x12, 0x34, 0x56, 255)),
            ("#12345678", rgba(0x12, 0x34, 0x56, 0x78)),
            ("#12345", None),
            ("#ggg", None),
            ("#", None),
            ("rgb(1, 2, 3)", rgba(1, 2, 3, 255)),
            ("RGB(1,2,3)", rgba(1, 2, 3, 255)),
            ("rgba(255, 0, 0, 0.5)", rgba(255, 0, 0, 128)),
            ("rgb(100%, 0%, 50%)", rgba(255, 0, 128, 255)),
            ("rgb(300, -5, 0)", rgba(255, 0, 0, 255)),
            ("rgb(1 2 3 / 50%)", rgba(1, 2, 3, 128)),
            ("rgba(1, 2, 3, 40%)", rgba(1, 2, 3, 102)),
            ("rgb(1, 2)", None),
            ("rgb(1, 2, 3", None),
            ("rgb(a, b, c)", None),
            ("rgb(1e3, 2, 3)", None),
            ("url(#x)", None),
            (
                "hsl(120, 50%, 25%)",
                Some(Color::Hsla {
                    h: 120.0,
                    s: 50.0,
                    l: 25.0,
                    a: 1.0,
                }),
            ),
            (
                "hsla(-30deg, 200%, 25, 0.25)",
                Some(Color::Hsla {
                    h: 330.0,
                    s: 100.0,
                    l: 25.0,
                    a: 0.25,
                }),
            ),
            (
                "hsl(720, 0%, 0%)",
                Some(Color::Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.0,
                    a: 1.0,
                }),
            ),
            ("red", Some(Color::Named("red"))),
            ("RebeccaPurple", Some(Color::Named("rebeccapurple"))),
            ("transparent", Some(Color::Transparent)),
            ("none", Some(Color::None)),
            ("reddish", None),
            ("currentColor", None),
            ("", None),
        ];
        for (input, want) in cases {
            assert_eq!(parse_color(input), *want, "{input:?}");
        }
    }

    #[test]
    fn named_colour_table_is_complete_and_sorted() {
        assert_eq!(NAMED_COLORS.len(), 148);
        assert!(NAMED_COLORS.windows(2).all(|w| w[0] < w[1]));
        for n in ["aliceblue", "yellowgreen", "grey", "darkslategrey", "lime"] {
            assert!(is_named_color(n), "{n}");
        }
        assert!(!is_named_color("transparent"));
    }

    #[test]
    fn style_list_accepted_properties() {
        let p = parse_style_list(
            "fill:#f9f,stroke:#333,stroke-width:4px,color:blue,stroke-dasharray: 5 5,\
             opacity:0.5,fill-opacity:1,stroke-opacity:0,font-weight:bold,font-style:italic",
        );
        assert_eq!(p.rejected, Vec::new());
        assert!(p.fixed_colour);
        let s = p.style;
        assert_eq!(s.fill, rgba(0xff, 0x99, 0xff, 255));
        assert_eq!(s.stroke, rgba(0x33, 0x33, 0x33, 255));
        assert_eq!(s.color, Some(Color::Named("blue")));
        assert_eq!(s.stroke_width, Some(4.0));
        assert_eq!(s.stroke_dasharray, Some(alloc::vec![5.0, 5.0]));
        assert_eq!(s.opacity, Some(0.5));
        assert_eq!(s.fill_opacity, Some(1.0));
        assert_eq!(s.stroke_opacity, Some(0.0));
        assert_eq!(s.font_weight, Some(FontWeight::SemiBold));
        assert_eq!(s.font_style, Some(FontStyle::Italic));
    }

    #[test]
    fn style_list_commas_inside_functions_and_dasharrays() {
        let p = parse_style_list("fill:rgb(1,2,3), stroke-dasharray: 5, 10 ,stroke:none;");
        assert_eq!(p.rejected, Vec::new());
        assert_eq!(p.style.fill, rgba(1, 2, 3, 255));
        assert_eq!(p.style.stroke_dasharray, Some(alloc::vec![5.0, 10.0]));
        assert_eq!(p.style.stroke, Some(Color::None));
        assert!(p.fixed_colour);
    }

    #[test]
    fn transparent_and_none_are_not_fixed_colours() {
        let p = parse_style_list("fill:transparent,stroke:none");
        assert!(!p.fixed_colour);
        assert!(p.rejected.is_empty());
    }

    #[test]
    fn style_list_rejections() {
        let cases: &[(&str, &str)] = &[
            ("font-size:20px", "font-size"),
            ("color:none", "color"),
            ("fill:url(http://x)", "fill"),
            ("stroke-width:21", "stroke-width"),
            ("stroke-width:-1px", "stroke-width"),
            ("stroke-width:4em", "stroke-width"),
            ("stroke-dasharray:1 2 3 4 5 6 7 8 9", "stroke-dasharray"),
            ("stroke-dasharray:101", "stroke-dasharray"),
            ("opacity:1.5", "opacity"),
            ("font-weight:300", "font-weight"),
            ("font-style:oblique", "font-style"),
            ("fill", "fill"),
            ("fill:red !important", "fill"),
            ("background:url(x)", "background"),
            ("fill:expression(alert(1))", "fill"),
        ];
        for (input, prop) in cases {
            let p = parse_style_list(input);
            assert_eq!(p.rejected.len(), 1, "{input:?}");
            assert!(p.rejected[0].message.contains(prop), "{input:?}");
            assert!(p.style.is_empty(), "{input:?}");
        }
    }

    #[test]
    fn rejected_ranges_point_into_the_text() {
        let text = "fill:red, bogus:1";
        let p = parse_style_list(text);
        assert_eq!(p.rejected.len(), 1);
        let r = &p.rejected[0];
        assert_eq!(&text[r.start..r.end], "bogus:1");
        assert_eq!(p.style.fill, Some(Color::Named("red")));
    }

    #[test]
    fn font_weights() {
        for (v, w) in [
            ("normal", FontWeight::Regular),
            ("400", FontWeight::Regular),
            ("bold", FontWeight::SemiBold),
            ("600", FontWeight::SemiBold),
            ("700", FontWeight::SemiBold),
        ] {
            let p = parse_style_list(&alloc::format!("font-weight:{v}"));
            assert_eq!(p.style.font_weight, Some(w), "{v}");
        }
    }

    #[test]
    fn empty_list_is_empty_style() {
        let p = parse_style_list("  ");
        assert!(p.style.is_empty());
        assert!(p.rejected.is_empty());
    }
}
