//! The literal defaults of the mixed roles are `color-mix(in oklab, fg p%, bg)` of the
//! foundation defaults (specs/svg-output.md#theming). This test recomputes each mix with
//! an `f64` reference of the CSS Color 4 algorithm (sRGB → linear → OKLab, interpolate,
//! back, round to 8 bit) and asserts the constants the core stores; `tests/color_mix.rs`
//! checks the core's own `oklab_mix` against the same constants.

mod css_support;

use merlion_render::svg::theme;

fn mix(fg: &str, bg: &str, pct: f64) -> String {
    let c = css_support::rgba8(&format!("color-mix(in oklab, {} {}%, {})", fg, pct, bg))
        .expect("colour");
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
