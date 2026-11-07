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
    let opts = wasm_options(circuits, texlabels);
    rpic_core::compile_json_with_options(src, &opts)
}

/// Like [`compile_with`], but the bundle also carries the device-independent
/// `scene` draw list (docs/scene.md); `flatten` (inches) delivers curves as
/// polylines. The plain entry points above stay byte-identical — the scene is
/// computed only here.
#[wasm_bindgen]
pub fn compile_scene(
    src: &str,
    circuits: bool,
    texlabels: bool,
    flatten: Option<f64>,
    outline: bool,
) -> String {
    let opts = wasm_options(circuits, texlabels);
    let sopts = rpic_core::SceneOptions {
        flatten: flatten.filter(|f| f.is_finite() && *f > 0.0),
        outline,
        source: None,
    };
    rpic_core::compile_json_all_with_options(src, &opts, &sopts)
}

/// Like [`compile_with`], plus named preludes (`names[i]` ↔ `texts[i]`):
/// source units spliced after `circuits`/`texlabels`, in order, before `src`
/// — positions inside one carry `file` = its name; the user's own positions
/// never shift. Bad names (or mismatched lengths) come back as an
/// `invalid_options` error bundle. `scene` adds the scene key as
/// [`compile_scene`] does.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn compile_preludes(
    src: &str,
    circuits: bool,
    texlabels: bool,
    names: Vec<String>,
    texts: Vec<String>,
    scene: bool,
    flatten: Option<f64>,
    outline: bool,
) -> String {
    if names.len() != texts.len() {
        return invalid_options_json(&format!(
            "preludes: {} names but {} texts",
            names.len(),
            texts.len()
        ));
    }
    let mut opts = wasm_options(circuits, texlabels);
    opts.preludes = names
        .into_iter()
        .zip(texts)
        .map(|(n, t)| rpic_core::Prelude::new(n, t))
        .collect();
    if scene {
        let sopts = rpic_core::SceneOptions {
            flatten: flatten.filter(|f| f.is_finite() && *f > 0.0),
            outline,
            source: None,
        };
        rpic_core::compile_json_all_with_options(src, &opts, &sopts)
    } else {
        rpic_core::compile_json_with_options(src, &opts)
    }
}

/// `{error, error_info: {kind: "invalid_options"}, warnings: []}` — the
/// bundle's error shape, built by hand (the lean build has no serde).
fn invalid_options_json(msg: &str) -> String {
    let d = rpic_core::Diagnostic::new("invalid_options", msg);
    let mut quoted = String::from("\"");
    for c in msg.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            c if (c as u32) < 0x20 => quoted.push_str(&format!("\\u{:04x}", c as u32)),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    format!(
        "{{\"error\":{quoted},\"error_info\":{},\"warnings\":[]}}",
        rpic_core::diagnostic_json(&d)
    )
}

pub(crate) fn wasm_options(circuits: bool, texlabels: bool) -> rpic_core::CompileOptions {
    rpic_core::CompileOptions {
        circuits,
        texlabels,
        base: None,
        // wasm has no filesystem; make `copy "file"` fail with the policy
        // error instead of an opaque io error (`copy "circuits"` still works)
        includes: rpic_core::IncludePolicy::Deny,
        ..Default::default()
    }
}

/// geopic (`--features geo`, the `rpic_wasm_geo` npm artifact): pic drawings
/// as GeoJSON. Gated because it adds ~180 KB to the module (DECISIONS.md G8).
#[cfg(feature = "geo")]
mod geo;

#[cfg(test)]
mod tests {
    #[test]
    fn mismatched_prelude_lists_are_invalid_options() {
        let j = super::compile_preludes(
            "box",
            false,
            false,
            vec!["a".into(), "b".into()],
            vec!["box".into()],
            false,
            None,
            false,
        );
        assert!(j.contains("\"kind\":\"invalid_options\""), "{j}");
        assert!(
            j.contains("\"error\":\"preludes: 2 names but 1 texts\""),
            "{j}"
        );
    }
}
