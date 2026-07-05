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

/// Render pic source to an SVG string. NULL on error.
///
/// # Safety
/// `src` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_render_svg(src: *const c_char, circuits: c_int) -> *mut c_char {
    let Some(s) = (unsafe { as_str(src) }) else {
        return ptr::null_mut();
    };
    ensure_math_renderer();
    match rpic_core::render_svg_with_options(s, &opts(circuits)) {
        Ok(svg) => to_c_string(svg),
        Err(_) => ptr::null_mut(),
    }
}

/// Compile to the JSON `{svg, animations, diagnostics, warnings, objects}`
/// bundle (or `{error, error_info}`).
/// NULL only if `src` is null/invalid.
///
/// # Safety
/// `src` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_compile_json(src: *const c_char, circuits: c_int) -> *mut c_char {
    let Some(s) = (unsafe { as_str(src) }) else {
        return ptr::null_mut();
    };
    ensure_math_renderer();
    to_c_string(rpic_core::compile_json_with_options(s, &opts(circuits)))
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
    let Some(s) = (unsafe { as_str(src) }) else {
        return ptr::null_mut();
    };
    ensure_math_renderer();
    let svg = match rpic_core::render_svg_with_options(s, &opts(circuits)) {
        Ok(v) => v,
        Err(_) => return ptr::null_mut(),
    };
    match rpic_render::to_png(&svg, scale as f32) {
        Ok(bytes) => to_c_bytes(bytes, out_len),
        Err(_) => ptr::null_mut(),
    }
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
    let Some(s) = (unsafe { as_str(src) }) else {
        return ptr::null_mut();
    };
    ensure_math_renderer();
    let svg = match rpic_core::render_svg_with_options(s, &opts(circuits)) {
        Ok(v) => v,
        Err(_) => return ptr::null_mut(),
    };
    match rpic_render::to_pdf(&svg) {
        Ok(bytes) => to_c_bytes(bytes, out_len),
        Err(_) => ptr::null_mut(),
    }
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
}
