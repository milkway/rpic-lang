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

/// Bundled fonts (the SVG backend emits `font-family="sans-serif"` at the
/// root; the rpic font attributes add `font-weight`/`font-style`/generic
/// `monospace` per `<text>`, so the bold/italic/mono faces ship too).
// All the embedded faces are used only by `load_embedded_fonts`, which is
// itself `raster`-gated — so gate the fonts too, or a math-only build (the
// wasm size split: `default-features = false, features = ["math"]`)
// `include_bytes!`s ~hundreds of KB of TTFs it never reads (#288).
#[cfg(feature = "raster")]
const EMBEDDED_FONT: &[u8] = include_bytes!("../fonts/Go-Regular.ttf");
#[cfg(feature = "raster")]
const EMBEDDED_FONT_BOLD: &[u8] = include_bytes!("../fonts/Go-Bold.ttf");
#[cfg(feature = "raster")]
const EMBEDDED_FONT_ITALIC: &[u8] = include_bytes!("../fonts/Go-Italic.ttf");
#[cfg(feature = "raster")]
const EMBEDDED_FONT_BOLD_ITALIC: &[u8] = include_bytes!("../fonts/Go-Bold-Italic.ttf");
#[cfg(feature = "raster")]
const EMBEDDED_FONT_MONO: &[u8] = include_bytes!("../fonts/Go-Mono.ttf");
#[cfg(feature = "raster")]
const EMBEDDED_FONT_MONO_BOLD: &[u8] = include_bytes!("../fonts/Go-Mono-Bold.ttf");
/// The bundled font's internal family name.
#[cfg(feature = "raster")]
const EMBEDDED_FONT_FAMILY: &str = "Go";
/// The bundled monospace family's internal name.
#[cfg(feature = "raster")]
const EMBEDDED_MONO_FAMILY: &str = "Go Mono";

#[cfg(feature = "math")]
pub mod math;

/// Default maximum PNG raster dimension, in pixels, for either axis.
#[cfg(feature = "raster")]
pub const DEFAULT_MAX_RASTER_DIMENSION: u32 = 32_768;

/// Default maximum PNG raster area, in pixels.
#[cfg(feature = "raster")]
pub const DEFAULT_MAX_RASTER_PIXELS: u64 = 64_000_000;

/// Limits applied before allocating a PNG raster surface.
#[cfg(feature = "raster")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RasterLimits {
    pub max_width: u32,
    pub max_height: u32,
    pub max_pixels: u64,
}

#[cfg(feature = "raster")]
impl RasterLimits {
    pub const fn new(max_width: u32, max_height: u32, max_pixels: u64) -> Self {
        Self {
            max_width,
            max_height,
            max_pixels,
        }
    }
}

#[cfg(feature = "raster")]
impl Default for RasterLimits {
    fn default() -> Self {
        Self {
            max_width: DEFAULT_MAX_RASTER_DIMENSION,
            max_height: DEFAULT_MAX_RASTER_DIMENSION,
            max_pixels: DEFAULT_MAX_RASTER_PIXELS,
        }
    }
}

/// Rasterize an SVG string to PNG bytes at the given scale (1.0 = 96 dpi, the
/// SVG's native resolution).
#[cfg(feature = "raster")]
fn load_embedded_fonts(db: &mut resvg::usvg::fontdb::Database) {
    for data in [
        EMBEDDED_FONT,
        EMBEDDED_FONT_BOLD,
        EMBEDDED_FONT_ITALIC,
        EMBEDDED_FONT_BOLD_ITALIC,
        EMBEDDED_FONT_MONO,
        EMBEDDED_FONT_MONO_BOLD,
    ] {
        db.load_font_data(data.to_vec());
    }
    db.set_serif_family(EMBEDDED_FONT_FAMILY);
    db.set_sans_serif_family(EMBEDDED_FONT_FAMILY);
    db.set_monospace_family(EMBEDDED_MONO_FAMILY);
    db.set_cursive_family(EMBEDDED_FONT_FAMILY);
    db.set_fantasy_family(EMBEDDED_FONT_FAMILY);
}

#[cfg(feature = "raster")]
pub fn to_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    to_png_with_limits(svg, scale, RasterLimits::default())
}

/// Rasterize an SVG string to PNG bytes with explicit raster allocation limits.
#[cfg(feature = "raster")]
pub fn to_png_with_limits(svg: &str, scale: f32, limits: RasterLimits) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};

    if !scale.is_finite() || scale <= 0.0 {
        return Err("scale must be a positive finite number".into());
    }

    let mut opt = usvg::Options::default();
    load_embedded_fonts(opt.fontdb_mut());
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;

    let size = tree.size();
    let (w, h) = checked_raster_size(size.width(), size.height(), scale, limits)?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("failed to allocate pixmap")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| e.to_string())
}

#[cfg(feature = "raster")]
fn checked_raster_size(
    svg_width: f32,
    svg_height: f32,
    scale: f32,
    limits: RasterLimits,
) -> Result<(u32, u32), String> {
    let width = checked_scaled_dimension(svg_width, scale, "width")?;
    let height = checked_scaled_dimension(svg_height, scale, "height")?;
    let pixels = u64::from(width) * u64::from(height);

    if width > limits.max_width || height > limits.max_height || pixels > limits.max_pixels {
        return Err(format!(
            "raster output exceeds configured pixel limit: {width}x{height} ({pixels} pixels) exceeds max {}x{} or {} pixels",
            limits.max_width, limits.max_height, limits.max_pixels
        ));
    }

    Ok((width, height))
}

#[cfg(feature = "raster")]
fn checked_scaled_dimension(value: f32, scale: f32, axis: &str) -> Result<u32, String> {
    let scaled = f64::from(value) * f64::from(scale);
    if !scaled.is_finite() {
        return Err(format!("raster {axis} is not finite"));
    }

    let pixels = scaled.ceil().max(1.0);
    if pixels > f64::from(u32::MAX) {
        return Err(format!(
            "raster output exceeds configured pixel limit: scaled {axis} {pixels:e} exceeds u32::MAX"
        ));
    }

    Ok(pixels as u32)
}

/// Convert an SVG string to PDF bytes.
#[cfg(feature = "raster")]
pub fn to_pdf(svg: &str) -> Result<Vec<u8>, String> {
    use svg2pdf::usvg;

    let mut opt = usvg::Options::default();
    load_embedded_fonts(opt.fontdb_mut());
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

    #[cfg(feature = "raster")]
    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"20\" viewBox=\"0 0 40 20\"><rect x=\"2\" y=\"2\" width=\"36\" height=\"16\" fill=\"none\" stroke=\"black\"/></svg>";
    #[cfg(feature = "raster")]
    const SVG_TEXT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"40\" viewBox=\"0 0 120 40\" font-family=\"sans-serif\" font-size=\"16\"><text x=\"4\" y=\"24\">Hello world</text></svg>";

    #[cfg(feature = "raster")]
    #[test]
    fn png_has_magic_and_scales() {
        let one = to_png(SVG, 1.0).unwrap();
        assert_eq!(&one[..4], &[0x89, 0x50, 0x4E, 0x47]); // PNG signature
        let two = to_png(SVG, 2.0).unwrap();
        assert!(two.len() > one.len()); // larger raster at 2x
    }

    #[cfg(feature = "raster")]
    #[test]
    fn png_rejects_invalid_scale() {
        assert!(to_png(SVG, 0.0).is_err());
        assert!(to_png(SVG, f32::NAN).is_err());
    }

    #[cfg(feature = "raster")]
    #[test]
    fn png_rejects_huge_scale_before_allocation() {
        let err = to_png(SVG, 100_000.0).unwrap_err();
        assert!(err.contains("raster output exceeds configured pixel limit"));
    }

    #[cfg(feature = "raster")]
    #[test]
    fn png_respects_custom_raster_limits() {
        let err = to_png_with_limits(SVG, 1.0, RasterLimits::new(100, 100, 100)).unwrap_err();
        assert!(err.contains("40x20 (800 pixels)"), "{err}");

        let png = to_png_with_limits(SVG, 1.0, RasterLimits::new(100, 100, 1_000)).unwrap();
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[cfg(feature = "raster")]
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
        assert!(
            (w_attr - span.width * 96.0).abs() < 0.01,
            "{w_attr} vs {}",
            span.width * 96.0
        );
    }

    #[cfg(all(feature = "math", feature = "raster"))]
    #[test]
    fn texlabels_math_renders_through_png_and_pdf() {
        // Full pipeline: RaTeX-typeset label -> nested SVG fragment ->
        // rasterized by resvg / converted by svg2pdf. Pins that the exact
        // markup the extension emits stays backend-stable.
        rpic_core::set_math_renderer(math::render_math);
        let src = "texlabels = 1\nbox \"$-\\frac{T}{2}$\" wid 1 ht 0.7";
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

    #[cfg(feature = "raster")]
    #[test]
    fn pdf_has_magic() {
        let pdf = to_pdf(SVG).unwrap();
        assert_eq!(&pdf[..4], b"%PDF");
    }

    #[cfg(feature = "raster")]
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
