//! Math-label rendering hook (rpic `texlabels` extension).
//!
//! The core stays free of any typesetting dependency: a renderer is a plain
//! function registered once per process (the CLI and bindings register the
//! RaTeX-backed implementation from `rpic-render`; the wasm build registers
//! nothing and math labels fall back to literal text). The hook is
//! deliberately neutral so an alternative backend — e.g. Typst + mitex, the
//! documented second choice in `docs/tex-labels.md` — can slot in without
//! touching the core.

use std::sync::OnceLock;

/// A typeset math label: a self-contained SVG fragment (glyph paths only,
/// rasterizable with no font database) plus exact metrics in inches.
#[derive(Debug, Clone, PartialEq)]
pub struct MathSpan {
    /// Complete `<svg …>` document for the formula, tight box, origin at the
    /// top-left. Embedded into the drawing as a nested `<svg>` element.
    pub svg: String,
    /// Advance width in inches.
    pub width: f64,
    /// Extent above the baseline in inches.
    pub height: f64,
    /// Extent below the baseline in inches.
    pub depth: f64,
}

/// Typeset `tex` (math mode, no delimiters) at the given font size in points.
pub type MathRenderFn = fn(tex: &str, font_pt: f64) -> Result<MathSpan, String>;

static MATH_RENDERER: OnceLock<MathRenderFn> = OnceLock::new();

/// Register the process-wide math renderer. The first call wins; later calls
/// are ignored (returns whether this call installed the renderer).
pub fn set_math_renderer(f: MathRenderFn) -> bool {
    MATH_RENDERER.set(f).is_ok()
}

/// The registered renderer, if any.
pub(crate) fn math_renderer() -> Option<MathRenderFn> {
    MATH_RENDERER.get().copied()
}
