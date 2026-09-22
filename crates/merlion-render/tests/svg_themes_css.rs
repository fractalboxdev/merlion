//! Checks `packages/merlion-themes/merlion-themes.css` against the token table
//! (specs/svg-output.md#theming) and the semantic-zoom contract (specs/viewer.md).

const CSS: &str = include_str!("../../../packages/merlion-themes/merlion-themes.css");

#[test]
fn ships_one_selector_per_limit_rank_pair() {
    let mut n = 0;
    for limit in 0..16 {
        for rank in 0..16 {
            let sel = format!(
                "[data-merlion-rank-limit=\"{}\"] [data-merlion-rank=\"{}\"] text",
                limit, rank
            );
            let present =
                CSS.contains(&format!("{},", sel)) || CSS.contains(&format!("{} {{", sel));
            assert_eq!(present, rank > limit, "limit {} rank {}", limit, rank);
            n += present as usize;
        }
    }
    assert_eq!(n, 120);
}

#[test]
fn every_theme_sets_the_foundations_and_palette() {
    for sel in [
        ":root,\n[data-theme=\"light\"] {",
        ":root:not([data-theme]) {",
        "[data-theme=\"dark\"] {",
        "[data-theme=\"harbour\"] {",
        "[data-theme=\"lantern\"] {",
    ] {
        let at = CSS.find(sel).unwrap_or_else(|| panic!("missing {}", sel));
        let body = &CSS[at..at + CSS[at..].find('}').unwrap()];
        for token in ["--merlion-bg:", "--merlion-fg:", "--merlion-accent:"] {
            assert!(body.contains(token), "{} lacks {}", sel, token);
        }
        for i in 1..=8 {
            assert!(
                body.contains(&format!("--merlion-series-{}:", i)),
                "{} series {}",
                sel,
                i
            );
        }
    }
}

#[test]
fn light_defaults_match_the_core() {
    use merlion_render::svg::theme;
    assert!(CSS.contains(&format!("--merlion-bg: {};", theme::BG)));
    assert!(CSS.contains(&format!("--merlion-fg: {};", theme::FG)));
    assert!(CSS.contains(&format!("--merlion-accent: {};", theme::ACCENT)));
}

#[test]
fn only_known_tokens_are_set() {
    let known = ["bg", "fg", "accent"];
    for decl in CSS.split("--merlion-").skip(1) {
        let name: String = decl
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if !decl[name.len()..].starts_with(':') {
            continue; // mentioned in a comment
        }
        let ok = known.contains(&name.as_str())
            || name
                .strip_prefix("series-")
                .and_then(|n| n.parse::<u8>().ok())
                .is_some_and(|n| (1..=8).contains(&n));
        assert!(ok, "unknown token --merlion-{}", name);
    }
}
