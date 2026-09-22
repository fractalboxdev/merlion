//! The literal defaults of the mixed roles are `color-mix(in oklab, fg p%, bg)` of the
//! foundation defaults (specs/svg-output.md#theming). This test recomputes each mix with
//! the CSS Color 4 algorithm (sRGB → linear → OKLab, interpolate, back, round to 8 bit)
//! and asserts the constants the core stores.

use merlion_render::svg::theme;

fn parse(hex: &str) -> [f64; 3] {
    let v = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f64 / 255.0;
    [v(1), v(3), v(5)]
}

fn to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_gamma(c: f64) -> f64 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Björn Ottosson, "A perceptual color space for image processing" (2020).
fn oklab([r, g, b]: [f64; 3]) -> [f64; 3] {
    let (r, g, b) = (to_linear(r), to_linear(g), to_linear(b));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn srgb([ll, a, b]: [f64; 3]) -> [f64; 3] {
    let l = (ll + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (ll - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (ll - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        to_gamma(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
        to_gamma(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
        to_gamma(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s),
    ]
}

fn mix(fg: &str, bg: &str, pct: f64) -> String {
    let (f, b) = (oklab(parse(fg)), oklab(parse(bg)));
    let p = pct / 100.0;
    let m = [0, 1, 2].map(|i| f[i] * p + b[i] * (1.0 - p));
    let c = srgb(m).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

#[test]
fn mixed_role_constants_are_the_oklab_mixes_of_the_defaults() {
    for (pct, constant) in [
        (55.0, theme::MUTED),
        (45.0, theme::LINE),
        (4.0, theme::SURFACE),
        (22.0, theme::BORDER),
        (2.0, theme::CLUSTER_BG),
    ] {
        assert_eq!(mix(theme::FG, theme::BG, pct), constant, "{}%", pct);
    }
}

#[test]
fn mix_endpoints_are_the_foundations() {
    assert_eq!(mix(theme::FG, theme::BG, 100.0), theme::FG);
    assert_eq!(mix(theme::FG, theme::BG, 0.0), theme::BG);
}
