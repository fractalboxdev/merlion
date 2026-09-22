//! The core's software `oklab_mix` (specs/architecture.md#determinism,
//! specs/svg-output.md#palette): it reproduces the stored mixed-role constants, matches
//! an `f64` CSS Color 4 reference (`powf`/`cbrt`) to ±1 per 8-bit channel, and gives the
//! same bits on every call.

mod css_support;

use merlion_render::color::{cbrt, oklab_mix, oklab_to_rgba8, oklch_to_rgba8, Rgba8};
use merlion_render::svg::theme;

fn hex(s: &str) -> Rgba8 {
    Rgba8::from_hex(s).unwrap_or_else(|| panic!("hex {}", s))
}

#[test]
fn mixes_of_the_defaults_are_the_stored_constants() {
    let (fg, bg) = (hex(theme::FG), hex(theme::BG));
    for (pct, constant) in [
        (55.0, theme::MUTED),
        (45.0, theme::LINE),
        (4.0, theme::SURFACE),
        (22.0, theme::BORDER),
        (2.0, theme::CLUSTER_BG),
    ] {
        assert_eq!(oklab_mix(fg, bg, pct).to_hex(), constant, "{}%", pct);
    }
    assert_eq!(theme::MUTED, "#7b7d81");
    assert_eq!(theme::LINE, "#919497");
    assert_eq!(theme::SURFACE, "#f5f5f5");
    assert_eq!(theme::BORDER, "#c8c9cb");
    assert_eq!(theme::CLUSTER_BG, "#fafafa");
}

#[test]
fn endpoints_and_identity() {
    let (fg, bg) = (hex(theme::FG), hex(theme::BG));
    assert_eq!(oklab_mix(fg, bg, 100.0), fg);
    assert_eq!(oklab_mix(fg, bg, 0.0), bg);
    for c in [
        "#000000", "#ffffff", "#cf222e", "#1a7f37", "#9a6700", "#0969da", "#123456",
    ] {
        for p in [0.0, 14.0, 50.0, 75.0, 100.0] {
            assert_eq!(oklab_mix(hex(c), hex(c), p), hex(c), "{} {}", c, p);
        }
    }
}

/// xorshift64*, so the sample is fixed.
fn rng(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    state.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

#[test]
fn matches_the_f64_reference_within_one_per_channel() {
    let mut s = 0x9e37_79b9_7f4a_7c15u64;
    let mut exact = 0;
    let n = 20_000;
    for _ in 0..n {
        let a = rng(&mut s);
        let b = rng(&mut s);
        let p = (rng(&mut s) % 101) as f64;
        let ca = Rgba8::new((a >> 8) as u8, (a >> 16) as u8, (a >> 24) as u8, 255);
        let cb = Rgba8::new((b >> 8) as u8, (b >> 16) as u8, (b >> 24) as u8, 255);
        let got = oklab_mix(ca, cb, p);
        let want = css_support::rgba8(&format!(
            "color-mix(in oklab, {} {}%, {})",
            ca.to_hex(),
            p,
            cb.to_hex()
        ))
        .unwrap();
        let d = [
            got.r.abs_diff(want[0]),
            got.g.abs_diff(want[1]),
            got.b.abs_diff(want[2]),
        ];
        assert!(
            d.iter().all(|&x| x <= 1),
            "{:?} {:?} {}%: {:?} vs {:?}",
            ca,
            cb,
            p,
            got,
            want
        );
        exact += (d == [0, 0, 0]) as usize;
    }
    assert!(exact * 100 > n * 99, "only {} of {} exact", exact, n);
}

#[test]
fn alpha_is_premultiplied() {
    let a = Rgba8::new(255, 0, 0, 255);
    let t = Rgba8::new(0, 0, 0, 0);
    // A fully transparent side contributes no colour, only alpha.
    let m = oklab_mix(a, t, 50.0);
    assert_eq!((m.r, m.g, m.b), (255, 0, 0));
    assert_eq!(m.a, 128);
    assert_eq!(oklab_mix(t, t, 30.0), t);
}

#[test]
fn deterministic_bits() {
    let mut s = 7u64;
    for _ in 0..1000 {
        let x = (rng(&mut s) % 1_000_000) as f64 / 1_000.0;
        assert_eq!(cbrt(x).to_bits(), cbrt(x).to_bits());
        let y = cbrt(x);
        assert!((y * y * y - x).abs() <= 1e-12 * x.max(1.0), "{} {}", x, y);
    }
    assert_eq!(cbrt(0.0), 0.0);
    assert_eq!(cbrt(-8.0), -2.0);
    assert_eq!(cbrt(27.0), 3.0);
    assert!(cbrt(f64::NAN).is_finite());
    let a = oklab_mix(hex("#0969da"), hex("#f6f2ea"), 37.5);
    let b = oklab_mix(hex("#0969da"), hex("#f6f2ea"), 37.5);
    assert_eq!(a, b);
}

#[test]
fn nonfinite_and_out_of_range_percentages_do_not_panic() {
    let (fg, bg) = (hex(theme::FG), hex(theme::BG));
    assert_eq!(oklab_mix(fg, bg, 150.0), fg);
    assert_eq!(oklab_mix(fg, bg, -5.0), bg);
    assert_eq!(oklab_mix(fg, bg, f64::NAN), bg);
}

#[test]
fn oklab_and_oklch_inputs_convert_inside_srgb() {
    // oklab(0.628 0.2249 0.1258) is sRGB red; oklch(0.628 0.2577 29.23) the same colour.
    assert_eq!(
        oklab_to_rgba8(0.62796, 0.22486, 0.12585, 1.0),
        Some(Rgba8::new(255, 0, 0, 255))
    );
    assert_eq!(
        oklch_to_rgba8(0.62796, 0.25768, 29.2339, 1.0),
        Some(Rgba8::new(255, 0, 0, 255))
    );
    assert_eq!(
        oklab_to_rgba8(1.0, 0.0, 0.0, 1.0),
        Some(Rgba8::new(255, 255, 255, 255))
    );
    assert_eq!(
        oklab_to_rgba8(0.0, 0.0, 0.0, 0.5),
        Some(Rgba8::new(0, 0, 0, 128))
    );
    // Far outside sRGB.
    assert_eq!(oklch_to_rgba8(0.7, 0.4, 150.0, 1.0), None);
    assert_eq!(oklab_to_rgba8(f64::NAN, 0.0, 0.0, 1.0), None);
    // Hue wraps.
    assert_eq!(
        oklch_to_rgba8(0.62796, 0.25768, 29.2339 + 720.0, 1.0),
        oklch_to_rgba8(0.62796, 0.25768, 29.2339, 1.0)
    );
    // Every 8-bit grey round-trips through oklab.
    for v in 0..=255u8 {
        let g = Rgba8::new(v, v, v, 255);
        assert_eq!(oklab_mix(g, g, 50.0), g);
    }
}

#[test]
fn hex_round_trip() {
    assert_eq!(hex("#abc").to_hex(), "#aabbcc");
    assert_eq!(hex("#11223344").to_hex(), "#11223344");
    assert_eq!(Rgba8::from_hex("#12345"), None);
    assert_eq!(Rgba8::from_hex("red"), None);
}
