//! WASM bindings for rpic.
//!
//! Exposes a single `compile` entry point that returns a JSON bundle
//! `{ "svg": "...", "animations": [...], "diagnostics": [...] }` (or
//! `{ "error": "..." }`). The browser playground injects the SVG and drives the
//! animations with GSAP.

use wasm_bindgen::prelude::*;

/// Math-enabled builds (`--features math`) register the RaTeX renderer once
/// at module init, so `texlabels` sources typeset `$…$` labels exactly like
/// the native CLI. The lean default build registers nothing and math labels
/// fall back to literal text plus a diagnostic.
#[cfg(feature = "math")]
#[wasm_bindgen(start)]
pub fn init_math() {
    rpic_core::set_math_renderer(rpic_render::math::render_math);
}

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
/// TeX math. Note: the default wasm build ships without the math renderer
/// (size budget), so with `texlabels` the labels fall back to literal text
/// and a diagnostic — use the math-enabled build (`--features math`, the
/// `rpic_wasm_math` npm artifact) to typeset them.
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
