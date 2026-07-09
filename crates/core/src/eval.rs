//! Evaluator: walks the [`crate::ast`] and produces a placed-primitive
//! [`Drawing`] using pic's positioning semantics.
//!
//! Model (Kernighan §3): a pen with a *current position* and *current
//! direction* walks the plane dropping primitives. Closed objects (box/circle/
//! ellipse/block) attach at the current point and advance by their extent;
//! open objects (line/arrow/move/spline/arc) trace from the current point in
//! the current direction. Labels, compass corners and ordinals
//! (`last`/`nth`) resolve against previously placed objects.
//!
//! Approximations (documented; refined later): `arc` renders a default quarter
//! turn.

use std::collections::{HashMap, HashSet};
use std::f64::consts::{FRAC_1_SQRT_2, PI};

use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::geom::{Bbox, Point};
use crate::ir::*;
use crate::token::{self, Corner, Dir, EnvVar, Func1, Func2, LineType, Prim};

/// An evaluation error. `info` carries the structured diagnostic when the
/// failure site had one at hand (a deferred-parse error's full [`Diagnostic`],
/// or a place reference's span); bindings surface it without re-deriving
/// positions from the message.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalError {
    pub msg: String,
    pub info: Option<Box<Diagnostic>>,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

type ER<T> = Result<T, EvalError>;

// Text metrics derive from the shared source of truth in `ir` (#291) — the
// SVG backend consumes the same constants, so the layout bbox and the
// rendered geometry cannot silently desync.
use crate::ir::{DP_TEXT_RATIO, TEXT_EM_IN as TEXT_EM};
/// Label font size in points, matching the SVG backend's `FONT_PT`.
const FONT_PT_MATH: f64 = crate::ir::FONT_PT_CLASSIC;
const TEXT_CHAR_W: f64 = crate::ir::TEXT_CHAR_W_RATIO * TEXT_EM;
const TEXT_LINE_H: f64 = crate::ir::TEXT_LINE_H_RATIO * TEXT_EM;
const TEXT_XHEIGHT: f64 = DP_TEXT_RATIO * TEXT_EM;
const DEFAULT_BRACE_DEPTH: f64 = 0.18;
const DEFAULT_BRACE_POS: f64 = 0.5;
const DEFAULT_HATCH_ANGLE: f64 = 45.0;
const DEFAULT_HATCH_SEP: f64 = 0.08;
const DEFAULT_HATCH_WIDTH: f64 = 0.8;

fn err<T>(msg: impl Into<String>) -> ER<T> {
    Err(EvalError {
        msg: msg.into(),
        info: None,
    })
}

/// An error that keeps its structured diagnostic (position, kind, hints).
fn err_diag<T>(d: Diagnostic) -> ER<T> {
    Err(diag_error(d))
}

fn diag_error(d: Diagnostic) -> EvalError {
    EvalError {
        msg: d.message.clone(),
        info: Some(Box::new(d)),
    }
}

/// A deferred-parse failure (an `if`/`for` body, `exec`, `sprintf` re-parse)
/// keeps the original ParseError's full diagnostic instead of flattening it
/// to a string.
fn parse_eval_error(e: crate::parser::ParseError) -> EvalError {
    EvalError {
        msg: e.to_string(),
        info: Some(Box::new(e.diagnostic())),
    }
}

fn unknown_label_error(name: &str, span: Option<&Span>) -> EvalError {
    let mut d = Diagnostic::new("unknown_label", format!("unknown label `{name}`")).found(name);
    if let Some(s) = span {
        d = d.at(s.clone());
    }
    diag_error(d)
}

fn ordinal_diagnostic(n: i64, available: usize, span: Option<&Span>) -> Diagnostic {
    let mut d = Diagnostic::new(
        "ordinal_out_of_range",
        format!("ordinal {n} out of range (available {available})"),
    )
    .found(n.to_string())
    .expected(format!("1..{available}"));
    if let Some(s) = span {
        d = d.at(s.clone());
    }
    d
}

fn finite(v: f64, context: &str) -> ER<f64> {
    if v.is_finite() {
        Ok(v)
    } else {
        err(format!("{context} produced non-finite numeric value"))
    }
}

/// Evaluate a parsed picture into a [`Drawing`].
pub fn eval(pic: &Picture) -> ER<Drawing> {
    eval_with_limits(pic, EvalLimits::default())
}

/// Evaluate a parsed picture with host-provided resource limits.
pub fn eval_with_limits(pic: &Picture, limits: EvalLimits) -> ER<Drawing> {
    let limits = limits.validate()?;
    let mut st = State::with_limits(limits);
    st.macros = pic.macros.clone();
    st.includes = pic.includes.clone();
    st.eval_stmts(&pic.stmts)?;
    let want_w = match &pic.width {
        Some(e) => Some(st.eval_expr(e)?),
        None => None,
    };
    let want_h = match &pic.height {
        Some(e) => Some(st.eval_expr(e)?),
        None => None,
    };
    let (maxw, maxh) = (st.env.get(EnvVar::Maxpswid), st.env.get(EnvVar::Maxpsht));
    let canvas_margin = st.canvas_margin()?;
    let mut d = Drawing {
        shapes: st.shapes,
        shape_layers: st.shape_layers,
        shape_classes: st.shape_classes,
        shape_spans: st.shape_spans,
        bbox: st.bbox,
        prelude_thick: st.env.get(EnvVar::Linethick),
        canvas_margin,
        canvas: st.canvas,
        anims: st.anims,
        anim_scroll: st.anim_scroll,
        diagnostics: st.diagnostics,
        warnings: st.warnings,
    };
    apply_ps_size(&mut d, want_w, want_h);
    clamp_to_maxps(&mut d, maxw, maxh);
    Ok(d)
}

/// Clamp the drawing to the `maxpswid`/`maxpsht` page bounds: if it exceeds
/// either, scale the whole picture down uniformly to fit (never up), matching
/// pic's PostScript page-fit behaviour.
fn clamp_to_maxps(d: &mut Drawing, maxw: f64, maxh: f64) {
    for _ in 0..4 {
        if d.bbox.is_empty() {
            return;
        }
        let (w, h) = (canvas_width(d), canvas_height(d));
        let mut factor = 1.0_f64;
        if maxw > 0.0 && w > maxw {
            factor = factor.min(maxw / w);
        }
        if maxh > 0.0 && h > maxh {
            factor = factor.min(maxh / h);
        }
        if factor >= 1.0 - 1e-9 {
            return;
        }
        for sh in &mut d.shapes {
            scale_shape(sh, factor);
        }
        scale_canvas(d, factor);
        d.prelude_thick *= factor;
        d.canvas_margin.scale_by(factor);
        d.bbox = drawing_painted_bbox(&d.shapes);
    }
}

fn scale_canvas(d: &mut Drawing, factor: f64) {
    if let Some(c) = d.canvas {
        let mut bb = Bbox::new();
        bb.add(Point::new(c.min.x * factor, c.min.y * factor));
        bb.add(Point::new(c.max.x * factor, c.max.y * factor));
        d.canvas = Some(bb);
    }
}

/// Apply `.PS <width> [<height>]` sizing: uniformly scale the whole drawing so
/// it matches the requested width (or height if only height is given). Font size
/// is left unchanged.
fn apply_ps_size(d: &mut Drawing, want_w: Option<f64>, want_h: Option<f64>) {
    if d.bbox.is_empty() {
        return;
    }
    let factor = match (want_w, want_h) {
        (Some(w), _) if w > 0.0 && d.bbox.width() > 0.0 => w / d.bbox.width(),
        (None, Some(h)) if h > 0.0 && d.bbox.height() > 0.0 => h / d.bbox.height(),
        _ => return,
    };
    if (factor - 1.0).abs() < 1e-9 {
        return;
    }
    for sh in &mut d.shapes {
        scale_shape(sh, factor);
    }
    scale_canvas(d, factor);
    d.prelude_thick *= factor;
    d.canvas_margin.scale_by(factor);
    d.bbox = drawing_painted_bbox(&d.shapes);
}

fn canvas_width(d: &Drawing) -> f64 {
    let raw = d.canvas.map_or_else(|| d.bbox.width(), |c| c.width());
    raw + d.canvas_margin.horizontal()
}

fn canvas_height(d: &Drawing) -> f64 {
    let raw = d.canvas.map_or_else(|| d.bbox.height(), |c| c.height());
    raw + d.canvas_margin.vertical()
}

/// The dimension variables that track `scale`.
const SCALED_VARS: [EnvVar; 23] = [
    EnvVar::Arcrad,
    EnvVar::Arrowht,
    EnvVar::Arrowwid,
    EnvVar::Boxht,
    EnvVar::Boxrad,
    EnvVar::Boxwid,
    EnvVar::Circlerad,
    EnvVar::Dashwid,
    EnvVar::Ellipseht,
    EnvVar::Ellipsewid,
    EnvVar::Lineht,
    EnvVar::Linewid,
    EnvVar::Moveht,
    EnvVar::Movewid,
    EnvVar::Textht,
    EnvVar::Textwid,
    EnvVar::Textoffset,
    EnvVar::Margin,
    EnvVar::Topmargin,
    EnvVar::Rightmargin,
    EnvVar::Bottommargin,
    EnvVar::Leftmargin,
    EnvVar::Dotrad,
];

// ---- environment variables -------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvalLimits {
    pub max_animation_seconds: f64,
    pub max_animation_repeat: i64,
    pub max_loop_iterations: u64,
    pub max_shapes: usize,
}

pub const DEFAULT_MAX_ANIMATION_SECONDS: f64 = 1_000_000.0;
pub const DEFAULT_MAX_ANIMATION_REPEAT: i64 = 1_000_000;
pub const DEFAULT_MAX_LOOP_ITERATIONS: u64 = 1_000_000;
pub const DEFAULT_MAX_SHAPES: usize = 1_000_000;

impl Default for EvalLimits {
    fn default() -> Self {
        Self {
            max_animation_seconds: DEFAULT_MAX_ANIMATION_SECONDS,
            max_animation_repeat: DEFAULT_MAX_ANIMATION_REPEAT,
            max_loop_iterations: DEFAULT_MAX_LOOP_ITERATIONS,
            max_shapes: DEFAULT_MAX_SHAPES,
        }
    }
}

impl EvalLimits {
    fn validate(self) -> ER<Self> {
        if !self.max_animation_seconds.is_finite() {
            return err("max_animation_seconds option must be finite");
        }
        if self.max_animation_seconds < 0.0 {
            return err("max_animation_seconds option must be non-negative");
        }
        if self.max_animation_repeat < 0 {
            return err("max_animation_repeat option must be non-negative");
        }
        Ok(self)
    }
}

#[derive(Clone)]
struct EnvVars {
    v: HashMap<u8, f64>,
}

fn ev_key(e: EnvVar) -> u8 {
    e as u8
}

impl EnvVars {
    fn new(limits: EvalLimits) -> Self {
        use EnvVar::*;
        let defaults = [
            (Arcrad, 0.25),
            (Arrowht, 0.1),
            (Arrowwid, 0.05),
            (Boxht, 0.5),
            (Boxrad, 0.0),
            (Boxwid, 0.75),
            (Circlerad, 0.25),
            (Dashwid, 0.05),
            (Ellipseht, 0.5),
            (Ellipsewid, 0.75),
            (Lineht, 0.5),
            (Linewid, 0.5),
            (Moveht, 0.5),
            (Movewid, 0.5),
            (Textht, (11.0 / 72.0) * DP_TEXT_RATIO),
            (Textoffset, 2.0 / 72.0),
            (Textwid, 0.0),
            (Arrowhead, 1.0),
            (Fillval, 0.5),
            (Linethick, 0.8),
            (Maxpsht, 11.0),
            (Maxpswid, 8.5),
            (Scale, 1.0),
            (Margin, 0.0),
            (Topmargin, 0.0),
            (Rightmargin, 0.0),
            (Bottommargin, 0.0),
            (Leftmargin, 0.0),
            (Texlabels, 0.0),
            (Dotrad, 0.035),
            (Maxanimrepeat, limits.max_animation_repeat as f64),
            (Maxanimseconds, limits.max_animation_seconds),
        ];
        let mut v = HashMap::new();
        for (e, d) in defaults {
            v.insert(ev_key(e), d);
        }
        EnvVars { v }
    }
    fn get(&self, e: EnvVar) -> f64 {
        *self.v.get(&ev_key(e)).unwrap_or(&0.0)
    }
    fn set(&mut self, e: EnvVar, val: f64) {
        self.v.insert(ev_key(e), val);
    }
}

// ---- placed-object bookkeeping ---------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum PKind {
    Box,
    Circle,
    Ellipse,
    Line,
    Move,
    Spline,
    Arc,
    Brace,
    Block,
    Text,
}

#[derive(Clone)]
struct Placed {
    kind: PKind,
    center: Point,
    bbox: Bbox,
    start: Point,
    end: Point,
    thick: f64,
    points: Vec<Point>,
    radius: f64,
    box_rad: f64,
    line_wid: f64,
    line_ht: f64,
    closed_path: bool,
    layer: i32,
    /// Index of the primary shape in `shapes` (None for point-only labels).
    shape: Option<usize>,
    /// For blocks: inner labels (sub-objects), translated into parent
    /// coordinates, so `B.A` / `last [].Outer` resolve. Empty otherwise.
    members: HashMap<String, Placed>,
    /// For blocks: the half-open range of child shape indices `[lo, hi)` the
    /// block drew, so `animate <block> … stagger` can fan across them. `None`
    /// for non-blocks and empty blocks.
    block_shapes: Option<(usize, usize)>,
}

impl Placed {
    fn corner(&self, c: Corner) -> Point {
        match self.kind {
            PKind::Circle | PKind::Ellipse => self.ellipse_corner(c),
            PKind::Line | PKind::Move | PKind::Spline => self.linear_corner(c),
            PKind::Brace => self.brace_corner(c),
            PKind::Arc => self.arc_corner(c),
            PKind::Box => self.box_corner(c),
            PKind::Block | PKind::Text => self.bbox_corner(c),
        }
    }

    fn bbox_corner(&self, c: Corner) -> Point {
        let (lo, hi) = (self.bbox.min, self.bbox.max);
        let mid = self.center;
        match c {
            Corner::N => Point::new(mid.x, hi.y),
            Corner::S => Point::new(mid.x, lo.y),
            Corner::E => Point::new(hi.x, mid.y),
            Corner::W => Point::new(lo.x, mid.y),
            Corner::Ne => Point::new(hi.x, hi.y),
            Corner::Se => Point::new(hi.x, lo.y),
            Corner::Nw => Point::new(lo.x, hi.y),
            Corner::Sw => Point::new(lo.x, lo.y),
            Corner::Center => mid,
            Corner::Start => self.start,
            Corner::End => self.end,
        }
    }

    fn ellipse_corner(&self, c: Corner) -> Point {
        let (rx, ry) = (self.bbox.width() / 2.0, self.bbox.height() / 2.0);
        let diag = |sx: f64, sy: f64| Point::new(sx * rx * FRAC_1_SQRT_2, sy * ry * FRAC_1_SQRT_2);
        let off = match c {
            Corner::N => Point::new(0.0, ry),
            Corner::S => Point::new(0.0, -ry),
            Corner::E => Point::new(rx, 0.0),
            Corner::W => Point::new(-rx, 0.0),
            Corner::Ne => diag(1.0, 1.0),
            Corner::Se => diag(1.0, -1.0),
            Corner::Nw => diag(-1.0, 1.0),
            Corner::Sw => diag(-1.0, -1.0),
            Corner::Center => Point::ZERO,
            Corner::Start => return self.start,
            Corner::End => return self.end,
        };
        self.center + off
    }

    fn linear_corner(&self, c: Corner) -> Point {
        if self.points.is_empty() {
            return self.bbox_corner(c);
        }
        if self.closed_path {
            return match c {
                Corner::Start => self.points[0],
                Corner::End => *self.points.last().unwrap(),
                _ => self.bbox_corner(c),
            };
        }
        match c {
            Corner::Center => (self.points[0] + *self.points.last().unwrap()) * 0.5,
            Corner::Start => self.points[0],
            Corner::End => *self.points.last().unwrap(),
            _ => {
                let mut best = self.points[0];
                for p in self.points.iter().skip(1) {
                    let better = match c {
                        Corner::N => p.y > best.y,
                        Corner::S => p.y < best.y,
                        Corner::E => p.x > best.x,
                        Corner::W => p.x < best.x,
                        Corner::Ne => {
                            (p.y > best.y && p.x >= best.x) || (p.y >= best.y && p.x > best.x)
                        }
                        Corner::Se => {
                            (p.y < best.y && p.x >= best.x) || (p.y <= best.y && p.x > best.x)
                        }
                        Corner::Sw => {
                            (p.y < best.y && p.x <= best.x) || (p.y <= best.y && p.x < best.x)
                        }
                        Corner::Nw => {
                            (p.y > best.y && p.x <= best.x) || (p.y >= best.y && p.x < best.x)
                        }
                        Corner::Center | Corner::Start | Corner::End => false,
                    };
                    if better {
                        best = *p;
                    }
                }
                best
            }
        }
    }

    fn brace_corner(&self, c: Corner) -> Point {
        match c {
            Corner::Center => self.center,
            Corner::Start => self.start,
            Corner::End => self.end,
            _ => {
                let mut bb = Bbox::new();
                for p in &self.points {
                    bb.add(*p);
                }
                if bb.is_empty() {
                    return self.bbox_corner(c);
                }
                bbox_point(&bb, c)
            }
        }
    }

    fn arc_corner(&self, c: Corner) -> Point {
        let diag = self.radius * FRAC_1_SQRT_2;
        let off = match c {
            Corner::N => Point::new(0.0, self.radius),
            Corner::S => Point::new(0.0, -self.radius),
            Corner::E => Point::new(self.radius, 0.0),
            Corner::W => Point::new(-self.radius, 0.0),
            Corner::Ne => Point::new(diag, diag),
            Corner::Se => Point::new(diag, -diag),
            Corner::Nw => Point::new(-diag, diag),
            Corner::Sw => Point::new(-diag, -diag),
            Corner::Center => Point::ZERO,
            Corner::Start => return self.start,
            Corner::End => return self.end,
        };
        self.center + off
    }

    fn box_corner(&self, c: Corner) -> Point {
        match c {
            Corner::Ne | Corner::Se | Corner::Nw | Corner::Sw if self.box_rad > 0.0 => {
                let inset = self
                    .box_rad
                    .min(self.bbox.width().abs().min(self.bbox.height().abs()) / 2.0)
                    * (1.0 - FRAC_1_SQRT_2);
                let x = self.bbox.width() / 2.0 - inset;
                let y = self.bbox.height() / 2.0 - inset;
                let off = match c {
                    Corner::Ne => Point::new(x, y),
                    Corner::Se => Point::new(x, -y),
                    Corner::Nw => Point::new(-x, y),
                    Corner::Sw => Point::new(-x, -y),
                    _ => Point::ZERO,
                };
                self.center + off
            }
            _ => self.bbox_corner(c),
        }
    }

    fn attr_width(&self) -> f64 {
        match self.kind {
            PKind::Brace => self.bbox.width(),
            PKind::Line | PKind::Move | PKind::Spline => self.line_wid,
            _ => self.bbox.width(),
        }
    }

    fn attr_height(&self) -> f64 {
        match self.kind {
            PKind::Brace => self.bbox.height(),
            PKind::Line | PKind::Move | PKind::Spline => self.line_ht,
            _ => self.bbox.height(),
        }
    }

    fn attr_radius(&self) -> f64 {
        match self.kind {
            PKind::Box => self.box_rad,
            PKind::Circle => self.bbox.width() / 2.0,
            PKind::Arc => self.radius,
            _ => 0.0,
        }
    }

    fn attr_diameter(&self) -> f64 {
        match self.kind {
            PKind::Circle => self.bbox.width(),
            PKind::Arc => self.radius * 2.0,
            _ => 0.0,
        }
    }

    fn attr_length(&self) -> f64 {
        match self.kind {
            PKind::Line | PKind::Move | PKind::Spline | PKind::Brace => self.start.dist(self.end),
            _ => 0.0,
        }
    }
}

fn bbox_point(bb: &Bbox, c: Corner) -> Point {
    let lo = bb.min;
    let hi = bb.max;
    let mid = (lo + hi) * 0.5;
    match c {
        Corner::N => Point::new(mid.x, hi.y),
        Corner::S => Point::new(mid.x, lo.y),
        Corner::E => Point::new(hi.x, mid.y),
        Corner::W => Point::new(lo.x, mid.y),
        Corner::Ne => Point::new(hi.x, hi.y),
        Corner::Se => Point::new(hi.x, lo.y),
        Corner::Nw => Point::new(lo.x, hi.y),
        Corner::Sw => Point::new(lo.x, lo.y),
        Corner::Center => mid,
        Corner::Start => lo,
        Corner::End => hi,
    }
}

// ---- evaluator state -------------------------------------------------------

struct State {
    pos: Point,
    dir: Dir,
    vars: HashMap<String, f64>,
    inherited_vars: HashSet<String>,
    export_vars: HashSet<String>,
    env: EnvVars,
    macros: Macros,
    includes: IncludeCtx,
    /// Labels visible from an enclosing scope (read-only, in absolute parent
    /// coordinates). A block may reference outer labels but must not let them
    /// affect its own `last`/nth/bbox, so they live here, not in `placed`.
    outer_labels: HashMap<String, Placed>,
    shapes: Vec<Shape>,
    shape_layers: Vec<i32>,
    shape_classes: Vec<Option<String>>,
    shape_spans: Vec<Option<Span>>,
    /// Span of the object statement currently being evaluated (attached to
    /// every shape it pushes).
    current_span: Option<Span>,
    /// Fixed page rectangle from `canvas from … to …` (last statement wins).
    canvas: Option<Bbox>,
    placed: Vec<Placed>,
    labels: HashMap<String, usize>,
    /// Visible geometry and text only; this becomes the final drawing/viewBox.
    bbox: Bbox,
    /// All evaluated geometry, including invisible helpers, for block sizing
    /// and anchor placement.
    layout_bbox: Bbox,
    // animation state
    anims: Vec<Anim>,
    anim_scroll: bool,
    diagnostics: Vec<String>,
    warnings: Vec<Diagnostic>,
    anim_cursor: f64,
    anim_end: HashMap<usize, f64>,
    limits: EvalLimits,
    rng: GlibcRand,
}

const DEFAULT_ANIM_DUR: f64 = 0.6;

fn checked_anim_time(name: &str, value: f64, max: f64) -> ER<f64> {
    if !value.is_finite() {
        return err(format!("animation {name} must be finite"));
    }
    if value < 0.0 {
        return err(format!("animation {name} must be non-negative"));
    }
    if value > max {
        return err(format!("animation {name} must be at most {max} seconds"));
    }
    Ok(value)
}

fn checked_anim_seconds_limit(value: f64, host_max: f64) -> ER<f64> {
    if !value.is_finite() {
        return err("maxanimseconds must be finite");
    }
    if value < 0.0 {
        return err("maxanimseconds must be non-negative");
    }
    if value > host_max {
        return err(format!("maxanimseconds must be at most {host_max}"));
    }
    Ok(value)
}

fn checked_anim_repeat(value: f64, max: i64) -> ER<i64> {
    if !value.is_finite() {
        return err("animation repeat must be finite");
    }
    if value < 0.0 && (value + 1.0).abs() > f64::EPSILON {
        return err("animation repeat must be -1 or non-negative");
    }

    let repeat = value.round();
    if repeat > max as f64 {
        return err(format!("animation repeat must be at most {max}"));
    }
    Ok(repeat as i64)
}

fn checked_anim_repeat_limit(value: f64, host_max: i64) -> ER<i64> {
    if !value.is_finite() {
        return err("maxanimrepeat must be finite");
    }
    if value < 0.0 {
        return err("maxanimrepeat must be non-negative");
    }

    let max = value.round();
    if max > host_max as f64 {
        return err(format!("maxanimrepeat must be at most {host_max}"));
    }
    Ok(max as i64)
}

fn additive_loop_iterations(from: f64, to: f64, by: f64) -> Option<u64> {
    const EPS: f64 = 1e-9;

    if !from.is_finite() || !to.is_finite() || !by.is_finite() {
        return None;
    }

    let cont = if by >= 0.0 {
        from <= to + EPS
    } else {
        from >= to - EPS
    };
    if !cont {
        return Some(0);
    }
    if by.abs() < f64::EPSILON {
        return Some(1);
    }

    let span = if by > 0.0 {
        to + EPS - from
    } else {
        from - (to - EPS)
    };
    let step = by.abs();
    let iterations = (span / step).floor() + 1.0;
    if !iterations.is_finite() || iterations >= u64::MAX as f64 {
        return Some(u64::MAX);
    }

    Some(iterations.max(0.0) as u64)
}

fn dir_unit(d: Dir) -> Point {
    match d {
        Dir::Right => Point::new(1.0, 0.0),
        Dir::Left => Point::new(-1.0, 0.0),
        Dir::Up => Point::new(0.0, 1.0),
        Dir::Down => Point::new(0.0, -1.0),
    }
}
fn horizontal(d: Dir) -> bool {
    matches!(d, Dir::Right | Dir::Left)
}

impl State {
    #[cfg(test)]
    fn new() -> Self {
        Self::with_limits(EvalLimits::default())
    }

    fn with_limits(limits: EvalLimits) -> Self {
        let mut vars = HashMap::new();
        install_dpic_compat_vars(&mut vars);
        State {
            pos: Point::ZERO,
            dir: Dir::Right,
            vars,
            inherited_vars: HashSet::new(),
            export_vars: HashSet::new(),
            env: EnvVars::new(limits),
            macros: HashMap::new(),
            includes: IncludeCtx::default(),
            outer_labels: HashMap::new(),
            shapes: Vec::new(),
            shape_layers: Vec::new(),
            shape_classes: Vec::new(),
            shape_spans: Vec::new(),
            current_span: None,
            canvas: None,
            placed: Vec::new(),
            labels: HashMap::new(),
            bbox: Bbox::new(),
            layout_bbox: Bbox::new(),
            anims: Vec::new(),
            anim_scroll: false,
            diagnostics: Vec::new(),
            warnings: Vec::new(),
            anim_cursor: 0.0,
            anim_end: HashMap::new(),
            limits,
            rng: GlibcRand::new(1),
        }
    }

    fn eval_stmts(&mut self, stmts: &[Stmt]) -> ER<()> {
        for s in stmts {
            self.eval_stmt(s)?;
        }
        Ok(())
    }

    fn checked_env_value(&self, e: EnvVar, value: f64) -> ER<f64> {
        match e {
            EnvVar::Maxanimrepeat => {
                Ok(checked_anim_repeat_limit(value, self.limits.max_animation_repeat)? as f64)
            }
            EnvVar::Maxanimseconds => {
                checked_anim_seconds_limit(value, self.limits.max_animation_seconds)
            }
            _ => Ok(value),
        }
    }

    fn max_anim_seconds(&self) -> ER<f64> {
        checked_anim_seconds_limit(
            self.env.get(EnvVar::Maxanimseconds),
            self.limits.max_animation_seconds,
        )
    }

    fn max_anim_repeat(&self) -> ER<i64> {
        checked_anim_repeat_limit(
            self.env.get(EnvVar::Maxanimrepeat),
            self.limits.max_animation_repeat,
        )
    }

    /// Parse a deferred `if`/`for` body now, expanding macros along this path.
    fn parse_body(&mut self, body: &Body) -> ER<Vec<Stmt>> {
        crate::parser::parse_body_tokens(body, &mut self.macros, &self.includes)
            .map_err(parse_eval_error)
    }

    fn eval_stmt(&mut self, s: &Stmt) -> ER<()> {
        match s {
            Stmt::Direction(d) => {
                self.dir = *d;
            }
            Stmt::Assign(list) => {
                for a in list {
                    self.eval_assignment(a)?;
                }
            }
            Stmt::Place { label, pos } => {
                let p = self.eval_pos(pos)?;
                let key = self.label_key(label)?;
                let mut bb = Bbox::new();
                bb.add(p);
                let idx = self.placed.len();
                self.placed.push(Placed {
                    kind: PKind::Text,
                    center: p,
                    bbox: bb,
                    start: p,
                    end: p,
                    thick: 0.0,
                    points: Vec::new(),
                    radius: 0.0,
                    box_rad: 0.0,
                    line_wid: 0.0,
                    line_ht: 0.0,
                    closed_path: false,
                    layer: 0,
                    shape: None,
                    members: HashMap::new(),
                    block_shapes: None,
                });
                self.labels.insert(key, idx);
            }
            Stmt::Group(stmts) => {
                let (pos, dir) = (self.pos, self.dir);
                self.eval_stmts(stmts)?;
                self.pos = pos;
                self.dir = dir;
            }
            Stmt::Object { label, object } => {
                let idx = self.eval_object(object)?;
                if let Some(l) = label {
                    let key = self.label_key(l)?;
                    self.labels.insert(key, idx);
                }
            }
            Stmt::Animate(a) => self.eval_animate(a)?,
            Stmt::AnimateScroll => self.anim_scroll = true,
            Stmt::Class { target, class } => {
                let idx = self.place_index(target)?;
                let name = self.eval_stringexpr(class)?;
                self.append_class_at(idx, &name)?;
            }
            Stmt::Canvas { from, to } => {
                let a = self.eval_pos(from)?;
                let b = self.eval_pos(to)?;
                let mut bb = Bbox::new();
                bb.add(a);
                bb.add(b);
                if bb.width() <= 0.0 || bb.height() <= 0.0 {
                    return err("canvas must have positive width and height");
                }
                self.canvas = Some(bb);
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                // only the taken branch is parsed (dead branches may be
                // syntactically invalid, e.g. an empty default-argument body)
                if self.eval_expr(cond)? != 0.0 {
                    let stmts = self.parse_body(then_body)?;
                    self.eval_stmts(&stmts)?;
                } else if let Some(e) = else_body {
                    let stmts = self.parse_body(e)?;
                    self.eval_stmts(&stmts)?;
                }
            }
            Stmt::For {
                var,
                subscript,
                from,
                to,
                by,
                mult,
                body,
            } => {
                let from = self.eval_expr(from)?;
                let to = self.eval_expr(to)?;
                let by = self.eval_expr(by)?;
                let mut v = from;
                let mut iters = 0u64;
                if !*mult
                    && let Some(total) = additive_loop_iterations(from, to, by)
                    && total > self.limits.max_loop_iterations
                {
                    return err(format!(
                        "for loop exceeded {} iterations",
                        self.limits.max_loop_iterations
                    ));
                }
                // Parsed lazily on the first iteration that actually runs, so
                // a zero-iteration loop never parses (or macro-expands) its
                // body — same deferred rule as dead `if` branches (#196).
                let mut parsed: Option<Vec<Stmt>> = None;
                const EPS: f64 = 1e-9;
                loop {
                    let cont = if *mult {
                        if by >= 1.0 {
                            v <= to + EPS
                        } else {
                            v >= to - EPS
                        }
                    } else if by >= 0.0 {
                        v <= to + EPS
                    } else {
                        v >= to - EPS
                    };
                    if !cont {
                        break;
                    }
                    if iters >= self.limits.max_loop_iterations {
                        return err(format!(
                            "for loop exceeded {} iterations",
                            self.limits.max_loop_iterations
                        ));
                    }
                    iters += 1;
                    let key = self.indexed_name(var, subscript.as_ref())?;
                    self.vars.insert(key, v);
                    if parsed.is_none() {
                        parsed = Some(self.parse_body(body)?);
                    }
                    self.eval_stmts(parsed.as_deref().unwrap())?;
                    let prev = v;
                    v = if *mult { v * by } else { v + by };
                    if (v - prev).abs() < f64::EPSILON {
                        break; // no progress (by 0, or *1) — avoid infinite loop
                    }
                }
            }
            Stmt::Print(item) => match item {
                PrintItem::Expr(e) => {
                    let v = self.eval_expr(e)?;
                    self.diagnostics.push(fmt_num(v));
                }
                PrintItem::Str(se) => {
                    let s = self.eval_stringexpr(se)?;
                    self.diagnostics.push(s);
                }
            },
            Stmt::Exec { command, arg_frame } => {
                let src = unescape_exec_source(&self.eval_stringexpr(command)?);
                let stmts = crate::parser::parse_exec_source(
                    &src,
                    &mut self.macros,
                    &self.includes,
                    arg_frame.as_deref(),
                )
                .map_err(parse_eval_error)?;
                self.eval_stmts(&stmts)?;
            }
            Stmt::Reset(list) => {
                if list.is_empty() {
                    self.env = EnvVars::new(self.limits);
                } else {
                    let d = EnvVars::new(self.limits);
                    for e in list {
                        self.env.set(*e, d.get(*e));
                    }
                }
            }
        }
        Ok(())
    }

    fn eval_animate(&mut self, a: &Animate) -> ER<()> {
        let idx = self.place_index(&a.target)?;
        let shape = self.placed[idx].shape.ok_or_else(|| EvalError {
            msg: "cannot animate a point (no drawn shape)".into(),
            info: None,
        })?;
        let max_seconds = self.max_anim_seconds()?;
        let dur = match &a.duration {
            Some(e) => checked_anim_time("duration", self.eval_expr(e)?, max_seconds)?,
            None => checked_anim_time("duration", DEFAULT_ANIM_DUR, max_seconds)?,
        };
        let mut start = match &a.timing {
            Timing::Sequential => self.anim_cursor,
            Timing::At(e) => self.eval_expr(e)?,
            Timing::After(p) => {
                let i = self.place_index(p)?;
                let sh = self.placed[i].shape.ok_or_else(|| EvalError {
                    msg: "`after` target has no animation".into(),
                    info: None,
                })?;
                *self.anim_end.get(&sh).unwrap_or(&0.0)
            }
        };
        start = checked_anim_time("start time", start, max_seconds)?;
        if let Some(d) = &a.delay {
            let delay = checked_anim_time("delay", self.eval_expr(d)?, max_seconds)?;
            start = checked_anim_time("start time", start + delay, max_seconds)?;
        }
        // Sequential/`after` timing tracks the *first* iteration's end, not the
        // repeated total — an ambient `repeat` loop (or an infinite one) must
        // not stall everything declared after it.
        let end = checked_anim_time("end time", start + dur, max_seconds)?;
        let max_repeat = self.max_anim_repeat()?;
        let repeat = match &a.repeat {
            Some(e) => checked_anim_repeat(self.eval_expr(e)?, max_repeat)?,
            None => 0,
        };
        self.anim_cursor = end;
        self.anim_end.insert(shape, end);
        if a.yoyo && repeat == 0 {
            let mut warning = Diagnostic::new(
                "yoyo_without_repeat",
                "`yoyo` has no effect without `repeat` (there is nothing to reverse)",
            );
            if let Some(span) = &a.effect_span {
                warning = warning.at(span.clone());
            }
            self.warnings.push(warning);
        }
        let ease = a.ease.as_ref().map(stringexpr_lit);
        let effect = stringexpr_lit(&a.effect);
        let is_move = effect == "move";
        let is_highlight = effect == "highlight";
        let is_slide = effect == "slide";
        let is_morph = effect == "morph";
        let from = a.slide_from.map(|d| {
            match d {
                Dir::Up => "up",
                Dir::Down => "down",
                Dir::Left => "left",
                Dir::Right => "right",
            }
            .to_string()
        });
        // Resolve the `along` path (a drawn object) to its shape index.
        let path = match &a.along {
            Some(p) => {
                let i = self.place_index(p)?;
                let sh = self.placed[i].shape.ok_or_else(|| EvalError {
                    msg: "`along` target has no drawn path".into(),
                    info: None,
                })?;
                Some(sh)
            }
            None => None,
        };
        // Resolve the `into` morph target (a drawn object) to its shape index.
        let morph = match &a.morph_into {
            Some(p) => {
                let i = self.place_index(p)?;
                let sh = self.placed[i].shape.ok_or_else(|| EvalError {
                    msg: "`into` target has no drawn shape".into(),
                    info: None,
                })?;
                Some(sh)
            }
            None => None,
        };
        // Resolve the `to` colour (any rpic colour form) to a CSS string.
        let color = match &a.color {
            Some(se) => Some(self.eval_color_expr(se)?),
            None => None,
        };
        if is_move && path.is_none() {
            return Err(EvalError {
                msg: "`move` needs a path: `animate <obj> with \"move\" along <path>`".into(),
                info: None,
            });
        }
        if path.is_some() && !is_move {
            let mut warning = Diagnostic::new(
                "along_without_move",
                "`along` only applies to the `move` effect and is ignored here",
            );
            if let Some(span) = &a.effect_span {
                warning = warning.at(span.clone());
            }
            self.warnings.push(warning);
        }
        if color.is_some() && !is_highlight {
            let mut warning = Diagnostic::new(
                "to_without_highlight",
                "`to <colour>` only applies to the `highlight` effect and is ignored here",
            );
            if let Some(span) = &a.effect_span {
                warning = warning.at(span.clone());
            }
            self.warnings.push(warning);
        }
        if is_slide && from.is_none() {
            return Err(EvalError {
                msg: "`slide` needs a direction: `animate <obj> with \"slide\" from <dir>`".into(),
                info: None,
            });
        }
        if from.is_some() && !is_slide {
            let mut warning = Diagnostic::new(
                "from_without_slide",
                "`from <dir>` only applies to the `slide` effect and is ignored here",
            );
            if let Some(span) = &a.effect_span {
                warning = warning.at(span.clone());
            }
            self.warnings.push(warning);
        }
        if is_morph && morph.is_none() {
            return Err(EvalError {
                msg: "`morph` needs a target: `animate <obj> with \"morph\" into <shape>`".into(),
                info: None,
            });
        }
        if morph.is_some() && !is_morph {
            let mut warning = Diagnostic::new(
                "into_without_morph",
                "`into <shape>` only applies to the `morph` effect and is ignored here",
            );
            if let Some(span) = &a.effect_span {
                warning = warning.at(span.clone());
            }
            self.warnings.push(warning);
        }
        let is_type = effect == "type";
        if a.type_unit.is_some() && !is_type {
            let mut warning = Diagnostic::new(
                "by_without_type",
                "`by word`/`by char` only applies to the `type` effect and is ignored here",
            );
            if let Some(span) = &a.effect_span {
                warning = warning.at(span.clone());
            }
            self.warnings.push(warning);
        }
        if !matches!(
            effect.as_str(),
            "draw" | "fade" | "pop" | "move" | "highlight" | "slide" | "morph" | "type"
        ) {
            let mut warning = Diagnostic::new(
                "unknown_animation_effect",
                format!(
                    "unknown animation effect `{effect}`; supported effects are `draw`, `fade`, `pop`, `move`, `highlight`, `slide`, `morph`, and `type`"
                ),
            )
            .found(effect.clone())
            .expected("draw, fade, pop, move, highlight, slide, morph, or type");
            if let Some(span) = &a.effect_span {
                warning = warning.at(span.clone());
            }
            self.warnings.push(warning);
        }
        let path = if is_move { path } else { None };
        let color = if is_highlight { color } else { None };
        let from = if is_slide { from } else { None };
        let morph = if is_morph { morph } else { None };
        let type_word = is_type && matches!(a.type_unit, Some(TypeUnit::Word));
        let make = |shape: usize, start: f64| Anim {
            shape,
            effect: effect.clone(),
            start,
            duration: dur,
            repeat,
            yoyo: a.yoyo,
            ease: ease.clone(),
            path,
            color: color.clone(),
            out: a.out,
            from: from.clone(),
            morph,
            type_word,
        };
        // `stagger <d>` on a block fans the effect across its *visible*
        // children (skipping `move`/invis spines), offset by d seconds each,
        // in source order — one manifest entry per child.
        if let Some(se) = &a.stagger {
            let step = checked_anim_time("stagger", self.eval_expr(se)?, max_seconds)?;
            let children: Vec<usize> = self.placed[idx]
                .block_shapes
                .map(|(lo, hi)| (lo..hi).filter(|&i| self.shapes[i].is_visible()).collect())
                .unwrap_or_default();
            if !children.is_empty() {
                let mut last_start = start;
                for (k, &child) in children.iter().enumerate() {
                    let child_start =
                        checked_anim_time("start time", start + k as f64 * step, max_seconds)?;
                    last_start = child_start;
                    self.anims.push(make(child, child_start));
                }
                let last_end = checked_anim_time("end time", last_start + dur, max_seconds)?;
                self.anim_cursor = last_end;
                // `after <block>` resolves to the block's own shape index, so
                // record the whole-stagger end there (line 863 seeded it with a
                // single-iteration end); recording under `children[0]` missed it
                // whenever the block leads with an invisible spine (audit).
                self.anim_end.insert(shape, last_end);
                return Ok(());
            }
            let mut warning = Diagnostic::new(
                "stagger_without_block",
                "`stagger` only applies to a block target with drawn children; animating the single object instead",
            );
            if let Some(span) = &a.effect_span {
                warning = warning.at(span.clone());
            }
            self.warnings.push(warning);
        }
        self.anims.push(make(shape, start));
        Ok(())
    }

    /// Append a validated CSS class to the shape behind `placed_idx` (rpic
    /// `class` extension). Multiple applications compose: `class="a b"`.
    fn append_class_at(&mut self, placed_idx: usize, name: &str) -> ER<()> {
        let name = name.trim();
        validate_class(name)?;
        let pl = &self.placed[placed_idx];
        if matches!(pl.kind, PKind::Block) {
            return err("`class` on a block is not supported yet; class its inner objects");
        }
        let Some(sh) = pl.shape else {
            return err("cannot attach a class to a point (no drawn shape)");
        };
        match &mut self.shape_classes[sh] {
            Some(existing) => {
                existing.push(' ');
                existing.push_str(name);
            }
            slot @ None => *slot = Some(name.to_string()),
        }
        Ok(())
    }

    /// rpic `texlabels` extension: typeset a fully `$…$`-delimited label as
    /// math via the registered renderer. Any failure — extension off, no
    /// renderer in this build, not fully delimited, or a TeX parse error —
    /// falls back to the literal text (a parse error also leaves a
    /// diagnostic). The picture itself never fails because of a math label.
    fn math_span_for(&mut self, s: &str) -> Option<crate::math::MathSpan> {
        if self.env.get(EnvVar::Texlabels) == 0.0 {
            return None;
        }
        let t = s.trim();
        let inner = t.strip_prefix('$')?.strip_suffix('$')?;
        if inner.is_empty() || inner.contains('$') {
            return None;
        }
        let Some(render) = crate::math::math_renderer() else {
            self.diagnostics.push(format!(
                "texlabels: no math renderer in this build; `{t}` kept literal"
            ));
            return None;
        };
        match render(inner, FONT_PT_MATH) {
            Ok(span) => Some(span),
            Err(e) => {
                self.diagnostics.push(format!(
                    "texlabels: `{t}` is not valid TeX math ({e}); label kept literal"
                ));
                None
            }
        }
    }

    fn scale_value(&self) -> ER<f64> {
        let scale = self.env.get(EnvVar::Scale);
        if scale.abs() < 1e-12 {
            return err("scale must be non-zero");
        }
        Ok(scale)
    }

    /// Convert a pic dimension from the current user units to internal inches.
    fn to_internal_dim(&self, v: f64) -> ER<f64> {
        Ok(v / self.scale_value()?)
    }

    fn env_dim(&self, e: EnvVar) -> ER<f64> {
        self.to_internal_dim(self.env.get(e))
    }

    fn canvas_margin(&self) -> ER<CanvasMargin> {
        let all = self.env_dim(EnvVar::Margin)?;
        Ok(CanvasMargin {
            top: all + self.env_dim(EnvVar::Topmargin)?,
            right: all + self.env_dim(EnvVar::Rightmargin)?,
            bottom: all + self.env_dim(EnvVar::Bottommargin)?,
            left: all + self.env_dim(EnvVar::Leftmargin)?,
        })
    }

    fn expr_dim(&mut self, e: &Expr) -> ER<f64> {
        let v = self.eval_expr(e)?;
        self.to_internal_dim(v)
    }

    /// Convert an internal geometric length back to pic's current user units.
    fn to_user_dim(&self, v: f64) -> f64 {
        v * self.env.get(EnvVar::Scale)
    }

    fn eval_assignment(&mut self, a: &Assignment) -> ER<()> {
        let rhs = self.eval_expr(&a.value)?;
        match &a.target {
            AssignTarget::Var(name, subscript) => {
                let key = self.indexed_name(name, subscript.as_ref())?;
                let cur = match self.vars.get(&key).copied() {
                    Some(v) => v,
                    None if matches!(a.op, AssignOp::Set) => 0.0,
                    None => return err(format!("variable not found `{key}`")),
                };
                let val = apply_op(a.op, cur, rhs)?;
                if !matches!(a.op, AssignOp::Set) && self.inherited_vars.contains(&key) {
                    self.export_vars.insert(key.clone());
                }
                self.vars.insert(key, val);
            }
            AssignTarget::Env(e) => {
                let cur = self.env.get(*e);
                let val = self.checked_env_value(*e, apply_op(a.op, cur, rhs)?)?;
                if matches!(e, EnvVar::Scale) {
                    if val.abs() < 1e-12 {
                        return err("scale must be non-zero");
                    }
                    if cur.abs() >= 1e-12 {
                        self.scale_existing_geometry(cur / val);
                    }
                    // Changing `scale` rescales all scaled dimension variables by
                    // the ratio (dpic semantics). They remain in user units;
                    // geometry converts them to internal inches by dividing by
                    // the current scale at use sites.
                    let ratio = if cur != 0.0 { val / cur } else { val };
                    for sv in SCALED_VARS {
                        let v = self.env.get(sv);
                        self.env.set(sv, v * ratio);
                    }
                }
                self.env.set(*e, val);
            }
        }
        Ok(())
    }

    fn scale_existing_geometry(&mut self, factor: f64) {
        if (factor - 1.0).abs() < 1e-12 {
            return;
        }
        self.pos = self.pos * factor;
        for sh in &mut self.shapes {
            scale_shape(sh, factor);
        }
        for pl in &mut self.placed {
            scale_placed(pl, factor);
        }
        scale_bbox_in_place(&mut self.bbox, factor);
        scale_bbox_in_place(&mut self.layout_bbox, factor);
    }

    // ---- objects -----------------------------------------------------------

    fn eval_object(&mut self, obj: &Object) -> ER<usize> {
        if !object_uses_bare_distance(&obj.kind) {
            self.warn_ignored_dist_attrs(obj);
        }
        // Blocks evaluate nested objects (which set their own spans) before
        // this frame finishes — save/restore keeps attribution per statement.
        let saved_span = std::mem::replace(&mut self.current_span, obj.span.clone());
        let result = self.eval_object_inner(obj);
        self.current_span = saved_span;
        let idx = result?;
        for a in &obj.attrs {
            if let Attr::Class(se) = a {
                let name = self.eval_stringexpr(se)?;
                self.append_class_at(idx, &name)?;
            }
        }
        Ok(idx)
    }

    fn warn_ignored_dist_attrs(&mut self, obj: &Object) {
        for attr in &obj.attrs {
            let Attr::Dist(expr, span) = attr else {
                continue;
            };
            let found = expr_bare_name(expr).unwrap_or("bare distance");
            let mut warning = Diagnostic::new(
                "ignored_attribute",
                format!("ignored `{found}` because this object does not accept a bare distance"),
            )
            .found(found)
            .expected("an attribute");
            if let Some(hint) = suggest_attribute(found) {
                warning = warning.hint(format!("did you mean `{hint}`?"));
            }
            if let Some(span) = span {
                warning = warning.at(span.clone());
            }
            self.warnings.push(warning);
        }
    }

    fn eval_object_inner(&mut self, obj: &Object) -> ER<usize> {
        match &obj.kind {
            ObjectKind::Primitive(p) => match p {
                Prim::Box | Prim::Circle | Prim::Ellipse => self.closed(*p, obj),
                Prim::Line | Prim::Arrow | Prim::Move | Prim::Spline => self.open(*p, obj),
                Prim::Arc => self.arc(obj),
            },
            ObjectKind::Text => self.text_obj(obj),
            ObjectKind::Brace => self.brace(obj),
            ObjectKind::Dot => self.closed_dot(obj),
            ObjectKind::Block(stmts) => self.block(stmts, obj),
            ObjectKind::Empty => self.block(&[], obj),
            ObjectKind::Continue => self.continue_obj(obj),
        }
    }

    // ---- attribute helpers -------------------------------------------------

    fn dim(&mut self, obj: &Object, kind: DimKind) -> ER<Option<f64>> {
        for a in &obj.attrs {
            if let Attr::Dim(k, e) = a
                && *k == kind
            {
                return Ok(Some(match kind {
                    DimKind::Thick | DimKind::Scaled => self.eval_expr(e)?,
                    DimKind::Ht | DimKind::Wid | DimKind::Rad | DimKind::Diam => {
                        self.expr_dim(e)?
                    }
                }));
            }
        }
        Ok(None)
    }

    fn has_dim(&self, obj: &Object, kind: DimKind) -> bool {
        obj.attrs
            .iter()
            .any(|a| matches!(a, Attr::Dim(k, _) if *k == kind))
    }

    fn scale_of(&mut self, obj: &Object) -> ER<f64> {
        Ok(self.dim(obj, DimKind::Scaled)?.unwrap_or(1.0))
    }

    fn dir_of(&self, obj: &Object) -> Dir {
        obj.attrs
            .iter()
            .rev()
            .find_map(|a| match a {
                Attr::Direction(d, _) => Some(*d),
                _ => None,
            })
            .unwrap_or(self.dir)
    }

    fn find_from(&mut self, obj: &Object) -> ER<Option<Point>> {
        for a in &obj.attrs {
            if let Attr::From(pos) = a {
                return Ok(Some(self.eval_pos(pos)?));
            }
        }
        Ok(None)
    }

    fn at_of(&mut self, obj: &Object) -> ER<Option<Point>> {
        for a in &obj.attrs {
            if let Attr::At(pos) = a {
                return Ok(Some(self.eval_pos(pos)?));
            }
        }
        Ok(None)
    }

    fn dest_of(&mut self, obj: &Object) -> ER<Option<Point>> {
        for a in &obj.attrs {
            if let Attr::To(pos) = a {
                return Ok(Some(self.eval_pos(pos)?));
            }
        }
        Ok(None)
    }

    /// The start/end `chop` amounts. `chop r1 chop r2` trims each end
    /// independently; a single `chop` applies to both ends.
    fn chop_of(&mut self, obj: &Object) -> ER<Option<(f64, f64)>> {
        let mut vals = Vec::new();
        for a in &obj.attrs {
            if let Attr::Chop(opt) = a {
                let amt = match opt {
                    Some(e) => self.expr_dim(e)?,
                    None => self.env_dim(EnvVar::Circlerad)?,
                };
                vals.push(amt);
            }
        }
        Ok(match vals.as_slice() {
            [] => None,
            [one] => Some((*one, *one)),
            [start, end, ..] => Some((*start, *end)),
        })
    }

    /// Width/height of the last placed object of the same closed kind (for `same`).
    fn last_dims_of(&self, p: Prim) -> Option<(f64, f64)> {
        let want = match p {
            Prim::Box => PKind::Box,
            Prim::Circle => PKind::Circle,
            Prim::Ellipse => PKind::Ellipse,
            _ => return None,
        };
        self.placed
            .iter()
            .rev()
            .find(|pl| pl.kind == want)
            .map(|pl| (pl.bbox.width(), pl.bbox.height()))
    }

    /// End-to-end vector of the last placed open object of the same kind.
    fn last_open_vector(&self, p: Prim) -> Option<Point> {
        let want = match p {
            Prim::Move => PKind::Move,
            Prim::Spline => PKind::Spline,
            Prim::Line | Prim::Arrow => PKind::Line,
            _ => return None,
        };
        self.placed
            .iter()
            .rev()
            .find(|pl| pl.kind == want)
            .map(|pl| pl.end - pl.start)
    }

    /// Compute the center of a closed object given direction and extents.
    fn place_center(&mut self, obj: &Object, dir: Dir, extent: f64, w: f64, h: f64) -> ER<Point> {
        self.place_with_corner_offset(obj, dir, extent, w, h, corner_offset)
    }

    fn place_closed_center(
        &mut self,
        p: Prim,
        obj: &Object,
        dir: Dir,
        extent: f64,
        dims: (f64, f64),
        rad: f64,
    ) -> ER<Point> {
        let (w, h) = dims;
        self.place_with_corner_offset(obj, dir, extent, w, h, |c, w, h| {
            closed_corner_offset(p, c, w, h, rad)
        })
    }

    fn place_with_corner_offset(
        &mut self,
        obj: &Object,
        dir: Dir,
        extent: f64,
        w: f64,
        h: f64,
        corner: impl Fn(Corner, f64, f64) -> Point,
    ) -> ER<Point> {
        if let Some(at) = self.at_of(obj)? {
            return Ok(at);
        }
        for a in &obj.attrs {
            if let Attr::With { anchor, at } = a {
                let ap = self.eval_pos(at)?;
                let off = match anchor {
                    WithAnchor::Corner(c) => corner(dir_start_end_corner(*c, dir), w, h),
                    WithAnchor::Pair(x, y) => Point::new(self.expr_dim(x)?, self.expr_dim(y)?),
                    WithAnchor::Place(_) => {
                        return err("`with .label` anchors are only valid on blocks");
                    }
                    WithAnchor::Plain => Point::ZERO,
                };
                return Ok(ap - off);
            }
        }
        Ok(self.pos + dir_unit(dir) * (extent / 2.0))
    }

    #[allow(clippy::too_many_arguments)]
    fn block_center(
        &mut self,
        obj: &Object,
        dir: Dir,
        extent: f64,
        w: f64,
        h: f64,
        local_center: Point,
        sub: &mut State,
    ) -> ER<Point> {
        if let Some(at) = self.at_of(obj)? {
            return Ok(at);
        }
        for a in &obj.attrs {
            if let Attr::With { anchor, at } = a {
                let ap = self.eval_pos(at)?;
                let off = match anchor {
                    WithAnchor::Corner(c) => corner_offset(dir_start_end_corner(*c, dir), w, h),
                    WithAnchor::Pair(x, y) => {
                        Point::new(self.expr_dim(x)?, self.expr_dim(y)?) - local_center
                    }
                    WithAnchor::Place(place) => sub.place_point(place)? - local_center,
                    WithAnchor::Plain => Point::ZERO,
                };
                return Ok(ap - off);
            }
        }
        Ok(self.pos + dir_unit(dir) * (extent / 2.0))
    }

    fn arrows_of(&self, obj: &Object, default_end: bool) -> Arrowheads {
        let mut found = None;
        for a in &obj.attrs {
            if let Attr::Arrowhead(h, _) = a {
                found = Some(match h {
                    token::Arrow::Left => Arrowheads::Start,
                    token::Arrow::Right => Arrowheads::End,
                    token::Arrow::Double => Arrowheads::Both,
                });
            }
        }
        found.unwrap_or(if default_end {
            Arrowheads::End
        } else {
            Arrowheads::None
        })
    }

    fn style_of(&mut self, obj: &Object) -> ER<Style> {
        // arrowhead dimensions follow the current `arrowht`/`arrowwid` globals
        let mut s = Style {
            arrow_ht: self.env_dim(EnvVar::Arrowht)?,
            arrow_wid: self.env_dim(EnvVar::Arrowwid)?,
            // `arrowhead = 0` draws an open (two-stroke) head; anything else
            // (default 2) is a filled triangle.
            arrow_filled: self.env.get(EnvVar::Arrowhead).round() as i64 != 0,
            ..Default::default()
        };
        let lt = self.env.get(EnvVar::Linethick);
        if lt > 0.0 {
            s.thick = Some(lt);
        }
        for a in &obj.attrs {
            match a {
                Attr::LineStyle(lt, opt) => match lt {
                    LineType::Solid => s.dash = Dash::Solid,
                    LineType::Dashed => {
                        let w = match opt {
                            Some(e) => self.expr_dim(e)?,
                            None => self.env_dim(EnvVar::Dashwid)?,
                        };
                        // a non-positive pitch would emit an invalid negative
                        // `stroke-dasharray` (#291); fall back to the default
                        let w = if w.is_finite() && w > 0.0 {
                            w
                        } else {
                            self.env_dim(EnvVar::Dashwid)?
                        };
                        s.dash = Dash::Dashed(w);
                    }
                    LineType::Dotted => {
                        // a non-positive pitch would emit an invalid dot gap
                        // (#291); drop it so the default spacing is used
                        let pitch = match opt {
                            Some(e) => {
                                let w = self.expr_dim(e)?;
                                (w.is_finite() && w > 0.0).then_some(w)
                            }
                            None => None,
                        };
                        s.dash = Dash::Dotted(pitch);
                    }
                    LineType::Invis => s.invis = true,
                },
                Attr::Fill(opt) => {
                    let g = match opt {
                        Some(e) => self.eval_expr(e)?,
                        None => self.env.get(EnvVar::Fillval),
                    };
                    s.fill = Some(Fill::Gray(g));
                    s.fill_open = true;
                }
                Attr::Color(kind, se) => {
                    let name = self.eval_color_expr(se)?;
                    match kind {
                        token::Color::Outlined => s.stroke = Some(name),
                        token::Color::Colored => {
                            s.stroke = Some(name.clone());
                            s.fill = Some(Fill::Color(name));
                        }
                        token::Color::Shaded => {
                            s.fill = Some(Fill::Color(name));
                            s.fill_open = true;
                        }
                    }
                }
                Attr::Hatch(kind) => {
                    let h = ensure_hatch(&mut s);
                    h.cross = matches!(kind, HatchKind::Cross);
                    s.fill_open = true;
                }
                Attr::HatchAngle(e) => ensure_hatch(&mut s).angle = self.eval_expr(e)?,
                Attr::HatchSep(e) => {
                    let sep = self.expr_dim(e)?;
                    if sep <= 0.0 {
                        return err("hatchsep must be positive");
                    }
                    ensure_hatch(&mut s).sep = sep;
                    s.fill_open = true;
                }
                Attr::HatchWidth(e) => {
                    let width = self.eval_expr(e)?;
                    if width < 0.0 {
                        return err("hatchwidth must be non-negative");
                    }
                    ensure_hatch(&mut s).width = width;
                    s.fill_open = true;
                }
                Attr::HatchColor(se) => {
                    let name = self.eval_color_expr(se)?;
                    ensure_hatch(&mut s).color = name;
                    s.fill_open = true;
                }
                Attr::Gradient(a, b) => {
                    let from = self.eval_color_expr(a)?;
                    let to = self.eval_color_expr(b)?;
                    let g = ensure_gradient(&mut s);
                    g.from = from;
                    g.to = to;
                    s.fill_open = true;
                }
                Attr::GradientAngle(e) => {
                    ensure_gradient(&mut s).angle = self.eval_expr(e)?;
                    s.fill_open = true;
                }
                Attr::Opacity(e) => {
                    let opacity = self.eval_expr(e)?;
                    if !(0.0..=1.0).contains(&opacity) {
                        return err("opacity must be between 0 and 1");
                    }
                    s.fill_opacity = Some(opacity);
                }
                Attr::Dim(DimKind::Thick, e) => s.thick = Some(self.eval_expr(e)?),
                // pikchr-flavoured `thin`: a lighter stroke, ⅔ of `linethick`.
                Attr::Thin => s.thick = Some(self.env.get(EnvVar::Linethick) * 2.0 / 3.0),
                Attr::Arrowhead(_, Some(e)) => {
                    s.arrow_filled = self.eval_expr(e)?.round() as i64 != 0;
                }
                _ => {}
            }
        }
        Ok(s)
    }

    fn eval_color_expr(&mut self, se: &StringExpr) -> ER<String> {
        if let StringExpr::Lit(name) = se {
            // A bareword in colour position that names a variable resolves to
            // its value as a numeric colour (`c = 0xRRGGBB; … colored c`), so
            // colours can be held in variables and computed. Variables win over
            // a same-named macro; a bareword that is neither stays a literal
            // colour name (`colored crimson`), so existing sources are inert.
            if let Some(&v) = self.vars.get(name) {
                return num_to_color(v);
            }
            if let Some(body) = self.macros.get(name).cloned() {
                let s = if let Some(lit) = single_token_macro_string(&body) {
                    lit
                } else {
                    let parsed = crate::parser::parse_stringexpr_tokens(
                        &body,
                        &mut self.macros,
                        &self.includes,
                    )
                    .map_err(parse_eval_error)?;
                    self.eval_stringexpr(&parsed)?
                };
                return self.checked_color(normalize_color_string(s));
            }
        }
        let color = normalize_color_string(self.eval_stringexpr(se)?);
        self.checked_color(color)
    }

    /// Reject active CSS/SVG paint forms, then warn (once) if a resolved colour
    /// string isn't a form any SVG renderer understands. Plain unknown names
    /// still pass through unchanged for dpic compatibility.
    fn checked_color(&mut self, color: String) -> ER<String> {
        if let Some(reason) = unsafe_svg_colour_reason(&color) {
            return err(reason);
        }
        if !crate::color::is_valid_color(&color) {
            let mut warning = Diagnostic::new(
                "invalid_color",
                format!("`{color}` is not a known colour name or hex/rgb() value"),
            )
            .found(color.clone())
            .expected("a CSS/xcolor colour name, #hex, or rgb(...)");
            if let Some(hint) = crate::color::suggest(&color) {
                warning = warning.hint(format!("did you mean `{hint}`?"));
            }
            self.warnings.push(warning);
        }
        Ok(color)
    }

    fn text_of(&mut self, obj: &Object) -> ER<Vec<TextLine>> {
        Ok(self.text_and_fit_text_of(obj)?.0)
    }

    fn text_and_fit_text_of(&mut self, obj: &Object) -> ER<(Vec<TextLine>, Option<Vec<TextLine>>)> {
        let mut lines: Vec<TextLine> = Vec::new();
        let mut fit_lines = None;
        let mut pending_halign = 0i8;
        let mut pending_valign = 0i8;
        // rpic font attributes bind like ljust/rjust: to the preceding string,
        // or — written before any string — to the next one.
        let mut pending_style = PendingStyle::default();
        for a in &obj.attrs {
            match a {
                Attr::TextPos(tp) => {
                    if let Some(line) = lines.last_mut() {
                        apply_text_pos(&mut line.halign, &mut line.valign, *tp);
                    } else {
                        apply_text_pos(&mut pending_halign, &mut pending_valign, *tp);
                    }
                }
                Attr::Bold => match lines.last_mut() {
                    Some(line) => line.bold = true,
                    None => pending_style.bold = true,
                },
                Attr::Italic => match lines.last_mut() {
                    Some(line) => line.italic = true,
                    None => pending_style.italic = true,
                },
                Attr::Mono => match lines.last_mut() {
                    Some(line) => line.family = Some("monospace".into()),
                    None => pending_style.family = Some("monospace".into()),
                },
                Attr::Font(se) => {
                    let family = self.eval_stringexpr(se)?;
                    match lines.last_mut() {
                        Some(line) => line.family = Some(family),
                        None => pending_style.family = Some(family),
                    }
                }
                Attr::FontSize(e) => {
                    let pt = self.eval_expr(e)?;
                    if !pt.is_finite() || pt <= 0.0 {
                        return err("fontsize must be a positive number of points");
                    }
                    match lines.last_mut() {
                        Some(line) => line.size_pt = Some(pt),
                        None => pending_style.size_pt = Some(pt),
                    }
                }
                Attr::Rotated(e) => {
                    let deg = self.eval_expr(e)?;
                    if !deg.is_finite() {
                        return err("rotated angle must be finite");
                    }
                    match lines.last_mut() {
                        Some(line) => line.rotate = Some(deg),
                        None => pending_style.rotate = Some(deg),
                    }
                }
                Attr::Aligned => match lines.last_mut() {
                    Some(line) => line.aligned = true,
                    None => pending_style.aligned = true,
                },
                Attr::Sized(big) => {
                    // pikchr big/small: 1.5× / 0.7× of the classic 11 pt
                    let pt = FONT_PT_CLASSIC * if *big { 1.5 } else { 0.7 };
                    match lines.last_mut() {
                        Some(line) => line.size_pt = Some(pt),
                        None => pending_style.size_pt = Some(pt),
                    }
                }
                Attr::Text(se) => {
                    let s = self.eval_stringexpr(se)?;
                    let math = self.math_span_for(&s);
                    lines.push(TextLine {
                        s,
                        math,
                        halign: pending_halign,
                        valign: pending_valign,
                        text_offset: self.env_dim(EnvVar::Textoffset)?,
                        bold: pending_style.bold,
                        italic: pending_style.italic,
                        family: pending_style.family.take(),
                        size_pt: pending_style.size_pt,
                        rotate: pending_style.rotate,
                        aligned: pending_style.aligned,
                    });
                    pending_halign = 0;
                    pending_valign = 0;
                    pending_style = PendingStyle::default();
                }
                Attr::Fit if fit_lines.is_none() => {
                    fit_lines = Some(lines.clone());
                }
                _ => {}
            }
        }
        Ok((lines, fit_lines))
    }
}

mod build;
mod helpers;
mod resolve;
use helpers::*;

#[cfg(test)]
mod tests;
