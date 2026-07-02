//! RaTeX-backed math renderer for the rpic `texlabels` extension.
//!
//! Register with [`rpic_core::set_math_renderer(render_math)`]. The hook in
//! the core is backend-neutral; `docs/tex-labels.md` records Typst + mitex as
//! the documented alternative should this backend ever need replacing.

use ratex_layout::engine::layout;
use ratex_layout::layout_options::LayoutOptions;
use ratex_layout::to_display::to_display_list;
use ratex_parser::parse;
use ratex_svg::{SvgOptions, render_to_svg};
use ratex_types::math_style::MathStyle;
use rpic_core::MathSpan;

/// Typeset `tex` (inline math, no `$` delimiters) at `font_pt` points into a
/// self-contained SVG fragment (KaTeX fonts embedded as glyph paths) plus
/// exact metrics in inches.
pub fn render_math(tex: &str, font_pt: f64) -> Result<MathSpan, String> {
    let nodes = parse(tex).map_err(|e| format!("{e}"))?;
    // `$…$` is inline math: use TeX text style, like LaTeX would.
    let opts = LayoutOptions {
        style: MathStyle::Text,
        ..Default::default()
    };
    let lb = layout(&nodes, &opts);
    let dl = to_display_list(&lb);
    // rpic's SVG space is 96 px/inch; one em of label text is font_pt points.
    let px_per_em = font_pt * 96.0 / 72.0;
    let svg = render_to_svg(
        &dl,
        &SvgOptions {
            font_size: px_per_em,
            padding: 0.0,
            embed_glyphs: true,
            ..Default::default()
        },
    );
    let em_in = font_pt / 72.0;
    Ok(MathSpan {
        svg,
        width: dl.width * em_in,
        height: dl.height * em_in,
        depth: dl.depth * em_in,
    })
}
