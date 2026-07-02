//! Raster (PNG) and PDF backends for rpic.
//!
//! These consume the SVG produced by `rpic-core` and convert it with pure-Rust
//! libraries (no system dependencies), keeping `rpic-core` itself
//! dependency-free and WASM-friendly.
//!
//! - PNG: parse the SVG with `usvg`, rasterize with `resvg`/`tiny-skia`.
//! - PDF: parse with svg2pdf's `usvg`, convert with `svg2pdf`.
//!
//! Text is rendered with a **bundled** font (the Go font, BSD-3-Clause)
//! registered as every default family, so attached labels rasterize identically
//! on any machine — no dependency on which fonts happen to be installed. See
//! `fonts/LICENSE`.

/// Bundled font used for all text (the SVG backend emits `font-family="sans-serif"`).
const EMBEDDED_FONT: &[u8] = include_bytes!("../fonts/Go-Regular.ttf");
/// The bundled font's internal family name.
const EMBEDDED_FONT_FAMILY: &str = "Go";

#[cfg(feature = "math")]
pub mod math;

/// Rasterize an SVG string to PNG bytes at the given scale (1.0 = 96 dpi, the
/// SVG's native resolution).
pub fn to_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};

    if !scale.is_finite() || scale <= 0.0 {
        return Err("scale must be a positive finite number".into());
    }

    let mut opt = usvg::Options::default();
    {
        let db = opt.fontdb_mut();
        db.load_font_data(EMBEDDED_FONT.to_vec());
        db.set_serif_family(EMBEDDED_FONT_FAMILY);
        db.set_sans_serif_family(EMBEDDED_FONT_FAMILY);
        db.set_monospace_family(EMBEDDED_FONT_FAMILY);
        db.set_cursive_family(EMBEDDED_FONT_FAMILY);
        db.set_fantasy_family(EMBEDDED_FONT_FAMILY);
    }
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;

    let size = tree.size();
    let w = ((size.width() * scale).ceil() as u32).max(1);
    let h = ((size.height() * scale).ceil() as u32).max(1);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("failed to allocate pixmap")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| e.to_string())
}

/// Convert an SVG string to PDF bytes.
pub fn to_pdf(svg: &str) -> Result<Vec<u8>, String> {
    use svg2pdf::usvg;

    let mut opt = usvg::Options::default();
    {
        let db = opt.fontdb_mut();
        db.load_font_data(EMBEDDED_FONT.to_vec());
        db.set_serif_family(EMBEDDED_FONT_FAMILY);
        db.set_sans_serif_family(EMBEDDED_FONT_FAMILY);
        db.set_monospace_family(EMBEDDED_FONT_FAMILY);
        db.set_cursive_family(EMBEDDED_FONT_FAMILY);
        db.set_fantasy_family(EMBEDDED_FONT_FAMILY);
    }
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;

    svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"20\" viewBox=\"0 0 40 20\"><rect x=\"2\" y=\"2\" width=\"36\" height=\"16\" fill=\"none\" stroke=\"black\"/></svg>";
    const SVG_TEXT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"40\" viewBox=\"0 0 120 40\" font-family=\"sans-serif\" font-size=\"16\"><text x=\"4\" y=\"24\">Hello world</text></svg>";

    #[test]
    fn png_has_magic_and_scales() {
        let one = to_png(SVG, 1.0).unwrap();
        assert_eq!(&one[..4], &[0x89, 0x50, 0x4E, 0x47]); // PNG signature
        let two = to_png(SVG, 2.0).unwrap();
        assert!(two.len() > one.len()); // larger raster at 2x
    }

    #[test]
    fn png_rejects_invalid_scale() {
        assert!(to_png(SVG, 0.0).is_err());
        assert!(to_png(SVG, f32::NAN).is_err());
    }

    #[test]
    fn gradient_fill_rasterizes_offline() {
        // The exact defs markup the rpic SVG backend emits for the `gradient`
        // extension must rasterize in resvg and convert in svg2pdf, so PNG and
        // PDF stay backend-stable with the SVG.
        const GRAD: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"20\" viewBox=\"0 0 40 20\"><defs><linearGradient id=\"grad0\" gradientUnits=\"objectBoundingBox\" x1=\"0\" y1=\"0.5\" x2=\"1\" y2=\"0.5\"><stop offset=\"0\" stop-color=\"black\"/><stop offset=\"1\" stop-color=\"white\"/></linearGradient></defs><rect x=\"2\" y=\"2\" width=\"36\" height=\"16\" fill=\"url(#grad0)\"/></svg>";
        const FLAT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"20\" viewBox=\"0 0 40 20\"><rect x=\"2\" y=\"2\" width=\"36\" height=\"16\" fill=\"gray\"/></svg>";
        let grad = to_png(GRAD, 1.0).unwrap();
        assert_eq!(&grad[..4], &[0x89, 0x50, 0x4E, 0x47]);
        // a real gradient compresses differently from a flat fill — if resvg
        // ignored the paint server, both rects would rasterize identically
        let flat = to_png(FLAT, 1.0).unwrap();
        assert_ne!(grad, flat);
        assert_eq!(&to_pdf(GRAD).unwrap()[..4], b"%PDF");
    }

    #[cfg(feature = "math")]
    #[test]
    fn math_fragment_root_is_unitless_px() {
        // RaTeX emits pt-suffixed root dimensions; the renderer must strip
        // them so the fragment scales 1:1 with its metrics (1pt = 4/3 px
        // would render formulas 33% larger than the layout box).
        let span = math::render_math("x", 11.0).unwrap();
        let head = &span.svg[..span.svg.find('>').unwrap()];
        assert!(!head.contains("pt\""), "{head}");
        // root width in px must match the metric width in inches * 96
        let w_attr: f64 = head
            .split("width=\"")
            .nth(1)
            .and_then(|t| t.split('\"').next())
            .unwrap()
            .parse()
            .unwrap();
        assert!((w_attr - span.width * 96.0).abs() < 0.01, "{w_attr} vs {}", span.width * 96.0);
    }

    #[cfg(feature = "math")]
    #[test]
    fn texlabels_math_renders_through_png_and_pdf() {
        // Full pipeline: RaTeX-typeset label -> nested SVG fragment ->
        // rasterized by resvg / converted by svg2pdf. Pins that the exact
        // markup the extension emits stays backend-stable.
        rpic_core::set_math_renderer(math::render_math);
        let src = "texlabels = 1\nbox \"$-\\\\frac{T}{2}$\" wid 1 ht 0.7";
        let d = rpic_core::compile(src).unwrap();
        let svg = rpic_core::to_svg(&d);
        assert!(svg.contains("<svg x=\""), "{svg}");
        assert!(!svg.contains("frac"), "raw TeX must not leak: {svg}");

        let png = to_png(&svg, 2.0).unwrap();
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
        // the math glyphs must actually paint: materially larger than the
        // same box with no label at all
        let blank_d = rpic_core::compile("box wid 1 ht 0.7").unwrap();
        let blank = to_png(&rpic_core::to_svg(&blank_d), 2.0).unwrap();
        assert!(
            png.len() > blank.len(),
            "png {} <= blank {}",
            png.len(),
            blank.len()
        );

        assert_eq!(&to_pdf(&svg).unwrap()[..4], b"%PDF");
    }

    #[test]
    fn pdf_has_magic() {
        let pdf = to_pdf(SVG).unwrap();
        assert_eq!(&pdf[..4], b"%PDF");
    }

    #[test]
    fn bundled_font_rasterizes_text() {
        // text must produce non-blank pixels using only the embedded font (no
        // reliance on system fonts) — the text PNG is materially larger than an
        // identically-sized blank one.
        let with_text = to_png(SVG_TEXT, 1.0).unwrap();
        let blank = to_png(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"40\" viewBox=\"0 0 120 40\"></svg>",
            1.0,
        )
        .unwrap();
        assert!(
            with_text.len() > blank.len() + 200,
            "text PNG {} vs blank {}",
            with_text.len(),
            blank.len()
        );
    }
}
