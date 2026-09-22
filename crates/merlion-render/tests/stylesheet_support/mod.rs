//! Checks compiled page CSS against the fixed shapes of specs/svg-output.md#stylesheet
//! and specs/security.md#output rule 3: allowed selectors only, known tokens only, and
//! literal values in each token's grammar.
#![allow(dead_code)]

fn is_theme_name(t: &str) -> bool {
    let b = t.as_bytes();
    !b.is_empty()
        && b.len() <= 32
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn is_class_name(n: &str) -> bool {
    let b = n.as_bytes();
    matches!(b.first(), Some(c) if c.is_ascii_alphabetic() || *c == b'_')
        && b.len() <= 64
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
}

fn theme_selector(s: &str) -> Option<&str> {
    let t = s.strip_prefix("[data-theme=\"")?.strip_suffix("\"]")?;
    is_theme_name(t).then_some(t)
}

fn role_selector(s: &str) -> bool {
    let s = match s.split_once(" .merlion ") {
        Some((theme, rest)) => {
            if theme_selector(theme).is_none() {
                return false;
            }
            rest
        }
        None => match s.strip_prefix(".merlion ") {
            Some(r) => r,
            None => return false,
        },
    };
    let name = s
        .strip_prefix(".merlion-cc-")
        .or_else(|| s.strip_prefix(".merlion-c-"));
    name.is_some_and(is_class_name)
}

fn is_hex(v: &str) -> bool {
    v.strip_prefix('#').is_some_and(|h| {
        (h.len() == 6 || h.len() == 8)
            && h.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    })
}

fn is_number(v: &str, lo: f64, hi: f64) -> bool {
    v.bytes().all(|c| c.is_ascii_digit() || c == b'.')
        && v.parse::<f64>().is_ok_and(|n| (lo..=hi).contains(&n))
}

fn value_ok(token: &str, v: &str, role: bool) -> bool {
    match token {
        "tone" => role && is_hex(v),
        "dash" => {
            role && (v == "none"
                || (!v.is_empty()
                    && v.split(' ').count() <= 8
                    && v.split(' ').all(|n| is_number(n, 0.0, 100.0))))
        }
        "stroke" => {
            !role
                && v.strip_suffix("px")
                    .is_some_and(|n| is_number(n, 0.0, 20.0))
        }
        t if t.starts_with("c-") => {
            let ok_name = ["-fill", "-stroke", "-color"]
                .iter()
                .any(|p| t.strip_suffix(p).is_some_and(|n| is_class_name(&n[2..])));
            !role && ok_name && (is_hex(v) || (v == "none" && !t.ends_with("-color")))
        }
        t => {
            let known = [
                "bg",
                "fg",
                "muted",
                "line",
                "surface",
                "border",
                "accent",
                "ok",
                "warn",
                "danger",
                "node-bg",
                "node-border",
                "node-text",
                "node-detail",
                "edge",
                "edge-label-bg",
                "cluster-bg",
                "cluster-border",
            ];
            let series = t
                .strip_prefix("series-")
                .and_then(|n| n.parse::<u8>().ok())
                .is_some_and(|n| (1..=8).contains(&n));
            !role && (known.contains(&t) || series) && is_hex(v)
        }
    }
}

/// Panics unless `css` is compiled page CSS in the fixed shape.
pub fn assert_page_css_safe(css: &str) {
    for bad in ['<', '>', '&', '\\', '\'', '!', '*', '/', ';'] {
        let n = css.matches(bad).count();
        if bad == ';' {
            assert_eq!(n, css.matches(";\n").count(), "stray ;");
        } else {
            assert_eq!(n, 0, "forbidden {:?} in {}", bad, css);
        }
    }
    assert!(!css.contains("url(") && !css.contains("@import"));
    let mut in_media = false;
    let mut in_rule: Option<bool> = None; // Some(is a role rule)
    for line in css.lines() {
        let (pad, close) = if in_media {
            ("    ", "  }")
        } else {
            ("  ", "}")
        };
        if in_rule.is_some() {
            if line == close {
                in_rule = None;
                continue;
            }
            let decl = line
                .strip_prefix(pad)
                .and_then(|d| d.strip_suffix(';'))
                .unwrap_or_else(|| panic!("declaration line {:?}", line));
            let (name, value) = decl.split_once(": ").expect("name: value");
            let token = name
                .strip_prefix("--merlion-")
                .unwrap_or_else(|| panic!("non-token {:?}", name));
            assert!(
                value_ok(token, value, in_rule == Some(true)),
                "value {:?} for {:?}",
                value,
                name
            );
            continue;
        }
        if in_media {
            if line == "}" {
                in_media = false;
                continue;
            }
            let sel = line
                .strip_prefix("  ")
                .and_then(|l| l.strip_suffix(" {"))
                .unwrap_or_else(|| panic!("media line {:?}", line));
            // The automatic dark theme, or a role under it (no named theme inside).
            let role = sel
                .strip_prefix(":root:not([data-theme]) ")
                .is_some_and(|r| r.starts_with(".merlion ") && role_selector(r));
            assert!(
                sel == ":root:not([data-theme])" || role,
                "media selector {:?}",
                sel
            );
            in_rule = Some(role);
            continue;
        }
        if line == "@media (prefers-color-scheme: dark) {" {
            in_media = true;
            continue;
        }
        let sel = line
            .strip_suffix(" {")
            .unwrap_or_else(|| panic!("line {:?}", line));
        let role = role_selector(sel);
        assert!(
            sel == ":root" || theme_selector(sel).is_some() || role,
            "selector {:?}",
            sel
        );
        in_rule = Some(role);
    }
    assert!(in_rule.is_none() && !in_media, "unterminated block");
}
