//! Raw `extern "C"` exports over the core (specs/integrations.md#fractalboxmerlion-wasm).
//!
//! No `wasm-bindgen`: strings cross the boundary as UTF-8 pointer-and-length pairs.
//!
//! - `alloc(len) -> ptr` and `dealloc(ptr, len)` manage input buffers the host fills.
//! - `render(src_ptr, src_len, opts_ptr, opts_len) -> ptr` and `check(…) -> ptr` return a
//!   result buffer: a `u32` little-endian byte length, then that many bytes of UTF-8 JSON.
//!   `render` gives `{svg, outline, diagnostics, fuel_used, error}` (the shape of
//!   `merlion render --json`); `check` gives `{diagnostics, error}`.
//! - `compile_stylesheet(css_ptr, css_len, opts_ptr, opts_len) -> ptr` returns
//!   `{css, palette, diagnostics, error}`: the compiled page CSS, the palette JSON of
//!   the chosen themes (`index.d.ts` `Palette`) and the compiler's diagnostics.
//! - `result_free(ptr)` releases a result buffer.
//!
//! The exports never free their inputs; the host deallocates them.

mod opts;

/// A hint larger than the core's 1 MiB input limit is ignored with `I022`, as in the CLI
/// (specs/integrations.md#file-handling).
const MAX_HINT_BYTES: usize = 1 << 20;

use merlion_render::numfmt::push_num;
use merlion_render::stylesheet::{self, Palette, PaletteTable, StylesheetLimits};
use merlion_render::{json, Diagnostic, RenderResult, Severity, Span};

/// `{"kind": kind, "message": message}`, for failures before the core runs.
fn boundary_error(kind: &str, message: &str) -> String {
    let mut s = String::from(r#"{"kind":"#);
    json::push_str(&mut s, kind);
    s.push_str(r#","message":"#);
    json::push_str(&mut s, message);
    s.push('}');
    s
}

/// A render result JSON whose `error` is a boundary error.
fn render_failure(kind: &str, message: &str) -> String {
    format!(
        r#"{{"svg":null,"outline":null,"diagnostics":[],"fuel_used":0,"error":{}}}"#,
        boundary_error(kind, message)
    )
}

/// The JSON of `render` for already-copied inputs.
pub fn render_json(src: &[u8], opts: &[u8]) -> String {
    let Ok(source) = core::str::from_utf8(src) else {
        return render_failure("invalid_input", "source is not UTF-8");
    };
    let mut options = match opts::render_options(opts) {
        Ok(o) => o,
        Err(msg) => return render_failure("invalid_options", &msg),
    };
    let mut pre = Vec::new();
    if options
        .hint
        .as_ref()
        .is_some_and(|h| h.len() > MAX_HINT_BYTES)
    {
        options.hint = None;
        pre.push(Diagnostic {
            severity: Severity::Info,
            code: "I022",
            span: Span::default(),
            message: "layout hint ignored: larger than 1 MiB".into(),
            fix: None,
        });
    }
    let mut result: RenderResult = merlion_render::render(source, &options);
    if !pre.is_empty() {
        pre.append(&mut result.diagnostics);
        result.diagnostics = pre;
    }
    json::render_result(&result)
}

/// The JSON of `check` for already-copied inputs. Only `strict` is read from the options.
pub fn check_json(src: &[u8], opts: &[u8]) -> String {
    let fail = |kind: &str, msg: &str| {
        format!(
            r#"{{"diagnostics":[],"error":{}}}"#,
            boundary_error(kind, msg)
        )
    };
    let Ok(source) = core::str::from_utf8(src) else {
        return fail("invalid_input", "source is not UTF-8");
    };
    let strict = match opts::render_options(opts) {
        Ok(o) => o.strict,
        Err(msg) => return fail("invalid_options", &msg),
    };
    let mut out = String::from(r#"{"diagnostics":"#);
    json::push_diagnostics(&mut out, &merlion_render::check(source, strict));
    out.push_str(r#","error":null}"#);
    out
}

/// The `roles`, `tones` and `clusterTones` members of one table; the dark table's are
/// named `dark`, `darkTones` and `darkClusterTones`.
fn push_table(out: &mut String, t: &PaletteTable, dark: bool) {
    let key = |k: &str| {
        if dark {
            match k {
                "roles" => String::from("dark"),
                "tones" => String::from("darkTones"),
                _ => String::from("darkClusterTones"),
            }
        } else {
            String::from(k)
        }
    };
    let mut roles: Vec<(String, String)> = Vec::new();
    for (r, c) in &t.colours {
        roles.push((r.name().into(), c.to_hex()));
    }
    for (n, c) in &t.series {
        roles.push((format!("series-{n}"), c.to_hex()));
    }
    if let Some(s) = t.stroke {
        let mut v = String::new();
        push_num(&mut v, s);
        roles.push(("stroke".into(), v));
    }
    for (n, p, c) in &t.classes {
        roles.push((
            format!("c-{}-{}", n, p.name()),
            c.map_or_else(|| "none".into(), |c| c.to_hex()),
        ));
    }
    json::push_str(out, &key("roles"));
    out.push_str(":{");
    for (i, (k, v)) in roles.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json::push_str(out, k);
        out.push(':');
        json::push_str(out, v);
    }
    out.push('}');
    for (name, cluster) in [("tones", false), ("clusterTones", true)] {
        out.push(',');
        json::push_str(out, &key(name));
        out.push_str(":{");
        for (i, tone) in t.tones.iter().filter(|x| x.cluster == cluster).enumerate() {
            if i > 0 {
                out.push(',');
            }
            json::push_str(out, &tone.name);
            out.push_str(":{");
            let mut first = true;
            if let Some(c) = tone.tone {
                out.push_str(r#""tone":"#);
                json::push_str(out, &c.to_hex());
                first = false;
            }
            if let Some(d) = &tone.dash {
                if !first {
                    out.push(',');
                }
                out.push_str(r#""dash":["#);
                for (j, n) in d.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    push_num(out, *n);
                }
                out.push(']');
            }
            out.push('}');
        }
        out.push('}');
    }
}

/// The `Palette` JSON of `index.d.ts`. The glue turns it back into the canonical string
/// the `palette` render option carries.
pub fn palette_json(p: &Palette) -> String {
    let mut out = String::from("{");
    push_table(&mut out, &p.light, false);
    if let Some(d) = &p.dark {
        out.push(',');
        push_table(&mut out, d, true);
    }
    out.push('}');
    out
}

/// The JSON of `compile_stylesheet` for already-copied inputs. Options: `theme`,
/// `autoDark` (theme names), `strict` (`W017`–`W019` become errors).
pub fn compile_stylesheet_json(css: &[u8], opts: &[u8]) -> String {
    let fail = |kind: &str, msg: &str| {
        format!(
            r#"{{"css":null,"palette":null,"diagnostics":[],"error":{}}}"#,
            boundary_error(kind, msg)
        )
    };
    let Ok(css) = core::str::from_utf8(css) else {
        return fail("invalid_input", "stylesheet is not UTF-8");
    };
    let (theme, auto_dark, strict) = match opts::stylesheet_options(opts) {
        Ok(o) => o,
        Err(msg) => return fail("invalid_options", &msg),
    };
    let (sheet, diags) = stylesheet::compile(css, &StylesheetLimits::default());
    let mut items = diags.items;
    if strict {
        for d in &mut items {
            if matches!(d.code, "W017" | "W018" | "W019") {
                d.severity = Severity::Error;
            }
        }
    }
    let sheet = sheet.filter(|_| !items.iter().any(|d| d.severity == Severity::Error));
    let mut out = String::from(r#"{"css":"#);
    let mut palette = None;
    match &sheet {
        Some(s) => {
            for t in [&theme, &auto_dark].into_iter().flatten() {
                if !s.theme_names().contains(&t.as_str()) {
                    return fail(
                        "unknown_theme",
                        &format!("the stylesheet defines no theme `{t}`"),
                    );
                }
            }
            palette = s.palette(theme.as_deref(), auto_dark.as_deref());
            json::push_str(&mut out, &s.to_css());
        }
        None => out.push_str("null"),
    }
    out.push_str(r#","palette":"#);
    match &palette {
        Some(p) => out.push_str(&palette_json(p)),
        None => out.push_str("null"),
    }
    out.push_str(r#","diagnostics":"#);
    json::push_diagnostics(&mut out, &items);
    out.push_str(r#","error":null}"#);
    out
}

/// Copies a host buffer into a slice.
///
/// # Safety
/// When `len > 0`, `ptr` must point to `len` readable bytes that stay valid for `'a`.
unsafe fn host_bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 || ptr.is_null() {
        return &[];
    }
    // SAFETY: guaranteed by the caller; the host filled this buffer from `alloc(len)`.
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

/// Moves `json` into a result buffer: `u32` LE length, then the bytes. Null when the
/// length does not fit in a `u32` (the glue reports it as `E001`).
fn into_result(json: String) -> *mut u8 {
    let Ok(len) = u32::try_from(json.len()) else {
        return std::ptr::null_mut();
    };
    let mut buf = Vec::with_capacity(json.len() + 4);
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(json.as_bytes());
    Box::into_raw(buf.into_boxed_slice()).cast::<u8>()
}

/// Allocates `len` bytes for the host to fill. Null when the allocator fails; a
/// zero-length request returns a dangling, never-dereferenced pointer.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let Ok(layout) = std::alloc::Layout::from_size_align(len, 1) else {
        return std::ptr::null_mut();
    };
    if len == 0 {
        return std::ptr::NonNull::<u8>::dangling().as_ptr();
    }
    // SAFETY: `layout` has a non-zero size.
    unsafe { std::alloc::alloc(layout) }
}

/// Frees a buffer from `alloc`.
///
/// # Safety
/// `ptr` must come from `alloc(len)` with the same `len`, and be freed once.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if len == 0 || ptr.is_null() {
        return;
    }
    if let Ok(layout) = std::alloc::Layout::from_size_align(len, 1) {
        // SAFETY: `ptr` was allocated by `alloc` with this layout (caller contract).
        unsafe { std::alloc::dealloc(ptr, layout) }
    }
}

/// Renders one diagram; returns a result buffer (see the module docs).
///
/// # Safety
/// Each pointer must address its length in readable bytes (any pointer when the length
/// is 0).
#[no_mangle]
pub unsafe extern "C" fn render(
    src_ptr: *const u8,
    src_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
) -> *mut u8 {
    // SAFETY: forwarded caller contract.
    let (src, opts) = unsafe { (host_bytes(src_ptr, src_len), host_bytes(opts_ptr, opts_len)) };
    into_result(render_json(src, opts))
}

/// Parses without rendering; returns a result buffer with `{diagnostics, error}`.
///
/// # Safety
/// As for [`render`].
#[no_mangle]
pub unsafe extern "C" fn check(
    src_ptr: *const u8,
    src_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
) -> *mut u8 {
    // SAFETY: forwarded caller contract.
    let (src, opts) = unsafe { (host_bytes(src_ptr, src_len), host_bytes(opts_ptr, opts_len)) };
    into_result(check_json(src, opts))
}

/// Compiles a stylesheet; returns a result buffer with `{css, palette, diagnostics,
/// error}`.
///
/// # Safety
/// As for [`render`].
#[no_mangle]
pub unsafe extern "C" fn compile_stylesheet(
    css_ptr: *const u8,
    css_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
) -> *mut u8 {
    // SAFETY: forwarded caller contract.
    let (css, opts) = unsafe { (host_bytes(css_ptr, css_len), host_bytes(opts_ptr, opts_len)) };
    into_result(compile_stylesheet_json(css, opts))
}

/// Frees a result buffer from `render`, `check` or `compile_stylesheet`. Null is a no-op.
///
/// # Safety
/// `ptr` must be a result buffer not yet freed.
#[no_mangle]
pub unsafe extern "C" fn result_free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: a result buffer starts with its u32 LE payload length and is exactly
    // `4 + len` bytes, allocated as a `Box<[u8]>` in `into_result`.
    unsafe {
        let len = u32::from_le_bytes(ptr.cast::<[u8; 4]>().read()) as usize;
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            ptr,
            len + 4,
        )));
    }
}

/// Test-only export that traps, so the JS glue's recovery path can be exercised.
#[cfg(feature = "test-trap")]
#[no_mangle]
pub extern "C" fn trap() {
    #[cfg(target_arch = "wasm32")]
    core::arch::wasm32::unreachable();
    #[cfg(not(target_arch = "wasm32"))]
    std::process::abort();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_json_has_the_cli_shape() {
        let j = render_json(b"not a diagram", b"");
        assert!(
            j.starts_with(r#"{"svg":null,"outline":null,"diagnostics":["#),
            "{j}"
        );
        assert!(j.contains(r#""severity":"error""#), "{j}");
        assert!(j.contains(r#""fuel_used":0,"error":{"kind":"#), "{j}");
    }

    #[test]
    fn invalid_utf8_and_options_are_errors_not_traps() {
        let j = render_json(b"\xff", b"");
        assert!(j.contains(r#""error":{"kind":"invalid_input""#), "{j}");
        let j = render_json(b"flowchart LR", b"{\"width\":-1}");
        assert!(
            j.contains(r#""error":{"kind":"invalid_options","message":"#),
            "{j}"
        );
        let j = check_json(b"x", b"[");
        assert!(j.contains(r#""kind":"invalid_options""#), "{j}");
    }

    #[test]
    fn auto_tone_false_matches_the_core_option_byte_for_byte() {
        let src = "flowchart LR\nsubgraph g [G]\n  d{D} --> c[(C)]\nend\nc --> s([S])";
        let off = merlion_render::render(
            src,
            &merlion_render::RenderOptions {
                auto_tone: false,
                ..Default::default()
            },
        );
        assert_eq!(
            render_json(src.as_bytes(), br#"{"autoTone":false}"#),
            json::render_result(&off)
        );
        assert!(!off.svg.unwrap().contains("merlion-auto"));
        let on = render_json(src.as_bytes(), b"");
        assert!(on.contains("merlion-auto"), "{on}");
    }

    #[test]
    fn oversized_hint_is_dropped_with_i022() {
        let hint = "a".repeat(MAX_HINT_BYTES + 1);
        let opts = format!(r#"{{"hint":"{hint}"}}"#);
        let j = render_json(b"not a diagram", opts.as_bytes());
        assert!(j.contains(r#""code":"I022""#), "{j}");
    }

    #[test]
    fn check_json_shape() {
        let j = check_json(b"not a diagram", br#"{"strict":true}"#);
        assert!(j.starts_with(r#"{"diagnostics":["#), "{j}");
        assert!(j.ends_with(r#","error":null}"#), "{j}");
    }

    const SHEET: &str =
        ":root { --merlion-bg: #fafafa; --merlion-stroke: 1.5px; --merlion-c-store-fill: none; }\n\
        [data-theme=\"dark\"] { --merlion-bg: #101418; }\n\
        .merlion-c-store { --merlion-tone: #b8408f; --merlion-dash: 4 2; }\n\
        .merlion-cc-zone { --merlion-tone: #1b98a6; }\n\
        [data-theme=\"dark\"] .merlion-c-store { --merlion-dash: none; }\n\
        .viewer { color: red; }\n";

    #[test]
    fn compile_stylesheet_json_shape() {
        let j = compile_stylesheet_json(SHEET.as_bytes(), br##"{"autoDark":"dark"}"##);
        assert!(j.starts_with(r##"{"css":":root {\n"##), "{j}");
        assert!(
            j.contains(r##""palette":{"roles":{"bg":"#fafafa","stroke":"1.5","c-store-fill":"none"},"tones":{"store":{"tone":"#b8408f","dash":[4,2]}},"clusterTones":{"zone":{"tone":"#1b98a6"}},"dark":{"bg":"#101418","stroke":"1.5","c-store-fill":"none"},"darkTones":{"store":{"tone":"#b8408f","dash":[]}},"darkClusterTones":{"zone":{"tone":"#1b98a6"}}}"##),
            "{j}"
        );
        assert!(j.contains(r##""code":"I032""##), "{j}");
        assert!(j.ends_with(r##""error":null}"##), "{j}");
        let j = compile_stylesheet_json(SHEET.as_bytes(), b"");
        assert!(j.contains(r##""tones":{"store":{"tone":"#b8408f","dash":[4,2]}},"clusterTones":{"zone":{"tone":"#1b98a6"}}},"diagnostics""##), "{j}");
    }

    #[test]
    fn compile_stylesheet_errors() {
        let j = compile_stylesheet_json(SHEET.as_bytes(), br#"{"theme":"nope"}"#);
        assert!(
            j.contains(r#""error":{"kind":"unknown_theme","message":"#),
            "{j}"
        );
        let j = compile_stylesheet_json(SHEET.as_bytes(), br#"{"them":"dark"}"#);
        assert!(j.contains(r#""kind":"invalid_options""#), "{j}");
        let j = compile_stylesheet_json(b"\xff", b"");
        assert!(j.contains(r#""kind":"invalid_input""#), "{j}");
        let j = compile_stylesheet_json(b":root { --merlion-font: x; }", br#"{"strict":true}"#);
        assert!(
            j.starts_with(
                r#"{"css":null,"palette":null,"diagnostics":[{"severity":"error","code":"W018""#
            ),
            "{j}"
        );
        let big = format!(":root{{--merlion-bg:#fff}}/*{}*/", "x".repeat(70_000));
        let j = compile_stylesheet_json(big.as_bytes(), b"");
        assert!(
            j.starts_with(
                r#"{"css":null,"palette":null,"diagnostics":[{"severity":"error","code":"E013""#
            ),
            "{j}"
        );
    }

    /// The palette JSON of `compileStylesheet`, converted by the glue to its canonical
    /// string, renders the same bytes as the CLI's in-process palette.
    #[test]
    fn palette_option_renders_like_the_cli() {
        let (sheet, _) = merlion_render::stylesheet::compile(SHEET, &Default::default());
        let p = sheet.unwrap().palette(Some("dark"), None).unwrap();
        let src = "flowchart LR\nA-->B\nclass A store\n";
        let native = merlion_render::render(
            src,
            &merlion_render::RenderOptions {
                palette: Some(p.clone()),
                ..Default::default()
            },
        );
        let mut opts = String::from(r#"{"palette":"#);
        json::push_str(&mut opts, &p.canonical());
        opts.push('}');
        assert_eq!(
            render_json(src.as_bytes(), opts.as_bytes()),
            json::render_result(&native)
        );
    }

    /// Reads a result buffer the way the JS glue does.
    fn read_result(ptr: *mut u8) -> String {
        assert!(!ptr.is_null());
        // SAFETY: `ptr` is a live result buffer from `render`/`check`: 4 length bytes
        // followed by that many bytes.
        let bytes = unsafe {
            let len = u32::from_le_bytes(*(ptr as *const [u8; 4])) as usize;
            std::slice::from_raw_parts(ptr.add(4), len).to_vec()
        };
        // SAFETY: `ptr` came from `render`/`check` and is freed once.
        unsafe { result_free(ptr) };
        String::from_utf8(bytes).unwrap()
    }

    fn put(bytes: &[u8]) -> (*mut u8, usize) {
        let p = alloc(bytes.len());
        if !bytes.is_empty() {
            assert!(!p.is_null());
            // SAFETY: `p` points to `bytes.len()` writable bytes from `alloc`.
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len()) };
        }
        (p, bytes.len())
    }

    #[test]
    fn abi_round_trip() {
        let (sp, sl) = put(b"not a diagram");
        let (op, ol) = put(b"{}");
        // SAFETY: the pointers and lengths come from `alloc` above.
        let r = unsafe { render(sp, sl, op, ol) };
        assert_eq!(read_result(r), render_json(b"not a diagram", b"{}"));
        // SAFETY: as above.
        let r = unsafe { check(sp, sl, op, ol) };
        assert_eq!(read_result(r), check_json(b"not a diagram", b"{}"));
        // SAFETY: each buffer is freed once with its allocation length.
        unsafe {
            dealloc(sp, sl);
            dealloc(op, ol);
        }
    }

    #[test]
    fn zero_length_inputs_need_no_allocation() {
        // SAFETY: a zero length never dereferences the pointer, null included.
        let r = unsafe { render(std::ptr::null_mut(), 0, std::ptr::null_mut(), 0) };
        assert!(read_result(r).contains(r#""error":{"kind":"#));
        // SAFETY: freeing null and zero-length buffers is a no-op.
        unsafe {
            dealloc(std::ptr::null_mut(), 0);
            dealloc(alloc(0), 0);
            result_free(std::ptr::null_mut());
        }
    }
}
