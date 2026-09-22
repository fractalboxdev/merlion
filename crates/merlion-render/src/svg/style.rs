//! The embedded `<style>` (specs/svg-output.md#embedded-style, #theming).
//!
//! Every rule starts with `#{id}`, so nothing matches outside the SVG. Rule order:
//!
//! 1. the text reset, verbatim from the spec, then the embedded font (`font: "embed"`);
//! 2. base rules, each reading its token with a literal fallback;
//! 3. `@supports (color: color-mix(in oklab, #000, #fff))` repeating only the declarations
//!    whose fallback chain contains a mixed role, now falling back to `color-mix`;
//! 4. source styles (`classDef`, then node `style`, then `linkStyle`), last so that at
//!    equal specificity they win over 2 and 3.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::numfmt::push_num;
use crate::options::FontMode;

use super::theme::{Role, FONT, FONT_MONO, FONT_SYSTEM, STROKE};

/// One base declaration: a CSS property reading either a colour role or a literal.
enum Value {
    Role(Role),
    Lit(String),
}

struct Rule {
    selector: &'static str,
    decls: Vec<(&'static str, Value)>,
}

fn stroke_var() -> String {
    let mut s = String::from("var(--merlion-stroke, ");
    push_num(&mut s, STROKE);
    s.push_str("px)");
    s
}

fn base_rules() -> Vec<Rule> {
    use Value::{Lit, Role as R};
    let lit = |s: &str| Lit(String::from(s));
    alloc::vec![
        Rule {
            selector: ".merlion-bg",
            decls: alloc::vec![("fill", R(Role::Bg))],
        },
        Rule {
            selector: ".merlion-label,#{id} .merlion-edge-text,#{id} .merlion-cluster-title",
            decls: alloc::vec![("text-anchor", lit("start")), ("white-space", lit("pre"))],
        },
        Rule {
            selector: ".merlion-label",
            decls: alloc::vec![("fill", R(Role::NodeText))],
        },
        Rule {
            selector: ".merlion-edge-text,#{id} .merlion-cluster-title",
            decls: alloc::vec![("fill", R(Role::Fg))],
        },
        Rule {
            selector: ".merlion-b",
            decls: alloc::vec![("font-weight", lit("600"))],
        },
        Rule {
            selector: ".merlion-i",
            decls: alloc::vec![("font-style", lit("italic"))],
        },
        Rule {
            selector: ".merlion-code",
            decls: alloc::vec![("font-family", lit(FONT_MONO))],
        },
        Rule {
            selector: ".merlion-node>.merlion-shape",
            decls: alloc::vec![
                ("fill", R(Role::NodeBg)),
                ("stroke", R(Role::NodeBorder)),
                ("stroke-width", Lit(stroke_var())),
            ],
        },
        Rule {
            selector: ".merlion-cluster>.merlion-cluster-box",
            decls: alloc::vec![
                ("fill", R(Role::ClusterBg)),
                ("stroke", R(Role::ClusterBorder)),
                ("stroke-width", Lit(stroke_var())),
            ],
        },
        Rule {
            selector: ".merlion-edge>.merlion-edge-path",
            decls: alloc::vec![
                ("fill", lit("none")),
                ("stroke", R(Role::Edge)),
                ("stroke-width", Lit(stroke_var())),
            ],
        },
        Rule {
            selector: ".merlion-edge>.merlion-thick",
            decls: alloc::vec![("stroke-width", Lit(format!("calc({} * 2)", stroke_var())))],
        },
        Rule {
            selector: ".merlion-edge>.merlion-dotted",
            decls: alloc::vec![("stroke-dasharray", lit("3 3"))],
        },
        Rule {
            selector: ".merlion-marker-fill",
            decls: alloc::vec![("fill", R(Role::Edge)), ("stroke", lit("none"))],
        },
        Rule {
            selector: ".merlion-marker-stroke",
            decls: alloc::vec![
                ("fill", lit("none")),
                ("stroke", R(Role::Edge)),
                ("stroke-width", lit("1.5px")),
            ],
        },
        Rule {
            selector: ".merlion-edge-label>.merlion-edge-label-bg",
            decls: alloc::vec![("fill", R(Role::EdgeLabelBg))],
        },
    ]
}

fn push_rule(out: &mut String, id: &str, selector: &str, body: &str) {
    out.push('#');
    out.push_str(id);
    out.push(' ');
    out.push_str(&selector.replace("{id}", id));
    out.push('{');
    out.push_str(body);
    out.push('}');
}

/// The text reset of specs/svg-output.md#embedded-style, verbatim apart from the
/// font stack (`system` mode) and the size (the measured `font_size`).
fn push_reset(out: &mut String, id: &str, font: FontMode, font_size: f64) {
    let family = if font == FontMode::System {
        FONT_SYSTEM
    } else {
        FONT
    };
    let mut size = String::new();
    push_num(&mut size, font_size);
    out.push('#');
    out.push_str(id);
    out.push_str(&format!(
        " text {{ font-family: var(--merlion-font, {}); font-size: var(--merlion-font-size, {}px); \
         font-weight: 400; font-style: normal; font-stretch: normal; \
         font-kerning: normal; font-variant-ligatures: none; \
         font-feature-settings: \"calt\" 0, \"liga\" 0; \
         letter-spacing: 0; word-spacing: 0; text-transform: none; }}",
        family, size
    ));
}

/// Source-style rules, already serialised: `(selector without the id prefix, body)`.
pub struct SourceRule {
    pub selector: String,
    pub body: String,
}

/// The full `<style>` text (unescaped: it contains no `<` or `&` by construction).
pub fn build(
    id: &str,
    font: FontMode,
    font_size: f64,
    font_css: Option<&str>,
    source: &[SourceRule],
) -> String {
    let mut out = String::new();
    push_reset(&mut out, id, font, font_size);
    if let Some(css) = font_css {
        out.push_str(css);
    }
    let rules = base_rules();
    for r in &rules {
        let mut body = String::new();
        for (prop, v) in &r.decls {
            body.push_str(prop);
            body.push(':');
            match v {
                Value::Role(role) => body.push_str(&role.var(false)),
                Value::Lit(s) => body.push_str(s),
            }
            body.push(';');
        }
        push_rule(&mut out, id, r.selector, &body);
    }
    out.push_str("@supports (color: color-mix(in oklab, #000, #fff)){");
    for r in &rules {
        let mut body = String::new();
        for (prop, v) in &r.decls {
            if let Value::Role(role) = v {
                if role.is_mixed() {
                    body.push_str(prop);
                    body.push(':');
                    body.push_str(&role.var(true));
                    body.push(';');
                }
            }
        }
        if !body.is_empty() {
            push_rule(&mut out, id, r.selector, &body);
        }
    }
    out.push('}');
    for r in source {
        push_rule(&mut out, id, &r.selector, &r.body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_rule_matches_the_spec() {
        let s = build("m1", FontMode::Link, 14.0, None, &[]);
        assert!(s.starts_with(
            "#m1 text { font-family: var(--merlion-font, Inter, ui-sans-serif, system-ui, sans-serif); \
             font-size: var(--merlion-font-size, 14px); font-weight: 400; font-style: normal; \
             font-stretch: normal; font-kerning: normal; font-variant-ligatures: none; \
             font-feature-settings: \"calt\" 0, \"liga\" 0; letter-spacing: 0; word-spacing: 0; \
             text-transform: none; }#m1 "
        ), "{}", s);
    }

    #[test]
    fn node_rule_reads_tokens_with_literal_fallbacks() {
        let s = build("m1", FontMode::Link, 14.0, None, &[]);
        assert!(s.contains(
            "#m1 .merlion-node>.merlion-shape{fill:var(--merlion-node-bg, var(--merlion-surface, #f5f5f5));\
             stroke:var(--merlion-node-border, var(--merlion-border, #c8c9cb));\
             stroke-width:var(--merlion-stroke, 1.25px);}"
        ), "{}", s);
    }

    #[test]
    fn color_mix_appears_only_inside_supports() {
        let s = build("m1", FontMode::Link, 14.0, None, &[]);
        let at = s.find("@supports").unwrap();
        assert!(!s[..at].contains("color-mix"));
        assert!(s[at..].contains(
            "#m1 .merlion-node>.merlion-shape{fill:var(--merlion-node-bg, var(--merlion-surface, \
             color-mix(in oklab, var(--merlion-fg, #1f2328) 4%, var(--merlion-bg, #ffffff))));"
        ), "{}", s);
    }

    #[test]
    fn no_custom_property_is_declared() {
        let s = build("m1", FontMode::Link, 14.0, None, &[]);
        assert!(!s.contains("{--") && !s.contains(";--"));
    }

    #[test]
    fn system_font_mode_uses_the_system_stack() {
        let s = build("m1", FontMode::System, 13.5, None, &[]);
        assert!(s.contains("var(--merlion-font, system-ui, sans-serif)"));
        assert!(s.contains("var(--merlion-font-size, 13.5px)"));
    }

    #[test]
    fn source_rules_come_last_and_are_scoped() {
        let src = [SourceRule {
            selector: String::from(".merlion-c-hot>.merlion-shape"),
            body: String::from("fill:red;"),
        }];
        let s = build("m1", FontMode::Link, 14.0, None, &src);
        assert!(
            s.ends_with("}#m1 .merlion-c-hot>.merlion-shape{fill:red;}"),
            "{}",
            s
        );
    }
}
