//! Raw `extern "C"` exports over the core (specs/integrations.md#fractalboxdevmerlion-wasm).
//!
//! No `wasm-bindgen`: strings cross the boundary as UTF-8 pointer-and-length pairs.
//!
//! - `alloc(len) -> ptr` and `dealloc(ptr, len)` manage input buffers the host fills.
//! - `render(src_ptr, src_len, opts_ptr, opts_len) -> ptr` and `check(…) -> ptr` return a
//!   result buffer: a `u32` little-endian byte length, then that many bytes of UTF-8 JSON.
//!   `render` gives `{svg, outline, diagnostics, fuel_used, error}` (the shape of
//!   `merlion render --json`); `check` gives `{diagnostics, error}`.
//! - `result_free(ptr)` releases a result buffer.
//!
//! The exports never free their inputs; the host deallocates them.

mod opts;

/// A hint larger than the core's 1 MiB input limit is ignored with `I022`, as in the CLI
/// (specs/integrations.md#file-handling).
const MAX_HINT_BYTES: usize = 1 << 20;

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

/// Frees a result buffer from `render` or `check`. Null is a no-op.
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
