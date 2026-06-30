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

fn with_circuits(src: &str, circuits: c_int) -> String {
    if circuits != 0 {
        format!("{}\n{}", rpic_core::CIRCUITS, src)
    } else {
        src.to_string()
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
    match rpic_core::render_svg(&with_circuits(s, circuits)) {
        Ok(svg) => to_c_string(svg),
        Err(_) => ptr::null_mut(),
    }
}

/// Compile to the JSON `{svg, animations, diagnostics}` bundle (or `{error}`).
/// NULL only if `src` is null/invalid.
///
/// # Safety
/// `src` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rpic_compile_json(src: *const c_char, circuits: c_int) -> *mut c_char {
    let Some(s) = (unsafe { as_str(src) }) else {
        return ptr::null_mut();
    };
    to_c_string(rpic_core::compile_json(&with_circuits(s, circuits)))
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
    let svg = match rpic_core::render_svg(&with_circuits(s, circuits)) {
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
    let svg = match rpic_core::render_svg(&with_circuits(s, circuits)) {
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
