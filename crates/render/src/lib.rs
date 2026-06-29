//! Raster (PNG) and PDF backends for rpic.
//!
//! These consume the SVG produced by `rpic-core` and convert it with pure-Rust
//! libraries (no system dependencies), keeping `rpic-core` itself
//! dependency-free and WASM-friendly.
//!
//! - PNG: parse the SVG with `usvg`, rasterize with `resvg`/`tiny-skia`.
//! - PDF: parse with svg2pdf's `usvg`, convert with `svg2pdf`.
//!
//! Both load system fonts so attached text renders.

/// Rasterize an SVG string to PNG bytes at the given scale (1.0 = 96 dpi, the
/// SVG's native resolution).
pub fn to_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};

    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
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
    opt.fontdb_mut().load_system_fonts();
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
}
