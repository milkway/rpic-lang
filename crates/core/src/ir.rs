//! Intermediate representation: the placed-primitive tree produced by the
//! evaluator and consumed by the render backends. All coordinates are absolute,
//! in pic units (inches), y pointing up.

use crate::diagnostic::Diagnostic;
use crate::geom::{Bbox, Point};

/// A fully evaluated drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawing {
    pub shapes: Vec<Shape>,
    /// Render layer per shape. Lower layers are emitted first; equal layers keep
    /// source order and shape ids stable.
    pub shape_layers: Vec<i32>,
    /// rpic extension: CSS class hook per shape, emitted on the shape's
    /// `<g id="sN">` group. Inert unless the host document styles it; `None`
    /// keeps the group byte-identical to classic output.
    pub shape_classes: Vec<Option<String>>,
    /// Source span of the statement that produced each shape (`None` for
    /// shapes without one). Drives the per-object geometry export; never
    /// affects rendering.
    pub shape_spans: Vec<Option<crate::diagnostic::Span>>,
    pub bbox: Bbox,
    /// Global `linethick` in points after picture-wide sizing, used only for
    /// dpic-style backend prelude padding. Per-shape strokes keep their own
    /// unscaled point thickness.
    pub prelude_thick: f64,
    /// Extra canvas whitespace in inches. This is an rpic extension inspired by
    /// Pikchr: it affects native backend framing only, not pic geometry.
    pub canvas_margin: CanvasMargin,
    /// rpic extension: a fixed page rectangle (`canvas from … to …`) in model
    /// space. When set, the SVG viewBox is derived from it instead of the
    /// content bounds; content outside is clipped by the viewBox.
    pub canvas: Option<Bbox>,
    pub anims: Vec<Anim>,
    /// rpic extension (`animate scroll`): a timeline-level hint that the host
    /// should scrub the animation on scroll rather than autoplay. Surfaced in
    /// the compile bundle as top-level `scroll`; the host wires ScrollTrigger.
    pub anim_scroll: bool,
    /// Lines emitted by pic `print` statements, without trailing newlines.
    pub diagnostics: Vec<String>,
    /// Non-fatal compiler warnings for accepted but likely unintended input.
    pub warnings: Vec<Diagnostic>,
}

/// Extra whitespace around the rendered canvas, in internal inches.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CanvasMargin {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl CanvasMargin {
    pub fn horizontal(self) -> f64 {
        self.left + self.right
    }

    pub fn vertical(self) -> f64 {
        self.top + self.bottom
    }

    pub fn scale_by(&mut self, factor: f64) {
        self.top *= factor;
        self.right *= factor;
        self.bottom *= factor;
        self.left *= factor;
    }
}

/// A resolved animation entry. `shape` indexes into [`Drawing::shapes`]; the
/// SVG backend gives that shape the id `s{shape}` so the player can target it.
#[derive(Debug, Clone, PartialEq)]
pub struct Anim {
    pub shape: usize,
    pub effect: String,
    /// Absolute start time in seconds.
    pub start: f64,
    pub duration: f64,
    /// GSAP `repeat`: `-1` loops forever, `0` plays once.
    pub repeat: i64,
    /// GSAP `yoyo`: alternate direction each repeat.
    pub yoyo: bool,
    /// GSAP easing name overriding the per-effect default, if given.
    pub ease: Option<String>,
    /// For the `move` effect: index of the shape whose geometry to follow.
    /// The SVG backend gives it the id `s{path}`, the MotionPath target.
    pub path: Option<usize>,
    /// For the `highlight` effect: the CSS target colour (`to <colour>`).
    pub color: Option<String>,
    /// Play as an exit (reverse) rather than an entrance.
    pub out: bool,
    /// For the `slide` effect: the direction it enters from
    /// (`"left"`/`"right"`/`"up"`/`"down"`).
    pub from: Option<String>,
    /// For the `morph` effect: index of the shape whose geometry to morph into.
    /// The SVG backend gives it the id `s{morph}`, the MorphSVG target.
    pub morph: Option<usize>,
    /// For the `type` effect: split the label by whole words (`by word`) rather
    /// than by character. The SVG backend wraps each unit in a `.rpic-ch` tspan
    /// the player staggers.
    pub type_word: bool,
    /// For the `scramble` effect: a custom scramble charset (`by "01"`); `None`
    /// uses the plugin's `upperCase`. Present in the manifest only when set.
    pub scramble_chars: Option<String>,
}

/// Line dash style.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Dash {
    #[default]
    Solid,
    /// Dash/gap base length in inches.
    Dashed(f64),
    /// Explicit dot spacing in inches; `None` means dpic's stroke-relative default.
    Dotted(Option<f64>),
}

/// Fill specification.
#[derive(Debug, Clone, PartialEq)]
pub enum Fill {
    /// Gray level, pic convention: 0 = black, 1 = white.
    Gray(f64),
    /// A named/explicit color.
    Color(String),
}

/// Hatch fill pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct Hatch {
    pub cross: bool,
    /// Line angle in degrees, measured in pic coordinates.
    pub angle: f64,
    /// Distance between hatch lines in inches.
    pub sep: f64,
    /// Hatch stroke width in points.
    pub width: f64,
    pub color: String,
}

/// Two-stop linear gradient fill (rpic extension, PSTricks-inspired).
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub from: String,
    pub to: String,
    /// Angle in degrees, measured in pic coordinates: 0 = left to right,
    /// 90 = bottom to top (matching how `hatchangle` measures).
    pub angle: f64,
}

/// Visual style shared by all shapes.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    /// Stroke color (CSS), or `None` to use the default.
    pub stroke: Option<String>,
    pub fill: Option<Fill>,
    pub hatch: Option<Hatch>,
    pub gradient: Option<Gradient>,
    /// Fill opacity, applied only to filled or hatched regions.
    pub fill_opacity: Option<f64>,
    /// Whether open paths/splines/arcs should emit a filled area. `color` on an
    /// open object only changes the stroke; `fill` and `shaded` fill.
    pub fill_open: bool,
    pub dash: Dash,
    /// Stroke thickness in points; `None` = backend default.
    pub thick: Option<f64>,
    /// Invisible (used by `move` and `invis`): geometry is still available for
    /// placement and anchors, but is not drawn.
    pub invis: bool,
    /// Invisible geometry that still contributes to output bounds. Dpic uses
    /// this for `move`; explicit `invis` helpers remain bounds-neutral.
    pub invis_bounds: bool,
    /// Arrowhead dimensions in inches (`arrowht`/`arrowwid`), used when this
    /// shape carries arrowheads.
    pub arrow_ht: f64,
    pub arrow_wid: f64,
    /// Filled (solid triangle, `arrowhead=2`) vs open (two strokes,
    /// `arrowhead=0`).
    pub arrow_filled: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            stroke: None,
            fill: None,
            hatch: None,
            gradient: None,
            fill_opacity: None,
            fill_open: false,
            dash: Dash::Solid,
            thick: None,
            invis: false,
            invis_bounds: false,
            arrow_ht: 0.1,
            arrow_wid: 0.05,
            arrow_filled: true,
        }
    }
}

/// Which ends of a path carry an arrowhead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Arrowheads {
    #[default]
    None,
    Start,
    End,
    Both,
}

/// A line of attached text with placement hints.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub s: String,
    /// rpic `texlabels` extension: when set, this line is a typeset math
    /// formula and `s` keeps the original literal for fallback/diagnostics.
    pub math: Option<crate::math::MathSpan>,
    /// horizontal: -1 = ljust, 0 = center, +1 = rjust.
    pub halign: i8,
    /// vertical: +1 = above, 0 = center, -1 = below.
    pub valign: i8,
    /// Extra text-position offset in inches (`textoffset`).
    pub text_offset: f64,
    /// rpic extension: bold face (`bold`).
    pub bold: bool,
    /// rpic extension: italic face (`italic`).
    pub italic: bool,
    /// rpic extension: font family — `Some("monospace")` from `mono`, or the
    /// family given to `font "…"`. `None` follows the root `<svg>` family.
    pub family: Option<String>,
    /// rpic extension: explicit size in points (`fontsize`); `None` keeps
    /// classic sizing (11 pt attached, height-derived standalone).
    pub size_pt: Option<f64>,
    /// rpic extension: rotation in degrees, CCW (`rotated`); applied about
    /// the line's anchor point in the SVG.
    pub rotate: Option<f64>,
    /// rpic extension: `aligned` — rotate to the host segment's angle. Set
    /// during text collection; the linear-object eval resolves it into
    /// `rotate` once the segment's start/end are known.
    pub aligned: bool,
}

/// Classic label size in points — the baseline `fontsize` scales against.
pub(crate) const FONT_PT_CLASSIC: f64 = 11.0;

// ---- shared text metrics (#291) ---------------------------------------------
// The single source of truth for label sizing, consumed by BOTH the evaluator
// (layout/canvas bbox) and the SVG backend (rendered ink bounds). They must
// agree, or label bounds silently desync from label geometry — keep any font
// sizing change here, never in a per-module copy.

/// dpic's x-height : em ratio.
pub(crate) const DP_TEXT_RATIO: f64 = 0.66;
/// The classic label em in inches (11 pt at 72 pt/in).
pub(crate) const TEXT_EM_IN: f64 = FONT_PT_CLASSIC / 72.0;
/// Estimated average glyph advance, as a fraction of the em.
pub(crate) const TEXT_CHAR_W_RATIO: f64 = 0.6;
/// Line height, as a fraction of the em.
pub(crate) const TEXT_LINE_H_RATIO: f64 = 1.2;

impl TextLine {
    /// Width scale vs. the classic 11 pt regular estimate: explicit
    /// `fontsize` scales linearly; bold glyphs run ~5% wider.
    pub(crate) fn width_factor(&self) -> f64 {
        let size = self.size_pt.map_or(1.0, |pt| pt / FONT_PT_CLASSIC);
        let weight = if self.bold { 1.05 } else { 1.0 };
        size * weight
    }

    /// Height scale (explicit `fontsize` vs. classic 11 pt).
    pub(crate) fn height_factor(&self) -> f64 {
        self.size_pt.map_or(1.0, |pt| pt / FONT_PT_CLASSIC)
    }

    /// Estimated ink width in inches: a math span's measured width, or
    /// chars × average advance × the size/weight factor — the one estimator
    /// behind the evaluator's layout bbox and the SVG backend's ink bounds.
    pub(crate) fn ink_width_in(&self) -> f64 {
        match &self.math {
            Some(m) => m.width,
            None => {
                self.s.chars().count() as f64 * TEXT_CHAR_W_RATIO * TEXT_EM_IN * self.width_factor()
            }
        }
    }
}

/// A placed drawing primitive.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Box {
        c: Point,
        w: f64,
        h: f64,
        rad: f64,
        style: Style,
        text: Vec<TextLine>,
    },
    Circle {
        c: Point,
        r: f64,
        style: Style,
        text: Vec<TextLine>,
    },
    Ellipse {
        c: Point,
        w: f64,
        h: f64,
        style: Style,
        text: Vec<TextLine>,
    },
    /// Straight polyline (line / arrow / move).
    Path {
        pts: Vec<Point>,
        /// rpic extension: this path was closed with the `close` attribute.
        closed: bool,
        arrows: Arrowheads,
        style: Style,
        text: Vec<TextLine>,
    },
    Spline {
        pts: Vec<Point>,
        /// dpic spline tension. `None` = the classic pic quadratic B-spline
        /// (straight first/last half-segments, tangent at segment midpoints);
        /// `Some(t)` = dpic's tensioned cubic spline through `t`.
        tension: Option<f64>,
        arrows: Arrowheads,
        style: Style,
        text: Vec<TextLine>,
    },
    Arc {
        c: Point,
        r: f64,
        /// start and end angles in radians.
        a0: f64,
        a1: f64,
        cw: bool,
        arrows: Arrowheads,
        style: Style,
        text: Vec<TextLine>,
    },
    Brace {
        a: Point,
        b: Point,
        cubics: Vec<[Point; 4]>,
        label_at: Point,
        style: Style,
        text: Vec<TextLine>,
    },
    Text {
        at: Point,
        text: Vec<TextLine>,
        bbox: Bbox,
        w: f64,
        h: f64,
        standalone: bool,
    },
}

impl Shape {
    /// Whether the shape paints anything. `false` for `move`/`invis` helpers
    /// (their `style.invis` is set) and empty text. Used by `animate … stagger`
    /// to fan only across a block's visible children, skipping spines.
    pub fn is_visible(&self) -> bool {
        match self {
            Shape::Box { style, .. }
            | Shape::Circle { style, .. }
            | Shape::Ellipse { style, .. }
            | Shape::Path { style, .. }
            | Shape::Spline { style, .. }
            | Shape::Arc { style, .. }
            | Shape::Brace { style, .. } => !style.invis,
            Shape::Text { text, .. } => !text.is_empty(),
        }
    }
}
