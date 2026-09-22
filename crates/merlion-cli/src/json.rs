//! Hand-written JSON serialiser for `render --json` and `--json-summary`
//! (zero dependencies, specs/supply-chain.md).

use merlion_render::{Diagnostic, RenderError, RenderResult};

/// Appends `s` as a JSON string literal (RFC 8259 §7). U+2028 and U+2029 are escaped
/// too, so the output is also a valid JavaScript string literal.
pub fn push_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_opt_str(out: &mut String, s: Option<&str>) {
    match s {
        Some(s) => push_str(out, s),
        None => out.push_str("null"),
    }
}

/// The JSON value of a `RenderError`, or `null`.
pub fn push_error(out: &mut String, e: Option<&RenderError>) {
    match e {
        None => out.push_str("null"),
        Some(RenderError::Parse) => out.push_str(r#"{"kind":"parse"}"#),
        Some(RenderError::UnsupportedDiagram { header }) => {
            out.push_str(r#"{"kind":"unsupported_diagram","header":"#);
            push_str(out, header);
            out.push('}');
        }
        Some(RenderError::TooLarge { what }) => {
            out.push_str(r#"{"kind":"too_large","what":"#);
            push_str(out, what);
            out.push('}');
        }
    }
}

/// `{severity, code, line, column, byte_start, byte_end, message, fix}`.
pub fn push_diagnostic(out: &mut String, d: &Diagnostic) {
    out.push_str(r#"{"severity":"#);
    push_str(out, d.severity.as_str());
    out.push_str(r#","code":"#);
    push_str(out, d.code);
    out.push_str(&format!(
        r#","line":{},"column":{},"byte_start":{},"byte_end":{},"message":"#,
        d.span.line, d.span.column, d.span.byte_start, d.span.byte_end
    ));
    push_str(out, &d.message);
    out.push_str(r#","fix":"#);
    match &d.fix {
        None => out.push_str("null"),
        Some(f) => {
            out.push_str(&format!(
                r#"{{"byte_start":{},"byte_end":{},"replacement":"#,
                f.span.byte_start, f.span.byte_end
            ));
            push_str(out, &f.replacement);
            out.push('}');
        }
    }
    out.push('}');
}

pub fn push_diagnostics(out: &mut String, ds: &[Diagnostic]) {
    out.push('[');
    for (i, d) in ds.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_diagnostic(out, d);
    }
    out.push(']');
}

/// `{svg, outline, diagnostics, fuel_used, error}`.
pub fn render_result(r: &RenderResult) -> String {
    let mut out = String::with_capacity(r.svg.as_ref().map_or(0, String::len) + 128);
    out.push_str(r#"{"svg":"#);
    push_opt_str(&mut out, r.svg.as_deref());
    out.push_str(r#","outline":"#);
    push_opt_str(&mut out, r.outline.as_deref());
    out.push_str(r#","diagnostics":"#);
    push_diagnostics(&mut out, &r.diagnostics);
    out.push_str(&format!(r#","fuel_used":{},"error":"#, r.fuel_used));
    push_error(&mut out, r.error.as_ref());
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use merlion_render::{Severity, Span};

    fn lit(s: &str) -> String {
        let mut out = String::new();
        push_str(&mut out, s);
        out
    }

    #[test]
    fn escapes_quotes_backslashes_and_controls() {
        assert_eq!(lit("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(lit("\n\r\t"), r#""\n\r\t""#);
        assert_eq!(lit("\u{1}\u{1f}"), r#""\u0001\u001f""#);
        assert_eq!(lit("é</>"), "\"é</>\"");
    }

    #[test]
    fn escapes_line_separators_for_js_embedding() {
        let bs = char::from(0x5c);
        assert_eq!(lit("\u{2028}\u{2029}"), format!("\"{bs}u2028{bs}u2029\""));
    }

    #[test]
    fn serialises_a_failed_result() {
        let r = RenderResult {
            svg: None,
            outline: None,
            diagnostics: vec![Diagnostic {
                severity: Severity::Error,
                code: "E003",
                span: Span {
                    line: 2,
                    column: 5,
                    byte_start: 7,
                    byte_end: 9,
                },
                message: "bad \"x\"".into(),
                fix: None,
            }],
            error: Some(RenderError::TooLarge { what: "input" }),
            fuel_used: 42,
        };
        assert_eq!(
            render_result(&r),
            concat!(
                r#"{"svg":null,"outline":null,"diagnostics":[{"severity":"error","code":"E003","#,
                r#""line":2,"column":5,"byte_start":7,"byte_end":9,"message":"bad \"x\"","fix":null}],"#,
                r#""fuel_used":42,"error":{"kind":"too_large","what":"input"}}"#
            )
        );
    }

    #[test]
    fn serialises_a_fix_and_success() {
        let r = RenderResult {
            svg: Some("<svg/>".into()),
            outline: Some("o".into()),
            diagnostics: vec![Diagnostic {
                severity: Severity::Repair,
                code: "R003",
                span: Span::default(),
                message: "m".into(),
                fix: Some(merlion_render::diag::Fix {
                    span: Span {
                        line: 1,
                        column: 1,
                        byte_start: 0,
                        byte_end: 3,
                    },
                    replacement: "\"".into(),
                }),
            }],
            error: None,
            fuel_used: 0,
        };
        let j = render_result(&r);
        assert!(j.starts_with(r#"{"svg":"<svg/>","outline":"o","#), "{j}");
        assert!(
            j.contains(r#""fix":{"byte_start":0,"byte_end":3,"replacement":"\""}"#),
            "{j}"
        );
        assert!(j.ends_with(r#""fuel_used":0,"error":null}"#), "{j}");
    }

    #[test]
    fn serialises_error_kinds() {
        let mut s = String::new();
        push_error(&mut s, Some(&RenderError::Parse));
        assert_eq!(s, r#"{"kind":"parse"}"#);
        s.clear();
        push_error(
            &mut s,
            Some(&RenderError::UnsupportedDiagram {
                header: "pie".into(),
            }),
        );
        assert_eq!(s, r#"{"kind":"unsupported_diagram","header":"pie"}"#);
    }
}
