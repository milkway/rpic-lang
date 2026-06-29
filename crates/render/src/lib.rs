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

/// Rasterize an SVG string to PNG bytes at the given scale (1.0 = 96 dpi, the
/// SVG's native resolution).
pub fn to_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};

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
