//! Label drawing (specs/svg-output.md#text). A label is one `<text>`; each line is a
//! `<tspan>` with explicit `x`/`y`; a line with more than one run nests one `<tspan>`
//! per run. Positions come only from the `LabelLayout`: every line is centred on the
//! label centre, runs are placed left to right from their measured widths with
//! `text-anchor: start`, and the baseline is `ascent` below each line's top.

use alloc::string::String;

use crate::numfmt::push_num;
use crate::text::{LabelLayout, Run, Weight};

use super::escape::push_escaped;

fn attr_num(out: &mut String, name: &str, v: f64) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    push_num(out, v);
    out.push('"');
}

fn safe(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Classes and presentation attributes of a formatted run.
fn run_format(out: &mut String, run: &Run) {
    let bold = run.weight == Weight::SemiBold;
    if !(bold || run.italic || run.code) {
        return;
    }
    out.push_str(" class=\"");
    let mut first = true;
    for (on, class) in [
        (bold, "merlion-b"),
        (run.italic, "merlion-i"),
        (run.code, "merlion-code"),
    ] {
        if on {
            if !first {
                out.push(' ');
            }
            out.push_str(class);
            first = false;
        }
    }
    out.push('"');
    if bold {
        out.push_str(" font-weight=\"600\"");
    }
    if run.italic {
        out.push_str(" font-style=\"italic\"");
    }
    if run.code {
        out.push_str(" font-family=\"");
        out.push_str(super::theme::FONT_MONO);
        out.push('"');
    }
}

/// Appends `<text class="{class}" fill="{fill}">…</text>` for `label` centred on (`cx`, `cy`).
/// Nothing is written for a label without lines.
pub fn push_label(
    out: &mut String,
    label: &LabelLayout,
    cx: f64,
    cy: f64,
    class: &str,
    fill: &str,
) {
    if label
        .lines
        .iter()
        .all(|l| l.runs.iter().all(|r| r.text.is_empty()))
    {
        return;
    }
    let (cx, cy) = (safe(cx), safe(cy));
    let top = cy - safe(label.height) / 2.0;
    out.push_str("<text class=\"");
    out.push_str(class);
    out.push_str("\" fill=\"");
    out.push_str(fill);
    out.push_str("\">");
    for (i, line) in label.lines.iter().enumerate() {
        let x0 = cx - safe(line.width) / 2.0;
        let y = top + i as f64 * safe(label.line_height) + safe(label.ascent);
        out.push_str("<tspan");
        attr_num(out, "x", x0);
        attr_num(out, "y", y);
        match line.runs.as_slice() {
            [run] => {
                run_format(out, run);
                out.push('>');
                push_escaped(out, &run.text);
            }
            runs => {
                out.push('>');
                let mut x = x0;
                for (k, run) in runs.iter().enumerate() {
                    out.push_str("<tspan");
                    if k > 0 {
                        attr_num(out, "x", x);
                    }
                    run_format(out, run);
                    out.push('>');
                    push_escaped(out, &run.text);
                    out.push_str("</tspan>");
                    x += safe(run.width);
                }
            }
        }
        out.push_str("</tspan>");
    }
    out.push_str("</text>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Line;
    use alloc::vec;

    fn run(t: &str, w: f64) -> Run {
        Run {
            text: String::from(t),
            weight: Weight::Regular,
            italic: false,
            code: false,
            width: w,
        }
    }

    fn layout(lines: alloc::vec::Vec<Line>) -> LabelLayout {
        LabelLayout {
            width: lines
                .iter()
                .fold(0.0, |a, l| if l.width > a { l.width } else { a }),
            height: 20.0 * lines.len() as f64,
            line_height: 20.0,
            ascent: 15.0,
            lines,
        }
    }

    #[test]
    fn single_line_is_centred() {
        let l = layout(vec![Line {
            runs: vec![run("Hello", 40.0)],
            width: 40.0,
        }]);
        let mut s = String::new();
        push_label(&mut s, &l, 100.0, 50.0, "merlion-label", "#000");
        assert_eq!(
            s,
            "<text class=\"merlion-label\" fill=\"#000\"><tspan x=\"80\" y=\"55\">Hello</tspan></text>"
        );
    }

    #[test]
    fn wrapped_lines_are_one_tspan_each() {
        let l = layout(vec![
            Line {
                runs: vec![run("one", 30.0)],
                width: 30.0,
            },
            Line {
                runs: vec![run("three", 50.0)],
                width: 50.0,
            },
        ]);
        let mut s = String::new();
        push_label(&mut s, &l, 100.0, 50.0, "c", "f");
        assert!(s.contains("<tspan x=\"85\" y=\"45\">one</tspan>"), "{}", s);
        assert!(
            s.contains("<tspan x=\"75\" y=\"65\">three</tspan>"),
            "{}",
            s
        );
    }

    #[test]
    fn runs_are_placed_from_their_widths() {
        let mut b = run("bold", 30.0);
        b.weight = Weight::SemiBold;
        let mut i = run("it", 10.0);
        i.italic = true;
        let mut c = run("x()", 20.0);
        c.code = true;
        let l = layout(vec![Line {
            runs: vec![run("a ", 10.0), b, i, c],
            width: 70.0,
        }]);
        let mut s = String::new();
        push_label(&mut s, &l, 35.0, 10.0, "c", "f");
        assert!(
            s.contains("<tspan x=\"0\" y=\"15\"><tspan>a </tspan>"),
            "{}",
            s
        );
        assert!(
            s.contains("<tspan x=\"10\" class=\"merlion-b\" font-weight=\"600\">bold</tspan>"),
            "{}",
            s
        );
        assert!(
            s.contains("<tspan x=\"40\" class=\"merlion-i\" font-style=\"italic\">it</tspan>"),
            "{}",
            s
        );
        assert!(
            s.contains("<tspan x=\"50\" class=\"merlion-code\" font-family=\"ui-monospace"),
            "{}",
            s
        );
    }

    #[test]
    fn hostile_text_is_escaped() {
        let l = layout(vec![Line {
            runs: vec![run("</text><script>alert(1)</script>", 10.0)],
            width: 10.0,
        }]);
        let mut s = String::new();
        push_label(&mut s, &l, 0.0, 0.0, "c", "f");
        assert!(!s.contains("<script"));
        assert!(s.contains("&lt;/text&gt;&lt;script&gt;"));
    }

    #[test]
    fn empty_label_writes_nothing() {
        let mut s = String::new();
        push_label(&mut s, &LabelLayout::default(), 0.0, 0.0, "c", "f");
        assert!(s.is_empty());
    }
}
