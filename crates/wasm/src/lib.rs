//! WASM bindings for rpic.
//!
//! Exposes a single `compile` entry point that returns a JSON bundle
//! `{ "svg": "...", "animations": [...], "diagnostics": [...] }` (or
//! `{ "error": "..." }`). The browser playground injects the SVG and drives the
//! animations with GSAP.

use wasm_bindgen::prelude::*;

/// Compile pic source to a JSON `{svg, animations, diagnostics}` bundle (or
/// `{error}`).
#[wasm_bindgen]
pub fn compile(src: &str) -> String {
    rpic_core::compile_json(src)
}

/// Like [`compile`], but with the native circuit-element library prepended
/// (so `resistor`, `and_gate`, … are available).
#[wasm_bindgen]
pub fn compile_circuits(src: &str) -> String {
    rpic_core::compile_json(&format!("{}\n{}", rpic_core::CIRCUITS, src))
}

/// Like [`compile`], with options: `circuits` prepends the circuit-element
/// library; `texlabels` sets `texlabels = 1` so `$…$` labels are typeset as
/// TeX math. Note: this wasm build ships without the math renderer (size
/// budget), so with `texlabels` the labels currently fall back to literal
/// text and a diagnostic — the option exists for API parity and for future
/// math-enabled builds.
#[wasm_bindgen]
pub fn compile_with(src: &str, circuits: bool, texlabels: bool) -> String {
    let src = if circuits {
        format!("{}\n{}", rpic_core::CIRCUITS, src)
    } else {
        src.to_string()
    };
    let src = if texlabels {
        format!("texlabels = 1\n{}", src)
    } else {
        src
    };
    rpic_core::compile_json(&src)
}
