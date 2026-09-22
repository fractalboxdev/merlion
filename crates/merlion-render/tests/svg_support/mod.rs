//! Output checkers shared by the SVG tests (specs/svg-output.md, specs/security.md#output).
#![allow(dead_code)]

use merlion_render::svg::EMBED_FONT_FAMILY;
use merlion_render::text::embedded_font_css;

/// Minimal XML well-formedness: balanced tags, double-quoted attributes without `<`,
/// valid entities, no duplicate attributes, one root element.
pub fn assert_well_formed(xml: &str) {
    let b = xml.as_bytes();
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    let mut roots = 0;
    while i < b.len() {
        match b[i] {
            b'<' => {
                if xml[i..].starts_with("<!--") {
                    let end = xml[i..].find("-->").expect("unterminated comment");
                    i += end + 3;
                    continue;
                }
                let end = i + xml[i..].find('>').expect("unterminated tag");
                let tag = &xml[i + 1..end];
                if let Some(name) = tag.strip_prefix('/') {
                    let open = stack.pop().unwrap_or_else(|| panic!("stray </{}>", name));
                    assert_eq!(open, name, "mismatched close tag");
                } else {
                    let self_closing = tag.ends_with('/');
                    let body = tag.trim_end_matches('/');
                    let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
                    let name = &body[..name_end];
                    assert!(
                        !name.is_empty()
                            && name
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':'),
                        "bad tag name {:?}",
                        name
                    );
                    let attrs = parse_attrs(&body[name_end..]);
                    let mut names: Vec<&str> = attrs.iter().map(|(n, _)| n.as_str()).collect();
                    names.sort();
                    let before = names.len();
                    names.dedup();
                    assert_eq!(before, names.len(), "duplicate attribute in <{}>", name);
                    if stack.is_empty() {
                        roots += 1;
                    }
                    if !self_closing {
                        stack.push(name.to_string());
                    }
                }
                i = end + 1;
            }
            b'&' => {
                let end = i + xml[i..].find(';').expect("unterminated entity");
                let ent = &xml[i + 1..end];
                assert!(
                    matches!(ent, "amp" | "lt" | "gt" | "quot" | "apos" | "#39"),
                    "bad entity &{};",
                    ent
                );
                i = end + 1;
            }
            _ => i += 1,
        }
    }
    assert!(!xml.contains("]]>"), "]]> in text");
    assert!(stack.is_empty(), "unclosed: {:?}", stack);
    assert_eq!(roots, 1, "exactly one root element");
    for c in xml.chars() {
        let ok = matches!(c, '\t' | '\n')
            || (c >= ' ' && c != '\u{7f}' && !('\u{80}'..='\u{9f}').contains(&c));
        assert!(
            ok && c != '\u{FFFE}' && c != '\u{FFFF}',
            "forbidden char {:?}",
            c
        );
    }
}

/// `name="value"` pairs; panics on unquoted or malformed attributes.
pub fn parse_attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s.trim_start();
    while !rest.is_empty() {
        let eq = rest
            .find('=')
            .unwrap_or_else(|| panic!("attribute without value: {:?}", rest));
        let name = rest[..eq].trim();
        assert!(
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':'),
            "bad attribute name {:?}",
            name
        );
        let after = &rest[eq + 1..];
        assert!(after.starts_with('"'), "unquoted attribute {}", name);
        let close = after[1..].find('"').expect("unterminated attribute");
        let value = &after[1..1 + close];
        assert!(!value.contains('<'), "< inside attribute {}", name);
        out.push((name.to_string(), value.to_string()));
        rest = after[close + 2..].trim_start();
    }
    out
}

/// Every attribute of every tag, as (tag, name, value).
pub fn all_attrs(xml: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find('<') {
        let end = rest[i..].find('>').unwrap() + i;
        let tag = &rest[i + 1..end];
        if !tag.starts_with('/') && !tag.starts_with('!') {
            let body = tag.trim_end_matches('/');
            let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
            for (n, v) in parse_attrs(&body[name_end..]) {
                out.push((body[..name_end].to_string(), n, v));
            }
        }
        rest = &rest[end + 1..];
    }
    out
}

pub fn style_text(svg: &str) -> &str {
    let a = svg.find("<style>").expect("style") + 7;
    let b = svg.find("</style>").expect("/style");
    &svg[a..b]
}

/// Top-level selectors of a stylesheet, descending into `@supports`/`@media` blocks.
pub fn css_selectors(css: &str) -> Vec<String> {
    let mut sels = Vec::new();
    let mut depth_is_group = Vec::new();
    let mut cur = String::new();
    let mut in_rule = false;
    let mut quote = false;
    for c in css.chars() {
        if c == '"' {
            quote = !quote;
        }
        if quote {
            if !in_rule {
                cur.push(c);
            }
            continue;
        }
        match c {
            '{' if !in_rule => {
                let s = cur.trim().to_string();
                cur.clear();
                if s.starts_with("@supports") || s.starts_with("@media") {
                    depth_is_group.push(true);
                } else {
                    sels.push(s);
                    in_rule = true;
                }
            }
            '}' if in_rule => in_rule = false,
            '}' => {
                depth_is_group.pop();
            }
            _ if !in_rule => cur.push(c),
            _ => {}
        }
    }
    assert!(cur.trim().is_empty(), "trailing css {:?}", cur);
    sels
}

/// specs/svg-output.md: forbidden content and scoping. The one exemption is the
/// embedded-font `@font-face` pair of `font: "embed"` mode, byte-for-byte as the core
/// builds it; it may occur once, and every other `@font-face` or `url(data:…)` fails.
pub fn assert_safe(svg: &str, id: &str) {
    let allowed = embedded_font_css(EMBED_FONT_FAMILY);
    assert!(
        svg.matches(allowed.as_str()).count() <= 1,
        "embedded font repeated"
    );
    let stripped = svg.replacen(allowed.as_str(), "", 1);
    let svg = stripped.as_str();
    let lower = svg.to_ascii_lowercase();
    for bad in [
        "<script",
        "<foreignobject",
        "<iframe",
        "<image",
        "<use",
        "<animate",
        "<set",
        "xlink:href",
        "@import",
        "javascript:",
    ] {
        assert!(!lower.contains(bad), "forbidden {:?}", bad);
    }
    for (tag, name, value) in all_attrs(svg) {
        assert!(
            !name.to_ascii_lowercase().starts_with("on"),
            "event handler {} on <{}>",
            name,
            tag
        );
        if name == "style" {
            assert_eq!(tag, "svg", "style attribute outside the root");
            assert!(value.starts_with("max-width:"), "root style {:?}", value);
        }
        if name == "href" {
            assert_eq!(tag, "a");
        }
    }
    let mut rest = svg;
    let prefix = format!("url(#{}-", id);
    while let Some(i) = rest.find("url(") {
        assert!(
            rest[i..].starts_with(&prefix),
            "external url: {}",
            &rest[i..i + 30.min(rest.len() - i)]
        );
        rest = &rest[i + 4..];
    }
    assert_safe_style(style_text(svg), id);
}

/// The allowed prefixes of an embedded-style selector (specs/svg-output.md#embedded-style).
pub fn scoped_prefixes(id: &str) -> [String; 3] {
    [
        format!("#{} ", id),
        format!(":where(#{} ", id),
        format!("#{}:not(:is([data-theme=\"light\"] *)) ", id),
    ]
}

/// specs/security.md#output rules 2 and 3 on the embedded style: no `<`, `&` or `\`; `@`
/// only in `@supports`, `@media (prefers-color-scheme: dark)` and the embedded font; no
/// `url()`; every selector starts with an allowed prefix; the only declared custom
/// properties are the per-element `--merlion-tone` / `--merlion-dash` reset to `initial`.
pub fn assert_safe_style(css: &str, id: &str) {
    assert!(!css.contains('<') && !css.contains('&') && !css.contains('\\'));
    for bad in [
        ":root",
        "html",
        "body",
        "@font-face",
        "data:",
        "url(",
        "@import",
        "!important",
    ] {
        assert!(!css.contains(bad), "css contains {}", bad);
    }
    let mut rest = css;
    while let Some(i) = rest.find('@') {
        let r = &rest[i..];
        assert!(
            r.starts_with("@supports (color: color-mix(in oklab, #000, #fff)){")
                || r.starts_with("@media (prefers-color-scheme: dark){"),
            "at-rule {:?}",
            &r[..r.len().min(40)]
        );
        rest = &r[1..];
    }
    let prefixes = scoped_prefixes(id);
    // librsvg drops every rule that contains `:where()`; only the per-element token
    // reset, which a CSS-less renderer does not need, may use it.
    for sel in css_selectors(css) {
        assert!(
            !sel.contains(":where(") || sel.starts_with(&format!(":where(#{} .merlion-node, ", id)),
            "`:where()` outside the token reset: {:?}",
            sel
        );
    }
    for sel in css_selectors(css) {
        for part in sel.split(',') {
            let part = part.trim();
            assert!(
                prefixes.iter().any(|p| part.starts_with(p.as_str()))
                    || (sel.starts_with(&prefixes[1]) && part.starts_with(&format!("#{} ", id))),
                "unscoped selector {:?}",
                part
            );
        }
    }
    let mut rest = css;
    while let Some(i) = rest.find("--") {
        let r = &rest[i..];
        let before = rest[..i].chars().last();
        if matches!(before, Some('{') | Some(';')) {
            assert!(
                r.starts_with("--merlion-tone:initial;")
                    || r.starts_with("--merlion-dash:initial;"),
                "declares a custom property: {:?}",
                &r[..r.len().min(40)]
            );
        }
        rest = &r[2..];
    }
}
