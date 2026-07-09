//! Stable C ABI for rpic.
//!
//! String-returning functions return a NUL-terminated, heap-allocated C string
//! (free with [`rpic_free_string`]) or NULL on error. Byte-returning functions
//! write the length to `out_len` and return a heap buffer (free with
//! [`rpic_free_bytes`]) or NULL on error. All input strings must be valid
//! NUL-terminated UTF-8.
//!
//! Header: see `rpic.h`.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int};
use std::ptr;
use std::sync::Once;

static MATH_RENDERER: Once = Once::new();

fn ensure_math_renderer() {
    MATH_RENDERER.call_once(|| {
        rpic_core::set_math_renderer(rpic_render::math::render_math);
    });
}

/// Borrow a C string as `&str`, or `None` if null / not UTF-8.
///
/// # Safety
/// `p` must be null or a valid NUL-terminated string for the call's duration.
unsafe fn as_str<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

/// `circuits != 0` loads the embedded library as a compile option (not
/// prepended text), so diagnostic positions stay relative to the caller's
/// source.
fn opts(circuits: c_int) -> rpic_core::CompileOptions {
    rpic_core::CompileOptions {
        circuits: circuits != 0,
        ..Default::default()
    }
}

/// Full compile options for the `*_ex` entry points — the C mirror of
/// [`rpic_core::CompileOptions`], so C/R embedders can enable `texlabels`,
/// sandbox `copy "file"` includes, and set the include base directory (the
/// circuits-only functions above cover the common case and stay for ABI
/// stability). All fields default to the circuits-only behaviour when zeroed.
#[repr(C)]
pub struct RpicOptions {
    /// Load the embedded circuit-element library (nonzero = on).
    pub circuits: c_int,
    /// Inject `texlabels = 1` (nonzero = on); the source can still override it.
    pub texlabels: c_int,
    /// `copy "file"` policy: 0 = unrestricted (CLI default), 1 = sandboxed to
    /// `base`, 2 = deny all filesystem includes. Any other value = unrestricted.
    pub include_policy: c_int,
    /// Directory `copy "file"` resolves against; NULL = none.
    pub base: *const c_char,
}

/// Build [`CompileOptions`] from a C options struct (NULL = circuits-only
/// defaults).
///
/// # Safety
/// `ex` must be null or point to a valid `RpicOptions`; its `base`, if set,
/// must be a valid NUL-terminated string for the call.
unsafe fn opts_ex(ex: *const RpicOptions) -> rpic_core::CompileOptions {
    let Some(o) = (unsafe { ex.as_ref() }) else {
        return rpic_core::CompileOptions::default();
    };
    let includes = match o.include_policy {
        1 => rpic_core::IncludePolicy::SandboxedToBase,
        2 => rpic_core::IncludePolicy::Deny,
        _ => rpic_core::IncludePolicy::Unrestricted,
    };
    rpic_core::CompileOptions {
        circuits: o.circuits != 0,
        texlabels: o.texlabels != 0,
        base: unsafe { as_str(o.base) }.map(std::path::PathBuf::from),
        includes,
        ..Default::default()
    }
}

fn to_c_string(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

fn to_c_bytes(bytes: Vec<u8>, out_len: *mut usize) -> *mut u8 {
    let mut boxed = bytes.into_boxed_slice();
    let len = boxed.len();
    let p = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    if !out_len.is_null() {
        unsafe { *out_len = len };
    }
    p
}

// ---- internal implementations (shared by the plain and `_ex` entry points) --

fn render_svg_impl(src: *const c_char, o: &rpic_core::CompileOptions) -> *mut c_char {
    let Some(s) = (unsafe { as_str(src) }) else {
        return ptr::null_mut();
    };
    ensure_math_renderer();
    match rpic_core::render_svg_with_options(s, o) {
        Ok(svg) => to_c_string(svg),
        Err(_) => ptr::null_mut(),
    }
}

fn compile_json_impl(src: *const c_char, o: &rpic_core::CompileOptions) -> *mut c_char {
    let Some(s) = (unsafe { as_str(src) }) else {
        return ptr::null_mut();
    };
    ensure_math_renderer();
    to_c_string(rpic_core::compile_json_with_options(s, o))
}

fn render_png_impl(
    src: *const c_char,
    scale: c_double,
    o: &rpic_core::CompileOptions,
    out_len: *mut usize,
) -> *mut u8 {
    let Some(s) = (unsafe { as_str(src) }) else {
        return ptr::null_mut();
    };
    ensure_math_renderer();
    let svg = match rpic_core::render_svg_with_options(s, o) {
        Ok(v) => v,
        Err(_) => return ptr::null_mut(),
    };
    match rpic_render::to_png(&svg, scale as f32) {
        Ok(bytes) => to_c_bytes(bytes, out_len),
        Err(_) => ptr::null_mut(),
    }
}

fn render_pdf_impl(
    src: *const c_char,
    o: &rpic_core::CompileOptions,
    out_len: *mut usize,
) -> *mut u8 {
    let Some(s) = (unsafe { as_str(src) }) else {
        return ptr::null_mut();
    };
    ensure_math_renderer();
    let svg = match rpic_core::render_svg_with_options(s, o) {
        Ok(v) => v,
        Err(_) => return ptr::null_mut(),
    };
    match rpic_render::to_pdf(&svg) {
        Ok(bytes) => to_c_bytes(bytes, out_len),
        Err(_) => ptr::null_mut(),
    }
}

// ---- circuits-only entry points (unchanged ABI) ----------------------------

/// Render pic source to an SVG string. NULL on error.
///
/// # Safety
/// `src` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_render_svg(src: *const c_char, circuits: c_int) -> *mut c_char {
    render_svg_impl(src, &opts(circuits))
}

/// Compile to the JSON `{svg, animations, diagnostics, warnings, objects}`
/// bundle (or `{error, error_info}`).
/// NULL only if `src` is null/invalid.
///
/// # Safety
/// `src` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_compile_json(src: *const c_char, circuits: c_int) -> *mut c_char {
    compile_json_impl(src, &opts(circuits))
}

/// Render to PNG. Writes the length to `out_len`; returns the buffer or NULL.
///
/// # Safety
/// `src` must be valid NUL-terminated UTF-8; `out_len` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_render_png(
    src: *const c_char,
    scale: c_double,
    circuits: c_int,
    out_len: *mut usize,
) -> *mut u8 {
    render_png_impl(src, scale, &opts(circuits), out_len)
}

/// Render to PDF. Writes the length to `out_len`; returns the buffer or NULL.
///
/// # Safety
/// `src` must be valid NUL-terminated UTF-8; `out_len` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_render_pdf(
    src: *const c_char,
    circuits: c_int,
    out_len: *mut usize,
) -> *mut u8 {
    render_pdf_impl(src, &opts(circuits), out_len)
}

// ---- full-options entry points (`_ex`, mirror `RpicOptions`) ----------------

/// Like [`rpic_render_svg`] with full [`RpicOptions`] (NULL = circuits-off
/// defaults).
///
/// # Safety
/// `src` must be a valid NUL-terminated UTF-8 string; `ex` null or a valid
/// `RpicOptions` (its `base`, if set, a valid NUL-terminated string).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_render_svg_ex(
    src: *const c_char,
    ex: *const RpicOptions,
) -> *mut c_char {
    render_svg_impl(src, &unsafe { opts_ex(ex) })
}

/// Like [`rpic_compile_json`] with full [`RpicOptions`] (NULL = defaults).
///
/// # Safety
/// See [`rpic_render_svg_ex`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_compile_json_ex(
    src: *const c_char,
    ex: *const RpicOptions,
) -> *mut c_char {
    compile_json_impl(src, &unsafe { opts_ex(ex) })
}

/// Like [`rpic_render_png`] with full [`RpicOptions`] (NULL = defaults).
///
/// # Safety
/// See [`rpic_render_svg_ex`]; `out_len` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_render_png_ex(
    src: *const c_char,
    scale: c_double,
    ex: *const RpicOptions,
    out_len: *mut usize,
) -> *mut u8 {
    render_png_impl(src, scale, &unsafe { opts_ex(ex) }, out_len)
}

/// Like [`rpic_render_pdf`] with full [`RpicOptions`] (NULL = defaults).
///
/// # Safety
/// See [`rpic_render_svg_ex`]; `out_len` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_render_pdf_ex(
    src: *const c_char,
    ex: *const RpicOptions,
    out_len: *mut usize,
) -> *mut u8 {
    render_pdf_impl(src, &unsafe { opts_ex(ex) }, out_len)
}

/// Free a string returned by `rpic_render_svg` / `rpic_compile_json`.
///
/// # Safety
/// `p` must come from this library and be freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_free_string(p: *mut c_char) {
    if !p.is_null() {
        drop(unsafe { CString::from_raw(p) });
    }
}

/// Free a byte buffer returned by `rpic_render_png` / `rpic_render_pdf`.
///
/// # Safety
/// `p`/`len` must match a buffer from this library, freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_free_bytes(p: *mut u8, len: usize) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, len)) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string_from_owned_ptr(p: *mut c_char) -> String {
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { rpic_free_string(p) };
        s
    }

    #[test]
    fn c_api_render_svg_registers_texlabels_renderer() {
        let src = CString::new("texlabels = 1\nbox \"$-\\frac{T}{2}$\" wid 1 ht 0.7").unwrap();

        let svg = string_from_owned_ptr(unsafe { rpic_render_svg(src.as_ptr(), 0) });

        assert!(svg.contains("<svg x=\""), "{svg}");
        assert!(!svg.contains("frac"), "{svg}");
    }

    #[test]
    fn c_api_compile_json_registers_texlabels_renderer() {
        let src = CString::new("texlabels = 1\nbox \"$-\\frac{T}{2}$\" wid 1 ht 0.7").unwrap();

        let json = string_from_owned_ptr(unsafe { rpic_compile_json(src.as_ptr(), 0) });

        assert!(json.contains("<svg x=\\\""), "{json}");
        assert!(!json.contains("no math renderer"), "{json}");
        assert!(!json.contains("frac"), "{json}");
    }

    #[test]
    fn c_api_ex_exposes_texlabels_flag_and_deny_policy() {
        // #286: the `_ex` entry point can enable texlabels as a flag …
        let src = CString::new("box \"$-\\frac{T}{2}$\" wid 1 ht 0.7").unwrap();
        let opts = RpicOptions {
            circuits: 0,
            texlabels: 1,
            include_policy: 0,
            base: ptr::null(),
        };
        let svg = string_from_owned_ptr(unsafe { rpic_render_svg_ex(src.as_ptr(), &opts) });
        assert!(!svg.contains("frac"), "texlabels flag not applied: {svg}");

        // … and sandbox includes: policy 2 (Deny) turns a `copy "file"` into a
        // clean policy error instead of a filesystem read
        let inc = CString::new("copy \"/etc/hostname\"\nbox").unwrap();
        let deny = RpicOptions {
            circuits: 0,
            texlabels: 0,
            include_policy: 2,
            base: ptr::null(),
        };
        let json = string_from_owned_ptr(unsafe { rpic_compile_json_ex(inc.as_ptr(), &deny) });
        assert!(json.contains("error"), "expected an include error: {json}");

        // NULL options == circuits-off defaults (no panic)
        let plain = CString::new("box").unwrap();
        let svg = string_from_owned_ptr(unsafe { rpic_render_svg_ex(plain.as_ptr(), ptr::null()) });
        assert!(svg.contains("<svg"), "{svg}");
    }
}
