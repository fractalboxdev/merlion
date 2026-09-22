//! The committed Inter WOFF2 subsets for `font: "embed"`
//! (specs/text-measurement.md#serving-the-font).
//!
//! `tools/fontgen/subset.sh` builds both files once, at development time, from the
//! same pinned inputs and coverage ranges as the metric tables, keeping only the
//! `kern` layout feature. The core only base64-encodes them into a `data:` URI.

use alloc::string::String;

/// Inter Regular (400), subset to the table coverage. OFL-1.1, see [`OFL_TEXT`].
pub const INTER_REGULAR_WOFF2: &[u8] = include_bytes!("../../assets/Inter-Regular.subset.woff2");
/// Inter SemiBold (600), subset to the table coverage. OFL-1.1, see [`OFL_TEXT`].
pub const INTER_SEMIBOLD_WOFF2: &[u8] = include_bytes!("../../assets/Inter-SemiBold.subset.woff2");
/// The SIL Open Font License text with Inter's copyright line.
pub const OFL_TEXT: &str = include_str!("../../assets/OFL.txt");

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with `=` padding (RFC 4648 §4).
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
    let sym = |v: u32| ALPHABET[(v & 63) as usize] as char;
    let mut chunks = bytes.chunks_exact(3);
    for c in &mut chunks {
        let n = (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32;
        out.push(sym(n >> 18));
        out.push(sym(n >> 12));
        out.push(sym(n >> 6));
        out.push(sym(n));
    }
    match *chunks.remainder() {
        [a] => {
            let n = (a as u32) << 16;
            out.push(sym(n >> 18));
            out.push(sym(n >> 12));
            out.push_str("==");
        }
        [a, b] => {
            let n = (a as u32) << 16 | (b as u32) << 8;
            out.push(sym(n >> 18));
            out.push(sym(n >> 12));
            out.push(sym(n >> 6));
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Keeps a family name to `[A-Za-z0-9 _-]` so it can sit inside a CSS string in an
/// inline `<style>` without escaping (specs/security.md); falls back to `Inter`.
fn safe_family(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-'))
        .take(64)
        .collect();
    let trimmed = s.trim();
    if trimmed.is_empty() {
        String::from("Inter")
    } else {
        String::from(trimmed)
    }
}

fn push_face(out: &mut String, family: &str, weight: u16, woff2: &[u8]) {
    out.push_str("@font-face{font-family:\"");
    out.push_str(family);
    out.push_str("\";font-style:normal;font-weight:");
    // Only 400 and 600 occur; written without `format!` to keep the output canonical.
    out.push_str(if weight == 600 { "600" } else { "400" });
    out.push_str(";font-display:block;src:url(data:font/woff2;base64,");
    out.push_str(&base64_encode(woff2));
    out.push_str(") format(\"woff2\")}");
}

/// Two `@font-face` rules (400 and 600) that load the embedded subsets under
/// `font_family_name`. The name should be unique to Merlion (not plain `Inter`),
/// because an inline SVG's `@font-face` is visible to the whole host document.
pub fn embedded_font_css(font_family_name: &str) -> String {
    let family = safe_family(font_family_name);
    let mut out = String::with_capacity(
        (INTER_REGULAR_WOFF2.len() + INTER_SEMIBOLD_WOFF2.len()) / 3 * 4 + 512,
    );
    push_face(&mut out, &family, 400, INTER_REGULAR_WOFF2);
    push_face(&mut out, &family, 600, INTER_SEMIBOLD_WOFF2);
    out
}

/// The OFL notice as an XML comment, for the SVG in `"embed"` mode
/// (specs/text-measurement.md#serving-the-font). XML forbids `--` inside a comment and
/// a `-` right before its end, so a space separates consecutive hyphens and ends the body.
pub fn ofl_xml_comment() -> String {
    let mut out = String::with_capacity(OFL_TEXT.len() + 64);
    out.push_str("<!--\nThe embedded font is a subset of Inter, a Modified Version under the SIL Open Font License 1.1.\n\n");
    let mut prev_hyphen = false;
    for c in OFL_TEXT.chars() {
        if c == '-' && prev_hyphen {
            out.push(' ');
        }
        prev_hyphen = c == '-';
        out.push(c);
    }
    out.push_str(" -->");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_rfc4648_vectors() {
        let cases: [(&[u8], &str); 7] = [
            (b"", ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ];
        for (input, want) in cases {
            assert_eq!(base64_encode(input), want);
        }
        assert_eq!(base64_encode(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[test]
    fn fonts_are_woff2() {
        assert_eq!(&INTER_REGULAR_WOFF2[..4], b"wOF2");
        assert_eq!(&INTER_SEMIBOLD_WOFF2[..4], b"wOF2");
    }

    #[test]
    fn css_has_two_faces_with_data_uris() {
        let css = embedded_font_css("Merlion Inter");
        assert_eq!(css.matches("@font-face{").count(), 2);
        assert!(css.contains("font-family:\"Merlion Inter\";"));
        assert!(css.contains("font-weight:400;"));
        assert!(css.contains("font-weight:600;"));
        assert_eq!(css.matches("data:font/woff2;base64,").count(), 2);
        assert!(css.contains(&base64_encode(INTER_REGULAR_WOFF2)));
    }

    #[test]
    fn family_name_cannot_break_out_of_css() {
        let css = embedded_font_css("x\";}</style><script>");
        assert!(css.starts_with("@font-face{font-family:\"xstylescript\";"));
        assert!(!css.contains('<'));
        assert!(embedded_font_css("\"\"").contains("font-family:\"Inter\";"));
    }

    #[test]
    fn ofl_comment_is_well_formed_xml() {
        let c = ofl_xml_comment();
        let body = &c[4..c.len() - 3];
        assert!(c.starts_with("<!--") && c.ends_with("-->"));
        assert!(!body.contains("--"));
        assert!(!body.ends_with('-'));
        assert!(c.contains("SIL OPEN FONT LICENSE Version 1.1"));
        assert!(c.contains("The Inter Project Authors"));
    }
}
