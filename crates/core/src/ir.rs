//! Intermediate representation: the placed-primitive tree produced by the
//! evaluator and consumed by the render backends. All coordinates are absolute,
//! in pic units (inches), y pointing up.

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
    pub bbox: Bbox,
    /// Global `linethick` in points after picture-wide sizing, used only for
    /// dpic-style backend prelude padding. Per-shape strokes keep their own
    /// unscaled point thickness.
    pub prelude_thick: f64,
    /// Extra canvas whitespace in inches. This is an rpic extension inspired by
    /// Pikchr: it affects native backend framing only, not pic geometry.
    pub canvas_margin: CanvasMargin,
    pub anims: Vec<Anim>,
    /// Lines emitted by pic `print` statements, without trailing newlines.
    pub diagnostics: Vec<String>,
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
    /// horizontal: -1 = ljust, 0 = center, +1 = rjust.
    pub halign: i8,
    /// vertical: +1 = above, 0 = center, -1 = below.
    pub valign: i8,
    /// Extra text-position offset in inches (`textoffset`).
    pub text_offset: f64,
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
