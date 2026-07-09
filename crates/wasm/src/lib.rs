//! WASM bindings for rpic.
//!
//! Exposes a single `compile` entry point that returns a JSON bundle
//! `{ "svg": "...", "animations": [...], "diagnostics": [...],
//! "warnings": [...] }` (or `{ "error": "...", "error_info": { ... } }`).
//! The browser playground injects the SVG and drives the animations with GSAP.

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

/// Compile pic source to a JSON `{svg, animations, diagnostics, warnings}`
/// bundle (or `{error, error_info}`).
#[wasm_bindgen]
pub fn compile(src: &str) -> String {
    // Force Deny like the other entry points: wasm has no filesystem, so a
    // `copy "file"` should fail with the clean policy error rather than the
    // opaque io error `compile_json`'s Unrestricted default would give (#286).
    // The reserved `copy "circuits"` still works under Deny.
    compile_with(src, false, false)
}

/// Like [`compile`], but with the native circuit-element library loaded
/// (so `resistor`, `and_gate`, … are available). Loaded as an option, not
/// prepended text: diagnostic positions stay relative to `src`.
#[wasm_bindgen]
pub fn compile_circuits(src: &str) -> String {
    compile_with(src, true, false)
}

/// Like [`compile`], with options: `circuits` loads the circuit-element
/// library; `texlabels` sets `texlabels = 1` so `$…$` labels are typeset as
/// TeX math. Note: the default wasm build ships without the math renderer
/// (size budget), so with `texlabels` the labels fall back to literal text
/// and a diagnostic — use the math-enabled build (`--features math`, the
/// `rpic_wasm_math` npm artifact) to typeset them.
#[wasm_bindgen]
pub fn compile_with(src: &str, circuits: bool, texlabels: bool) -> String {
    let opts = rpic_core::CompileOptions {
        circuits,
        texlabels,
        base: None,
        // wasm has no filesystem; make `copy "file"` fail with the policy
        // error instead of an opaque io error (`copy "circuits"` still works)
        includes: rpic_core::IncludePolicy::Deny,
        ..Default::default()
    };
    rpic_core::compile_json_with_options(src, &opts)
}
