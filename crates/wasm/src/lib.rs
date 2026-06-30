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
