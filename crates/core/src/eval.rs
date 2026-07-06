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

const DP_TEXT_RATIO: f64 = 0.66;
const TEXT_EM: f64 = 11.0 / 72.0;
/// Label font size in points, matching the SVG backend's `FONT_PT`.
const FONT_PT_MATH: f64 = 11.0;
const TEXT_CHAR_W: f64 = 0.6 * TEXT_EM;
const TEXT_LINE_H: f64 = 1.2 * TEXT_EM;
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
    let mut st = State::new();
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

#[derive(Clone)]
struct EnvVars {
    v: HashMap<u8, f64>,
}

fn ev_key(e: EnvVar) -> u8 {
    e as u8
}

impl EnvVars {
    fn new() -> Self {
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
    rng: GlibcRand,
}

const DEFAULT_ANIM_DUR: f64 = 0.6;

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
    fn new() -> Self {
        let mut vars = HashMap::new();
        install_dpic_compat_vars(&mut vars);
        State {
            pos: Point::ZERO,
            dir: Dir::Right,
            vars,
            inherited_vars: HashSet::new(),
            export_vars: HashSet::new(),
            env: EnvVars::new(),
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
            rng: GlibcRand::new(1),
        }
    }

    fn eval_stmts(&mut self, stmts: &[Stmt]) -> ER<()> {
        for s in stmts {
            self.eval_stmt(s)?;
        }
        Ok(())
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
                    iters += 1;
                    if iters > 1_000_000 {
                        return err("for loop exceeded 1,000,000 iterations");
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
                    &self.macros,
                    &self.includes,
                    arg_frame.as_deref(),
                )
                .map_err(parse_eval_error)?;
                self.eval_stmts(&stmts)?;
            }
            Stmt::Reset(list) => {
                if list.is_empty() {
                    self.env = EnvVars::new();
                } else {
                    let d = EnvVars::new();
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
        let dur = match &a.duration {
            Some(e) => self.eval_expr(e)?,
            None => DEFAULT_ANIM_DUR,
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
        if let Some(d) = &a.delay {
            start += self.eval_expr(d)?;
        }
        // Sequential/`after` timing tracks the *first* iteration's end, not the
        // repeated total — an ambient `repeat` loop (or an infinite one) must
        // not stall everything declared after it.
        let end = start + dur;
        self.anim_cursor = end;
        self.anim_end.insert(shape, end);
        let repeat = match &a.repeat {
            Some(e) => self.eval_expr(e)?.round() as i64,
            None => 0,
        };
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
        if !matches!(
            effect.as_str(),
            "draw" | "fade" | "pop" | "move" | "highlight" | "slide" | "morph"
        ) {
            let mut warning = Diagnostic::new(
                "unknown_animation_effect",
                format!(
                    "unknown animation effect `{effect}`; supported effects are `draw`, `fade`, `pop`, `move`, `highlight`, `slide`, and `morph`"
                ),
            )
            .found(effect.clone())
            .expected("draw, fade, pop, move, highlight, slide, or morph");
            if let Some(span) = &a.effect_span {
                warning = warning.at(span.clone());
            }
            self.warnings.push(warning);
        }
        let path = if is_move { path } else { None };
        let color = if is_highlight { color } else { None };
        let from = if is_slide { from } else { None };
        let morph = if is_morph { morph } else { None };
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
        };
        // `stagger <d>` on a block fans the effect across its *visible*
        // children (skipping `move`/invis spines), offset by d seconds each,
        // in source order — one manifest entry per child.
        if let Some(se) = &a.stagger {
            let step = self.eval_expr(se)?;
            let children: Vec<usize> = self.placed[idx]
                .block_shapes
                .map(|(lo, hi)| (lo..hi).filter(|&i| self.shapes[i].is_visible()).collect())
                .unwrap_or_default();
            if !children.is_empty() {
                for (k, &child) in children.iter().enumerate() {
                    self.anims.push(make(child, start + k as f64 * step));
                }
                let last_end = start + (children.len() - 1) as f64 * step + dur;
                self.anim_cursor = last_end;
                self.anim_end.insert(children[0], last_end);
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
                let val = apply_op(a.op, cur, rhs)?;
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

    /// `continue`: append another segment to the most recent line/spline,
    /// extending it from its current end in the current (or given) direction.
    fn continue_obj(&mut self, obj: &Object) -> ER<usize> {
        let idx = self
            .shapes
            .iter()
            .rposition(|s| matches!(s, Shape::Path { .. } | Shape::Spline { .. }))
            .ok_or_else(|| EvalError {
                msg: "`continue` has no previous line to extend".into(),
                info: None,
            })?;
        let pidx = self.placed.iter().position(|pl| pl.shape == Some(idx));
        if let Some(pi) = pidx
            && self.placed[pi].closed_path
        {
            return err("polygon is closed");
        }
        let start = match &self.shapes[idx] {
            Shape::Path { pts, .. } | Shape::Spline { pts, .. } => *pts.last().unwrap(),
            _ => unreachable!(),
        };
        let deflen_h = self.env_dim(EnvVar::Linewid)?;
        let deflen_v = self.env_dim(EnvVar::Lineht)?;

        let mut pts = vec![start];
        let mut pend = Point::ZERO;
        let mut any = false;
        let mut last_dir = self.dir;
        for a in &obj.attrs {
            match a {
                Attr::Direction(d, opt) => {
                    let dist = match opt {
                        Some(e) => self.expr_dim(e)?,
                        None => {
                            if horizontal(*d) {
                                deflen_h
                            } else {
                                deflen_v
                            }
                        }
                    };
                    pend = pend + dir_unit(*d) * dist;
                    last_dir = *d;
                    any = true;
                }
                Attr::Then => {
                    let np = *pts.last().unwrap() + pend;
                    pts.push(np);
                    pend = Point::ZERO;
                }
                Attr::To(pos) => {
                    if pend != Point::ZERO {
                        let np = *pts.last().unwrap() + pend;
                        pts.push(np);
                        pend = Point::ZERO;
                    }
                    pts.push(self.eval_pos(pos)?);
                    any = true;
                }
                Attr::By(pos) => {
                    let dp = self.eval_pos(pos)?;
                    pend = pend + (dp - Point::ZERO);
                    any = true;
                }
                Attr::Dist(e, _) => {
                    let dist = self.expr_dim(e)?;
                    pend = pend + dir_unit(last_dir) * dist;
                    any = true;
                }
                _ => {}
            }
        }
        if pend != Point::ZERO {
            let np = *pts.last().unwrap() + pend;
            pts.push(np);
        }
        if pts.len() == 1 && !any {
            let dist = if horizontal(self.dir) {
                deflen_h
            } else {
                deflen_v
            };
            pts.push(start + dir_unit(self.dir) * dist);
            last_dir = self.dir;
        }

        let new: Vec<Point> = pts[1..].to_vec();
        let visible = match &mut self.shapes[idx] {
            Shape::Path { pts: p, style, .. } | Shape::Spline { pts: p, style, .. } => {
                p.extend_from_slice(&new);
                (!style.invis, stroke_half_width(style))
            }
            _ => unreachable!(),
        };
        let end = *pts.last().unwrap();
        let mut bb = Bbox::new();
        for q in &new {
            self.layout_bbox.add(*q);
            bb.add(*q);
        }
        if visible.0 {
            self.bbox.union(&painted_bbox(&bb, visible.1));
        }
        self.pos = end;
        self.dir = last_dir;

        if let Some(pi) = pidx {
            self.placed[pi].end = end;
            self.placed[pi].bbox.add(end);
            self.placed[pi].center = (self.placed[pi].start + end) * 0.5;
            self.placed[pi].points.extend_from_slice(&new);
        }
        Ok(pidx.unwrap_or(idx))
    }

    /// rpic extension: a junction dot — circle machinery with `dotrad` as
    /// the default radius and a solid (gray-0) fill unless overridden, the
    /// exact geometry the classic `dot(P)` circuit macro produced.
    fn closed_dot(&mut self, obj: &Object) -> ER<usize> {
        self.closed_impl(Prim::Circle, obj, true)
    }

    fn closed(&mut self, p: Prim, obj: &Object) -> ER<usize> {
        self.closed_impl(p, obj, false)
    }

    fn closed_impl(&mut self, p: Prim, obj: &Object, dot: bool) -> ER<usize> {
        let mut style = self.style_of(obj)?;
        if dot && style.fill.is_none() {
            style.fill = Some(Fill::Gray(0.0));
        }
        let (text, fit_text) = self.text_and_fit_text_of(obj)?;
        let dir = self.dir_of(obj);
        let scale = self.scale_of(obj)?;

        // dimensions — `same` reuses the previous like-object's size as the
        // default; explicit ht/wid/rad still override.
        let prev = if obj.attrs.iter().any(|a| matches!(a, Attr::Same)) {
            self.last_dims_of(p)
        } else {
            None
        };
        let (mut w, mut h, mut rad);
        match p {
            Prim::Circle => {
                let def_r = prev.map(|(pw, _)| pw / 2.0).unwrap_or(self.env_dim(if dot {
                    EnvVar::Dotrad
                } else {
                    EnvVar::Circlerad
                })?);
                let r = self.dim(obj, DimKind::Rad)?.unwrap_or(def_r);
                let r = self.dim(obj, DimKind::Diam)?.map(|d| d / 2.0).unwrap_or(r);
                w = 2.0 * r;
                h = 2.0 * r;
                rad = r;
            }
            Prim::Ellipse => {
                let (dw, dh) = prev.unwrap_or((
                    self.env_dim(EnvVar::Ellipsewid)?,
                    self.env_dim(EnvVar::Ellipseht)?,
                ));
                w = self.dim(obj, DimKind::Wid)?.unwrap_or(dw);
                h = self.dim(obj, DimKind::Ht)?.unwrap_or(dh);
                rad = 0.0;
            }
            _ => {
                let (dw, dh) =
                    prev.unwrap_or((self.env_dim(EnvVar::Boxwid)?, self.env_dim(EnvVar::Boxht)?));
                w = self.dim(obj, DimKind::Wid)?.unwrap_or(dw);
                h = self.dim(obj, DimKind::Ht)?.unwrap_or(dh);
                rad = self
                    .dim(obj, DimKind::Rad)?
                    .unwrap_or(self.env_dim(EnvVar::Boxrad)?);
            }
        }
        if let Some(fit_text) = fit_text {
            let (fit_w, fit_h) = fitted_text_size(&fit_text).ok_or_else(|| EvalError {
                msg: "`fit` requires visible text before the attribute".into(),
                info: None,
            })?;
            match p {
                Prim::Circle => {
                    if !self.has_dim(obj, DimKind::Rad) && !self.has_dim(obj, DimKind::Diam) {
                        let diam = fit_w.hypot(fit_h);
                        w = diam;
                        h = diam;
                        rad = diam / 2.0;
                    }
                }
                Prim::Ellipse => {
                    if !self.has_dim(obj, DimKind::Wid) {
                        w = fit_w;
                    }
                    if !self.has_dim(obj, DimKind::Ht) {
                        h = fit_h;
                    }
                }
                _ => {
                    if !self.has_dim(obj, DimKind::Wid) {
                        w = fit_w;
                    }
                    if !self.has_dim(obj, DimKind::Ht) {
                        h = fit_h;
                    }
                }
            }
        }
        w *= scale;
        h *= scale;
        rad *= scale;

        let extent = if horizontal(dir) { w } else { h };
        let center = self.place_closed_center(p, obj, dir, extent, (w, h), rad)?;

        let mut bb = Bbox::new();
        bb.add(center - Point::new(w / 2.0, h / 2.0));
        bb.add(center + Point::new(w / 2.0, h / 2.0));
        let layout_bb = if matches!(p, Prim::Box) {
            dpic_box_layout_bbox(center, w, h)
        } else {
            bb
        };
        self.layout_bbox.union(&layout_bb);
        if closed_shape_is_visible(&style) {
            self.bbox
                .union(&painted_bbox(&bb, stroke_half_width(&style)));
        }
        self.union_text(center, &text);

        let shape = match p {
            Prim::Circle => Shape::Circle {
                c: center,
                r: rad,
                style,
                text,
            },
            Prim::Ellipse => Shape::Ellipse {
                c: center,
                w,
                h,
                style,
                text,
            },
            _ => Shape::Box {
                c: center,
                w,
                h,
                rad,
                style,
                text,
            },
        };
        let layer = self.layer_of(obj, 0)?;
        self.push_shape(shape, layer);

        let half = dir_unit(dir) * (extent / 2.0);
        let start = center - half;
        let end = center + half;
        self.pos = end;
        self.dir = dir;
        let kind = match p {
            Prim::Circle => PKind::Circle,
            Prim::Ellipse => PKind::Ellipse,
            _ => PKind::Box,
        };
        let sh = self.shapes.len() - 1;
        let idx = self.record(kind, center, bb, start, end, 0.0, Some(sh));
        if matches!(kind, PKind::Box) {
            self.placed[idx].box_rad = rad;
        }
        Ok(idx)
    }

    fn brace(&mut self, obj: &Object) -> ER<usize> {
        let style = self.style_of(obj)?;
        let text = self.text_of(obj)?;
        let start = self.find_from(obj)?.unwrap_or(self.pos);
        let has_to = obj.attrs.iter().any(|a| matches!(a, Attr::To(_)));
        let (end, last_dir) = self.brace_end(obj, start, has_to)?;
        let v = end - start;
        let len = v.len();
        if len <= 1e-12 {
            return err("brace endpoints must be distinct");
        }

        let depth = self
            .dim(obj, DimKind::Wid)?
            .unwrap_or(DEFAULT_BRACE_DEPTH)
            .abs();
        let pos = self.brace_pos_of(obj)?;
        let label_offset = self.brace_label_offset_of(obj)?;
        let side = brace_side(v / len, self.brace_side_dir(obj, has_to));
        let cubics = brace_cubics(start, end, side * depth, pos);
        let cusp = brace_cusp(&cubics).unwrap_or(start + v * pos);
        let label_at =
            cusp + side * (self.env_dim(EnvVar::Textoffset)? + TEXT_LINE_H + label_offset);
        let mut bb = cubics_bbox(&cubics);
        self.layout_bbox.union(&bb);
        if !style.invis {
            self.bbox
                .union(&painted_bbox(&bb, stroke_half_width(&style)));
        } else if style.invis_bounds {
            self.bbox.union(&bb);
        }
        let text_bb = text_bbox(label_at, &text);
        self.bbox.union(&text_bb);
        bb.union(&text_bb);

        let shape = Shape::Brace {
            a: start,
            b: end,
            cubics: cubics.clone(),
            label_at,
            style,
            text,
        };
        let layer = self.layer_of(obj, 0)?;
        self.push_shape(shape, layer);

        self.pos = end;
        self.dir = last_dir;
        let sh = self.shapes.len() - 1;
        let idx = self.record(PKind::Brace, cusp, bb, start, end, 0.0, Some(sh));
        self.placed[idx].points = sample_cubics(&cubics, 6);
        Ok(idx)
    }

    fn brace_end(&mut self, obj: &Object, start: Point, has_to: bool) -> ER<(Point, Dir)> {
        if has_to {
            let mut end = None;
            for a in &obj.attrs {
                if let Attr::To(pos) = a {
                    end = Some(self.eval_pos(pos)?);
                }
            }
            let end = end.unwrap();
            return Ok((end, nearest_dir(end - start)));
        }

        let mut pend = Point::ZERO;
        let mut any = false;
        let mut last_dir = self.dir;
        for a in &obj.attrs {
            match a {
                Attr::Direction(d, opt) => {
                    let dist = match opt {
                        Some(e) => self.expr_dim(e)?,
                        None => {
                            if horizontal(*d) {
                                self.env_dim(EnvVar::Linewid)?
                            } else {
                                self.env_dim(EnvVar::Lineht)?
                            }
                        }
                    };
                    pend = pend + dir_unit(*d) * dist;
                    last_dir = *d;
                    any = true;
                }
                Attr::By(pos) => {
                    pend = pend + self.eval_pos(pos)?;
                    any = true;
                }
                Attr::Dist(e, _) => {
                    let dist = self.expr_dim(e)?;
                    pend = pend + dir_unit(last_dir) * dist;
                    any = true;
                }
                _ => {}
            }
        }
        if any {
            Ok((start + pend, last_dir))
        } else {
            let dist = if horizontal(self.dir) {
                self.env_dim(EnvVar::Linewid)?
            } else {
                self.env_dim(EnvVar::Lineht)?
            };
            Ok((start + dir_unit(self.dir) * dist, self.dir))
        }
    }

    fn brace_side_dir(&self, obj: &Object, has_to: bool) -> Option<Dir> {
        if !has_to {
            return None;
        }
        obj.attrs.iter().rev().find_map(|a| match a {
            Attr::Direction(d, None) => Some(*d),
            _ => None,
        })
    }

    fn brace_pos_of(&mut self, obj: &Object) -> ER<f64> {
        let mut pos = DEFAULT_BRACE_POS;
        for a in &obj.attrs {
            if let Attr::BracePos(e) = a {
                pos = self.eval_expr(e)?;
            }
        }
        if !pos.is_finite() || pos <= 0.0 || pos >= 1.0 {
            return err("bracepos must be between 0 and 1");
        }
        Ok(pos)
    }

    fn brace_label_offset_of(&mut self, obj: &Object) -> ER<f64> {
        let mut offset = 0.0;
        for a in &obj.attrs {
            if let Attr::BraceLabelOffset(e) = a {
                offset = self.expr_dim(e)?;
            }
        }
        Ok(offset)
    }

    fn open(&mut self, p: Prim, obj: &Object) -> ER<usize> {
        let mut style = self.style_of(obj)?;
        let mut text = self.text_of(obj)?;
        let line_wid = style.arrow_wid;
        let line_ht = style.arrow_ht;
        let is_move = matches!(p, Prim::Move);
        if is_move {
            style.invis = true;
            style.invis_bounds = true;
        }
        let (deflen_h, deflen_v) = if is_move {
            (
                self.env_dim(EnvVar::Movewid)?,
                self.env_dim(EnvVar::Moveht)?,
            )
        } else {
            (
                self.env_dim(EnvVar::Linewid)?,
                self.env_dim(EnvVar::Lineht)?,
            )
        };
        let same_vec = if obj.attrs.iter().any(|a| matches!(a, Attr::Same)) {
            self.last_open_vector(p)
        } else {
            None
        };

        // starting point
        let start = self.find_from(obj)?.unwrap_or(self.pos);
        let mut pts = vec![start];
        let mut pend = Point::ZERO;
        let mut any = false;
        let mut last_dir = self.dir;
        let mut closed = false;

        for a in &obj.attrs {
            match a {
                Attr::Direction(d, opt) => {
                    if closed {
                        return err("polygon is closed");
                    }
                    let dist = match opt {
                        Some(e) => self.expr_dim(e)?,
                        None => {
                            if horizontal(*d) {
                                deflen_h
                            } else {
                                deflen_v
                            }
                        }
                    };
                    pend = pend + dir_unit(*d) * dist;
                    last_dir = *d;
                    any = true;
                }
                Attr::Then => {
                    if closed {
                        return err("polygon is closed");
                    }
                    let np = *pts.last().unwrap() + pend;
                    pts.push(np);
                    pend = Point::ZERO;
                }
                Attr::To(pos) => {
                    if closed {
                        return err("polygon is closed");
                    }
                    if pend != Point::ZERO {
                        let np = *pts.last().unwrap() + pend;
                        pts.push(np);
                        pend = Point::ZERO;
                    }
                    pts.push(self.eval_pos(pos)?);
                    any = true;
                }
                Attr::By(pos) => {
                    if closed {
                        return err("polygon is closed");
                    }
                    let d = self.eval_pos(pos)?;
                    pend = pend + (d - Point::ZERO);
                    any = true;
                }
                Attr::Dist(e, _) => {
                    if closed {
                        return err("polygon is closed");
                    }
                    let dist = self.expr_dim(e)?;
                    pend = pend + dir_unit(last_dir) * dist;
                    any = true;
                }
                Attr::Close => {
                    if pend != Point::ZERO {
                        let np = *pts.last().unwrap() + pend;
                        pts.push(np);
                        pend = Point::ZERO;
                    }
                    if pts.len() < 3 {
                        return err("need at least 3 vertices in order to close the polygon");
                    }
                    if closed {
                        return err("polygon already closed");
                    }
                    let first = pts[0];
                    let last = *pts.last().unwrap();
                    if first.dist(last) > 1e-12 {
                        let closing = first - last;
                        pts.push(first);
                        last_dir = nearest_dir(closing);
                    }
                    closed = true;
                    any = true;
                }
                _ => {}
            }
        }
        if pend != Point::ZERO {
            let np = *pts.last().unwrap() + pend;
            pts.push(np);
        }
        if pts.len() == 1 && !any {
            if let Some(v) = same_vec.filter(|v| v.len() > 1e-12) {
                pts.push(start + v);
                last_dir = nearest_dir(v);
            } else {
                // bare line/arrow/move in the current direction
                let dist = if horizontal(self.dir) {
                    deflen_h
                } else {
                    deflen_v
                };
                pts.push(start + dir_unit(self.dir) * dist);
                last_dir = self.dir;
            }
        }

        // `chop`: positive values trim each end; negative values extend it,
        // as in dpic examples such as `chop 0 chop -0.1`.
        if let Some((start_chop, end_chop)) = self.chop_of(obj)?
            && pts.len() >= 2
        {
            let n = pts.len();
            if start_chop != 0.0 {
                let d0 = pts[1] - pts[0];
                let l0 = d0.len();
                if l0 > f64::EPSILON && (start_chop < 0.0 || l0 > start_chop) {
                    pts[0] = pts[0] + d0 / l0 * start_chop;
                }
            }
            if end_chop != 0.0 {
                let d1 = pts[n - 2] - pts[n - 1];
                let l1 = d1.len();
                if l1 > f64::EPSILON && (end_chop < 0.0 || l1 > end_chop) {
                    pts[n - 1] = pts[n - 1] + d1 / l1 * end_chop;
                }
            }
        }

        // arrowheads
        let arrows = self.arrows_of(obj, matches!(p, Prim::Arrow));

        let mut bb = Bbox::new();
        for pt in &pts {
            bb.add(*pt);
        }
        self.layout_bbox.union(&bb);
        if !style.invis {
            self.bbox
                .union(&painted_bbox(&bb, stroke_half_width(&style)));
        } else if style.invis_bounds {
            self.bbox.union(&bb);
        }

        let end = *pts.last().unwrap();
        let center = if closed {
            (bb.min + bb.max) * 0.5
        } else {
            (pts[0] + end) * 0.5
        };
        self.union_text(center, &text);
        let kind = match p {
            Prim::Spline => PKind::Spline,
            Prim::Move => PKind::Move,
            _ => PKind::Line,
        };
        let tension = if matches!(p, Prim::Spline) {
            let mut t = None;
            for a in &obj.attrs {
                if let Attr::SplineTension(e) = a {
                    t = Some(self.eval_expr(e)?);
                }
            }
            t
        } else {
            None
        };
        // rpic `aligned`: rotate any aligned label to the segment's angle
        // (start → end), normalized to keep text upright.
        if text.iter().any(|l| l.aligned) {
            let seg = end - pts[0];
            if seg.x.abs() > 1e-9 || seg.y.abs() > 1e-9 {
                let deg = readable_angle(seg.y.atan2(seg.x).to_degrees());
                // a ~horizontal segment leaves the label upright (no transform)
                if deg.abs() > 1e-6 {
                    for l in text.iter_mut().filter(|l| l.aligned && l.rotate.is_none()) {
                        l.rotate = Some(deg);
                    }
                }
            }
        }
        let shape = if matches!(p, Prim::Spline) {
            Shape::Spline {
                pts: pts.clone(),
                tension,
                arrows,
                style,
                text,
            }
        } else {
            Shape::Path {
                pts: pts.clone(),
                closed,
                arrows,
                style,
                text,
            }
        };
        let layer = self.layer_of(obj, 0)?;
        self.push_shape(shape, layer);

        self.pos = end;
        self.dir = last_dir;
        let sh = self.shapes.len() - 1;
        let idx = self.record(kind, center, bb, pts[0], end, 0.0, Some(sh));
        self.placed[idx].points = pts;
        self.placed[idx].line_wid = line_wid;
        self.placed[idx].line_ht = line_ht;
        self.placed[idx].closed_path = closed;
        Ok(idx)
    }

    fn arc(&mut self, obj: &Object) -> ER<usize> {
        let mut style = self.style_of(obj)?;
        if let Some(wid) = self.dim(obj, DimKind::Wid)? {
            style.arrow_wid = wid;
        }
        if let Some(ht) = self.dim(obj, DimKind::Ht)? {
            style.arrow_ht = ht;
        }
        let text = self.text_of(obj)?;
        let start = self.find_from(obj)?.unwrap_or(self.pos);
        let cw = self.arc_cw_of(obj);
        let arc_dir = self.dir_of(obj);
        let rad_attr = self.dim(obj, DimKind::Rad)?;
        let to = self.dest_of(obj)?;
        let explicit_center = if to.is_some() {
            self.arc_explicit_center(obj)?
        } else {
            None
        };

        // (center, radius, start angle, end angle)
        let (center, r, a0, a1) = if let Some(end) = to {
            // arc from `start` to `to`, optional radius
            let chord = end - start;
            let clen = chord.len();
            if clen < 1e-9 {
                return err("degenerate arc: `from` and `to` coincide");
            }
            let r = rad_attr
                .unwrap_or(self.env_dim(EnvVar::Arcrad)?)
                .max(clen / 2.0);
            let dx = chord.x;
            let dy = chord.y;
            let ts = dx * dx + dy * dy;
            let mut t = ((4.0 * r * r - ts).max(0.0) / ts).sqrt();
            let arc_sign = if cw { -1.0 } else { 1.0 };
            // Dpic uses the prevailing direction to choose between the two
            // possible circle centers for a chord/radius pair.
            match arc_dir {
                Dir::Up => {
                    if arc_sign * ((-dx) - (t * dy)) < 0.0 {
                        t = -t;
                    }
                }
                Dir::Down => {
                    if arc_sign * ((-dx) - (t * dy)) > 0.0 {
                        t = -t;
                    }
                }
                Dir::Right => {
                    if arc_sign * (dy - (t * dx)) < 0.0 {
                        t = -t;
                    }
                }
                Dir::Left => {
                    if arc_sign * (dy - (t * dx)) > 0.0 {
                        t = -t;
                    }
                }
            }
            let center = start + Point::new(0.5 * (dx + t * dy), 0.5 * (dy - t * dx));
            let (center, r) = if let Some(center) = explicit_center {
                (center, end.dist(center))
            } else {
                (center, r)
            };
            let (a0, a1) = arc_angles(center, start, end, cw);
            (center, r, a0, a1)
        } else {
            // default: a quarter turn from the current heading
            let r = rad_attr.unwrap_or(self.env_dim(EnvVar::Arcrad)?);
            let din = dir_unit(self.dir);
            let normal = if cw {
                Point::new(din.y, -din.x)
            } else {
                Point::new(-din.y, din.x)
            };
            let center = start + normal * r;
            let a0 = (start - center).y.atan2((start - center).x);
            let sweep = if cw { -PI / 2.0 } else { PI / 2.0 };
            (center, r, a0, a0 + sweep)
        };

        let at = |t: f64| center + Point::new(t.cos(), t.sin()) * r;
        let end = at(a1);
        let arrows = self.arrows_of(obj, false);

        let mut bb = Bbox::new();
        bb.add(start);
        bb.add(end);
        for k in 0..=12 {
            bb.add(at(a0 + (a1 - a0) * (k as f64 / 12.0)));
        }
        self.layout_bbox.union(&bb);
        if !style.invis {
            self.bbox
                .union(&painted_bbox(&bb, stroke_half_width(&style)));
        }
        self.union_text(center, &text);

        let layer = self.layer_of(obj, 0)?;
        self.push_shape(
            Shape::Arc {
                c: center,
                r,
                a0,
                a1,
                cw,
                arrows,
                style,
                text,
            },
            layer,
        );

        // new heading is the tangent at the end point
        let tang = if a1 >= a0 {
            Point::new(-a1.sin(), a1.cos())
        } else {
            Point::new(a1.sin(), -a1.cos())
        };
        self.dir = nearest_dir(tang);
        self.pos = end;
        let sh = self.shapes.len() - 1;
        let idx = self.record(PKind::Arc, center, bb, start, end, 0.0, Some(sh));
        self.placed[idx].radius = r;
        Ok(idx)
    }

    fn arc_cw_of(&self, obj: &Object) -> bool {
        obj.attrs
            .iter()
            .rev()
            .find_map(|a| match a {
                Attr::Cw => Some(true),
                Attr::Ccw => Some(false),
                _ => None,
            })
            .unwrap_or(false)
    }

    fn arc_explicit_center(&mut self, obj: &Object) -> ER<Option<Point>> {
        for a in &obj.attrs {
            match a {
                Attr::At(pos) => return Ok(Some(self.eval_pos(pos)?)),
                Attr::With {
                    anchor: WithAnchor::Plain | WithAnchor::Corner(Corner::Center),
                    at,
                } => {
                    return Ok(Some(self.eval_pos(at)?));
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn text_obj(&mut self, obj: &Object) -> ER<usize> {
        let text = self.text_of(obj)?;
        if obj.attrs.iter().any(|a| matches!(a, Attr::Opacity(_))) {
            return err("opacity applies only to filled regions");
        }
        let dir = self.dir_of(obj);
        let w = self
            .dim(obj, DimKind::Wid)?
            .unwrap_or(self.env_dim(EnvVar::Textwid)?);
        let h = match self.dim(obj, DimKind::Ht)? {
            Some(h) => h,
            None => {
                // classic: textht per line; a styled line contributes its
                // fontsize ratio, so `"big" fontsize 22` gets a 2x line
                let lines: f64 = if text.is_empty() {
                    1.0
                } else {
                    text.iter().map(|l| l.height_factor()).sum()
                };
                self.env_dim(EnvVar::Textht)? * lines.max(1.0)
            }
        };
        let extent = if horizontal(dir) { w } else { h };
        let at = self.place_center(obj, dir, extent, w, h)?;
        let mut bb = Bbox::new();
        bb.add(at - Point::new(w / 2.0, h / 2.0));
        bb.add(at + Point::new(w / 2.0, h / 2.0));
        self.layout_bbox.union(&bb);
        let text_bb = text_object_bbox(at, &text, w, h);
        self.bbox.union(&text_bb);
        let layer = self.layer_of(obj, 0)?;
        self.push_shape(
            Shape::Text {
                at,
                text,
                bbox: text_bb,
                w,
                h,
                standalone: true,
            },
            layer,
        );
        let half = dir_unit(dir) * (extent / 2.0);
        let start = at - half;
        let end = at + half;
        self.pos = end;
        self.dir = dir;
        let sh = self.shapes.len() - 1;
        Ok(self.record(PKind::Text, at, bb, start, end, 0.0, Some(sh)))
    }

    /// Union an estimated text extent into the drawing bbox, so wide labels and
    /// bare text objects aren't clipped by the SVG viewBox.
    fn union_text(&mut self, center: Point, lines: &[TextLine]) {
        let bb = text_bbox(center, lines);
        self.bbox.union(&bb);
    }

    fn block(&mut self, stmts: &[Stmt], obj: &Object) -> ER<usize> {
        let block_text = self.text_of(obj)?;
        let block_fill_opacity = self.style_of(obj)?.fill_opacity;
        // Evaluate the block in a local scope at its own origin. Labels from
        // the containing scope are visible for references such as `$1.start`
        // inside macro-generated blocks, but are not captured as new members.
        let mut sub = State::new();
        sub.env = self.env.clone();
        sub.vars = self.vars.clone();
        sub.inherited_vars = self.vars.keys().cloned().collect();
        sub.export_vars.clear();
        sub.macros = self.macros.clone();
        sub.includes = self.includes.clone();
        sub.rng = self.rng.clone();
        // expose this scope's labels (read-only, absolute coords) to the block
        sub.outer_labels = self.outer_labels.clone();
        for (name, &i) in &self.labels {
            sub.outer_labels
                .insert(name.clone(), self.placed[i].clone());
        }
        sub.eval_stmts(stmts)?;
        // Variables, parameters and direction changes inside `[ ... ]` are
        // local to the block. Random draws still consume the shared sequence.
        self.rng = sub.rng.clone();
        self.diagnostics.append(&mut sub.diagnostics);
        self.warnings.append(&mut sub.warnings);
        for key in &sub.export_vars {
            if let Some(val) = sub.vars.get(key).copied() {
                self.vars.insert(key.clone(), val);
                if self.inherited_vars.contains(key) {
                    self.export_vars.insert(key.clone());
                }
            }
        }

        let layout_sub_bb = if sub.layout_bbox.is_empty() {
            let mut b = Bbox::new();
            b.add(Point::ZERO);
            b
        } else {
            sub.layout_bbox
        };
        let local_center = (layout_sub_bb.min + layout_sub_bb.max) * 0.5;
        let w = layout_sub_bb.width();
        let h = layout_sub_bb.height();

        let dir = self.dir_of(obj);
        let extent = if horizontal(dir) { w } else { h };
        let target = self.block_center(obj, dir, extent, w, h, local_center, &mut sub)?;
        let shift = target - local_center;

        let first_shape = self.shapes.len();
        // capture inner labels (translated into parent space) before the block's
        // shapes are moved out, so `B.A` / `last [].Outer` can resolve.
        let mut members: HashMap<String, Placed> = HashMap::new();
        for (name, &i) in &sub.labels {
            let mut pl = sub.placed[i].clone();
            rebase_placed(&mut pl, shift, first_shape);
            members.insert(name.clone(), pl);
        }
        let layer_shift =
            self.layer_shift_for(obj, sub.shape_layers.iter().copied().max().unwrap_or(0))?;
        for (((mut sh, layer), class), span) in sub
            .shapes
            .into_iter()
            .zip(sub.shape_layers)
            .zip(sub.shape_classes)
            .zip(sub.shape_spans)
        {
            translate_shape(&mut sh, shift);
            if let Some(opacity) = block_fill_opacity {
                multiply_shape_fill_opacity(&mut sh, opacity);
            }
            self.push_shape(sh, layer + layer_shift);
            *self.shape_classes.last_mut().unwrap() = class;
            // inner objects carry their own statement spans through the merge
            *self.shape_spans.last_mut().unwrap() = span;
        }
        // The block's child shapes occupy [first_shape, child_end); captured
        // before the block's own centred label (pushed below) so a `stagger`
        // fans across the members, not the title.
        let child_end = self.shapes.len();
        if let Some(c) = sub.canvas {
            // like variables, `canvas` is global: a block's setting wins,
            // translated into parent space with the rest of its geometry
            let mut bb = Bbox::new();
            bb.add(c.min + shift);
            bb.add(c.max + shift);
            self.canvas = Some(bb);
        }
        let shape = if self.shapes.len() > first_shape {
            Some(first_shape)
        } else {
            None
        };
        let mut bb = Bbox::new();
        bb.add(layout_sub_bb.min + shift);
        bb.add(layout_sub_bb.max + shift);
        self.layout_bbox.union(&bb);
        if !sub.bbox.is_empty() {
            let mut visible_bb = Bbox::new();
            visible_bb.add(sub.bbox.min + shift);
            visible_bb.add(sub.bbox.max + shift);
            self.bbox.union(&visible_bb);
        }
        let block_text_bb = text_bbox(target, &block_text);
        self.bbox.union(&block_text_bb);
        if has_visible_text(&block_text) {
            let layer = self.layer_of(obj, 0)?;
            self.push_shape(
                Shape::Text {
                    at: target,
                    text: block_text,
                    bbox: block_text_bb,
                    w: 0.0,
                    h: 0.0,
                    standalone: false,
                },
                layer,
            );
        }

        let half = dir_unit(dir) * (extent / 2.0);
        let start = target - half;
        let end = target + half;
        self.pos = end;
        self.dir = dir;
        let idx = self.record(PKind::Block, target, bb, start, end, 0.0, shape);
        self.placed[idx].members = members;
        if child_end > first_shape {
            self.placed[idx].block_shapes = Some((first_shape, child_end));
        }
        Ok(idx)
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        &mut self,
        kind: PKind,
        center: Point,
        bbox: Bbox,
        start: Point,
        end: Point,
        thick: f64,
        shape: Option<usize>,
    ) -> usize {
        let idx = self.placed.len();
        self.placed.push(Placed {
            kind,
            center,
            bbox,
            start,
            end,
            thick,
            points: Vec::new(),
            radius: 0.0,
            box_rad: 0.0,
            line_wid: 0.0,
            line_ht: 0.0,
            closed_path: false,
            layer: shape
                .and_then(|s| self.shape_layers.get(s).copied())
                .unwrap_or(0),
            shape,
            members: HashMap::new(),
            block_shapes: None,
        });
        idx
    }

    fn push_shape(&mut self, shape: Shape, layer: i32) {
        self.shapes.push(shape);
        self.shape_layers.push(layer);
        self.shape_classes.push(None);
        self.shape_spans.push(self.current_span.clone());
    }

    fn layer_of(&mut self, obj: &Object, current: i32) -> ER<i32> {
        let mut layer = current;
        for a in &obj.attrs {
            if let Attr::Behind(place) = a {
                let target = self.resolve_obj(place)?;
                if layer >= target.layer {
                    layer = target.layer - 1;
                }
            }
        }
        Ok(layer)
    }

    fn layer_shift_for(&mut self, obj: &Object, current_max: i32) -> ER<i32> {
        Ok(self.layer_of(obj, current_max)? - current_max)
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
                        s.dash = Dash::Dashed(w);
                    }
                    LineType::Dotted => {
                        s.dash = Dash::Dotted(match opt {
                            Some(e) => Some(self.expr_dim(e)?),
                            None => None,
                        });
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
                Attr::Arrowhead(_, Some(e)) => {
                    s.arrow_filled = self.eval_expr(e)?.round() as i64 != 0;
                }
                _ => {}
            }
        }
        Ok(s)
    }

    fn eval_color_expr(&mut self, se: &StringExpr) -> ER<String> {
        if let StringExpr::Lit(name) = se
            && let Some(body) = self.macros.get(name).cloned()
        {
            if let Some(lit) = single_token_macro_string(&body) {
                return Ok(lit);
            }
            let parsed =
                crate::parser::parse_stringexpr_tokens(&body, &mut self.macros, &self.includes)
                    .map_err(parse_eval_error)?;
            return self.eval_stringexpr(&parsed);
        }
        self.eval_stringexpr(se)
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

    // ---- positions & places ------------------------------------------------

    fn eval_pos(&mut self, pos: &Position) -> ER<Point> {
        match pos {
            Position::Pair(x, y) => Ok(Point::new(self.expr_dim(x)?, self.expr_dim(y)?)),
            Position::Place(loc) => self.eval_loc(loc),
            Position::Between { frac, a, b, .. } => {
                let f = self.eval_expr(frac)?;
                let pa = self.eval_pos(a)?;
                let pb = self.eval_pos(b)?;
                Ok(pa.lerp(pb, f))
            }
            Position::Sum(sign, a, b) => {
                let pa = self.eval_pos(a)?;
                let pb = self.eval_pos(b)?;
                Ok(match sign {
                    Sign::Plus => pa + pb,
                    Sign::Minus => pa - pb,
                })
            }
            Position::Scale(p, s, div) => {
                let pp = self.eval_pos(p)?;
                let sv = self.eval_expr(s)?;
                if *div {
                    if sv == 0.0 {
                        return err("division by zero in position");
                    }
                    Ok(pp / sv)
                } else {
                    Ok(pp * sv)
                }
            }
        }
    }

    fn eval_loc(&mut self, loc: &Location) -> ER<Point> {
        match loc {
            Location::Place(p) => self.place_point(p),
            Location::Paren(pos) => self.eval_pos(pos),
            Location::ParenPair(p1, p2) => {
                Ok(Point::new(self.eval_pos(p1)?.x, self.eval_pos(p2)?.y))
            }
        }
    }

    fn place_point(&mut self, p: &Place) -> ER<Point> {
        match p {
            Place::Here => Ok(self.pos),
            Place::Name {
                name,
                subscript,
                span,
            } => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                Ok(self.resolve_label(&key, span.as_ref())?.center)
            }
            Place::Nth { count, obj, span } => {
                let idx = self.nth_index(count, obj, span.as_ref())?;
                Ok(self.placed[idx].center)
            }
            Place::Corner(inner, c) => {
                let pl = self.resolve_obj(inner)?;
                Ok(pl.corner(*c))
            }
            Place::CornerOf(c, inner) => {
                let pl = self.resolve_obj(inner)?;
                Ok(pl.corner(*c))
            }
            Place::Member(..) => Ok(self.resolve_obj(p)?.center),
        }
    }

    /// Resolve an object-valued place to its [`Placed`] record, descending into
    /// block members for `B.A` / `last [].Outer`.
    fn resolve_obj(&mut self, p: &Place) -> ER<Placed> {
        match p {
            Place::Name {
                name,
                subscript,
                span,
            } => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.resolve_label(&key, span.as_ref())
            }
            Place::Nth { count, obj, span } => {
                let idx = self.nth_index(count, obj, span.as_ref())?;
                Ok(self.placed[idx].clone())
            }
            Place::Corner(inner, _) | Place::CornerOf(_, inner) => self.resolve_obj(inner),
            Place::Member(base, sub) => {
                let b = self.resolve_obj(base)?;
                let key = match sub.as_ref() {
                    Place::Name {
                        name, subscript, ..
                    } => self.indexed_name(name, subscript.as_deref())?,
                    _ => return err("a block sub-label must be a name"),
                };
                b.members.get(&key).cloned().ok_or_else(|| EvalError {
                    msg: format!("no sub-label `{key}` in that block"),
                    info: None,
                })
            }
            Place::Here => err("`Here` is a point, not an object"),
        }
    }

    fn place_index(&mut self, p: &Place) -> ER<usize> {
        match p {
            Place::Name {
                name,
                subscript,
                span,
            } => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.label_index(&key, span.as_ref())
            }
            Place::Nth { count, obj, span } => self.nth_index(count, obj, span.as_ref()),
            Place::Corner(inner, _) | Place::CornerOf(_, inner) => self.place_index(inner),
            Place::Here => err("`Here` is a point, not an object"),
            Place::Member(_, _) => err("block sub-labels (B.A) are not supported yet"),
        }
    }

    fn label_index(&self, name: &str, span: Option<&Span>) -> ER<usize> {
        self.labels
            .get(name)
            .copied()
            .ok_or_else(|| unknown_label_error(name, span))
    }

    /// Resolve a label to its [`Placed`], falling back to enclosing-scope labels
    /// (absolute coordinates) so a block can reference outer labels.
    fn resolve_label(&self, key: &str, span: Option<&Span>) -> ER<Placed> {
        if let Some(&idx) = self.labels.get(key) {
            Ok(self.placed[idx].clone())
        } else if let Some(pl) = self.outer_labels.get(key) {
            Ok(pl.clone())
        } else {
            Err(unknown_label_error(key, span))
        }
    }

    fn label_key(&mut self, label: &Label) -> ER<String> {
        self.indexed_name(&label.name, label.subscript.as_ref())
    }

    fn indexed_name(&mut self, name: &str, subscript: Option<&Expr>) -> ER<String> {
        match subscript {
            Some(Expr::Index(items)) => {
                let mut parts = Vec::with_capacity(items.len());
                for e in items {
                    parts.push(fmt_num(self.eval_expr(e)?));
                }
                Ok(format!("{name}[{}]", parts.join(",")))
            }
            Some(e) => Ok(format!("{name}[{}]", fmt_num(self.eval_expr(e)?))),
            None => Ok(name.to_string()),
        }
    }

    fn nth_index(&mut self, count: &Nth, obj: &PrimObj, span: Option<&Span>) -> ER<usize> {
        // Untyped `last` matches the most recent object of any kind; a typed
        // reference (`last box`) filters to that kind.
        let want = primobj_kind(obj);
        let matches: Vec<usize> = self
            .placed
            .iter()
            .enumerate()
            .filter(|(_, pl)| want.is_none_or(|w| pl.kind == w))
            .map(|(i, _)| i)
            .collect();
        if matches.is_empty() {
            return match want {
                Some(w) => err(format!("no {:?} object to reference", want_name(w))),
                None => err("no object to reference".to_string()),
            };
        }
        let idx = match count {
            Nth::Last => *matches.last().unwrap(),
            Nth::Count(e, from_last) => {
                let n = self.eval_expr(e)?.round() as i64;
                if n < 1 {
                    return err("ordinal must be >= 1");
                }
                let k = (n - 1) as usize;
                if *from_last {
                    if k >= matches.len() {
                        return err_diag(ordinal_diagnostic(n, matches.len(), span));
                    }
                    matches[matches.len() - 1 - k]
                } else {
                    if k >= matches.len() {
                        return err_diag(ordinal_diagnostic(n, matches.len(), span));
                    }
                    matches[k]
                }
            }
        };
        Ok(idx)
    }

    // ---- string expressions ------------------------------------------------

    fn eval_stringexpr(&mut self, se: &StringExpr) -> ER<String> {
        Ok(match se {
            StringExpr::Lit(s) => s.clone(),
            StringExpr::Concat(a, b) => {
                format!("{}{}", self.eval_stringexpr(a)?, self.eval_stringexpr(b)?)
            }
            StringExpr::Arg(n) => format!("${n}"), // should have been expanded
            StringExpr::Sprintf(fmt, args) => {
                let f = self.eval_stringexpr(fmt)?;
                let mut vals = Vec::with_capacity(args.len());
                for e in args {
                    vals.push(self.eval_printf_arg(e)?);
                }
                sprintf_fmt(&f, &vals)
            }
            StringExpr::SvgFont(_) => String::new(),
            StringExpr::Rgb(args) => {
                let mut c = [0u32; 3];
                for (i, e) in args.iter().enumerate() {
                    let v = self.eval_expr(e)?;
                    if !v.is_finite() || !(0.0..=255.0).contains(&v.round()) {
                        return err("rgb() component out of range 0-255");
                    }
                    c[i] = v.round() as u32;
                }
                format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
            }
            StringExpr::ColorNum(e) => {
                let v = self.eval_expr(e)?;
                if !v.is_finite() || v.round() < 0.0 || v.round() > 0xFFFFFF as f64 {
                    return err("numeric color out of range 0-0xFFFFFF");
                }
                format!("#{:06x}", v.round() as u32)
            }
        })
    }

    fn eval_printf_arg(&mut self, e: &Expr) -> ER<PrintfArg> {
        match e {
            Expr::Str(se) => Ok(PrintfArg::Str(self.eval_stringexpr(se)?)),
            _ => Ok(PrintfArg::Num(self.eval_expr(e)?)),
        }
    }

    // ---- expressions -------------------------------------------------------

    /// Evaluate an operand to its string form (for `==`/`!=`); a numeric operand
    /// is rendered textually so `"$1" == "2"` style comparisons work uniformly.
    fn expr_str(&mut self, e: &Expr) -> ER<String> {
        match e {
            Expr::Str(se) => self.eval_stringexpr(se),
            _ => Ok(fmt_num(self.eval_expr(e)?)),
        }
    }

    fn eval_expr(&mut self, e: &Expr) -> ER<f64> {
        let value = match e {
            Expr::Num(v) => *v,
            Expr::Str(_) => return err("a string is only valid as an `==`/`!=` operand"),
            Expr::Index(_) => return err("a comma subscript is only valid inside `name[...]`"),
            Expr::Var(name, subscript) => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.vars.get(&key).copied().ok_or_else(|| EvalError {
                    msg: format!("variable not found `{key}`"),
                    info: None,
                })?
            }
            Expr::Env(v) => self.env.get(*v),
            Expr::Unary(op, a) => {
                let x = self.eval_expr(a)?;
                match op {
                    UnOp::Neg => -x,
                    UnOp::Pos => x,
                    UnOp::Not => bool_f(x == 0.0),
                }
            }
            Expr::Bin(op, a, b) => {
                // string equality: pic compares string operands with `==`/`!=`
                if matches!(op, BinOp::Eq | BinOp::Ne)
                    && (matches!(a.as_ref(), Expr::Str(_)) || matches!(b.as_ref(), Expr::Str(_)))
                {
                    let sa = self.expr_str(a)?;
                    let sb = self.expr_str(b)?;
                    return Ok(bool_f(if matches!(op, BinOp::Eq) {
                        sa == sb
                    } else {
                        sa != sb
                    }));
                }
                let x = self.eval_expr(a)?;
                let y = self.eval_expr(b)?;
                match op {
                    BinOp::Add => x + y,
                    BinOp::Sub => x - y,
                    BinOp::Mul => x * y,
                    BinOp::Div => {
                        if y == 0.0 {
                            return err("division by zero");
                        }
                        x / y
                    }
                    BinOp::Mod => dpic_mod(x, y, "modulo by zero")?,
                    BinOp::Pow => dpic_pow(x, y)?,
                    BinOp::Eq => bool_f(x == y),
                    BinOp::Ne => bool_f(x != y),
                    BinOp::Lt => bool_f(x < y),
                    BinOp::Le => bool_f(x <= y),
                    BinOp::Gt => bool_f(x > y),
                    BinOp::Ge => bool_f(x >= y),
                    BinOp::And => bool_f(x != 0.0 && y != 0.0),
                    BinOp::Or => bool_f(x != 0.0 || y != 0.0),
                }
            }
            Expr::Func1(f, a) => {
                let x = self.eval_expr(a)?;
                match f {
                    Func1::Abs => x.abs(),
                    Func1::Acos => x.acos(),
                    Func1::Asin => x.asin(),
                    Func1::Cos => x.cos(),
                    Func1::Exp => 10f64.powf(x),
                    Func1::Expe => x.exp(),
                    Func1::Int => x.trunc(),
                    Func1::Log => x.log10(),
                    Func1::Loge => x.ln(),
                    Func1::Sign => {
                        if x >= 0.0 {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                    Func1::Sin => x.sin(),
                    Func1::Sqrt => x.sqrt(),
                    Func1::Tan => x.tan(),
                    Func1::Floor => x.floor(),
                }
            }
            Expr::Func2(f, a, b) => {
                let x = self.eval_expr(a)?;
                let y = self.eval_expr(b)?;
                match f {
                    Func2::Atan2 => x.atan2(y),
                    Func2::Max => x.max(y),
                    Func2::Min => x.min(y),
                    Func2::Pmod => x.rem_euclid(y),
                }
            }
            Expr::Rand(seed) => {
                let seed = match seed {
                    Some(e) => Some(self.eval_expr(e)?),
                    None => None,
                };
                self.rand(seed)
            }
            Expr::Assign(name, subscript, v) => {
                let val = self.eval_expr(v)?;
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.vars.insert(key, val);
                val
            }
            Expr::DotX(loc) => {
                let x = self.eval_loc(loc)?.x;
                self.to_user_dim(x)
            }
            Expr::DotY(loc) => {
                let y = self.eval_loc(loc)?.y;
                self.to_user_dim(y)
            }
            Expr::PlaceAttr(place, param) => {
                let pl = self.resolve_obj(place)?;
                match param {
                    token::Param::Width => self.to_user_dim(pl.attr_width()),
                    token::Param::Height => self.to_user_dim(pl.attr_height()),
                    token::Param::Radius => self.to_user_dim(pl.attr_radius()),
                    token::Param::Diameter => self.to_user_dim(pl.attr_diameter()),
                    token::Param::Length => self.to_user_dim(pl.attr_length()),
                    token::Param::Thickness => pl.thick,
                }
            }
        };
        finite(value, "numeric expression")
    }

    fn rand(&mut self, seed: Option<f64>) -> f64 {
        if let Some(seed) = seed {
            self.rng.seed(seed.trunc() as i64);
        }
        self.rng.next_f64()
    }
}

// ---- free helpers ----------------------------------------------------------

fn apply_op(op: AssignOp, cur: f64, rhs: f64) -> ER<f64> {
    let value = match op {
        AssignOp::Set | AssignOp::ColonSet => rhs,
        AssignOp::Add => cur + rhs,
        AssignOp::Sub => cur - rhs,
        AssignOp::Mul => cur * rhs,
        AssignOp::Div => {
            if rhs == 0.0 {
                return err("division by zero in assignment");
            }
            cur / rhs
        }
        AssignOp::Rem => dpic_mod(cur, rhs, "modulo by zero in assignment")?,
    };
    finite(value, "assignment")
}

#[derive(Clone)]
struct GlibcRand {
    state: [u32; 31],
    f: usize,
    r: usize,
}

impl GlibcRand {
    fn new(seed: i64) -> Self {
        let mut rng = GlibcRand {
            state: [0; 31],
            f: 3,
            r: 0,
        };
        rng.seed(seed);
        rng
    }

    fn seed(&mut self, seed: i64) {
        let seed = if seed == 0 { 1 } else { seed };
        let mut x = seed.rem_euclid(2_147_483_647) as u32;
        if x == 0 {
            x = 1;
        }
        self.state[0] = x;
        for i in 1..31 {
            let prev = self.state[i - 1] as i64;
            let hi = prev / 127_773;
            let lo = prev % 127_773;
            let mut next = 16_807 * lo - 2_836 * hi;
            if next < 0 {
                next += 2_147_483_647;
            }
            self.state[i] = next as u32;
        }
        self.f = 3;
        self.r = 0;
        for _ in 0..310 {
            self.next_i31();
        }
    }

    fn next_i31(&mut self) -> u32 {
        let x = self.state[self.f].wrapping_add(self.state[self.r]);
        self.state[self.f] = x;
        let out = (x >> 1) & 0x7fff_ffff;
        self.f = (self.f + 1) % 31;
        self.r = (self.r + 1) % 31;
        out
    }

    fn next_f64(&mut self) -> f64 {
        self.next_i31() as f64 / 2_147_483_647.0
    }
}

fn bool_f(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

fn dpic_round(x: f64) -> i64 {
    if x < 0.0 {
        -((-x + 0.5).floor() as i64)
    } else {
        (x + 0.5).floor() as i64
    }
}

fn dpic_mod(x: f64, y: f64, zero_msg: &'static str) -> ER<f64> {
    let i = dpic_round(x);
    let j = dpic_round(y);
    if j == 0 {
        return err(zero_msg);
    }
    Ok((i - (i / j) * j) as f64)
}

fn dpic_pow(x: f64, y: f64) -> ER<f64> {
    if x == 0.0 && y < 0.0 {
        return err("zero cannot be raised to a negative power");
    }
    let iy = dpic_round(y);
    if iy as f64 == y {
        return Ok(dpic_int_pow(x, iy));
    }
    if x < 0.0 {
        return err("negative base with non-integer exponent");
    }
    Ok(x.powf(y))
}

fn dpic_int_pow(x: f64, y: i64) -> f64 {
    if y == 0 {
        return 1.0;
    }
    if x == 0.0 || y == 1 {
        return x;
    }
    if y < 0 {
        return dpic_int_pow(1.0 / x, -y);
    }
    if y == 2 {
        return x * x;
    }
    if y & 1 == 1 {
        x * dpic_int_pow(x, y - 1)
    } else {
        let half = dpic_int_pow(x, y >> 1);
        half * half
    }
}

fn apply_text_pos(halign: &mut i8, valign: &mut i8, pos: token::TextPos) {
    match pos {
        token::TextPos::Ljust => *halign = -1,
        token::TextPos::Rjust => *halign = 1,
        token::TextPos::Center => {
            *halign = 0;
            *valign = 0;
        }
        token::TextPos::Above => *valign = 1,
        token::TextPos::Below => *valign = -1,
    }
}

fn ensure_hatch(style: &mut Style) -> &mut Hatch {
    style.hatch.get_or_insert_with(|| Hatch {
        cross: false,
        angle: DEFAULT_HATCH_ANGLE,
        sep: DEFAULT_HATCH_SEP,
        width: DEFAULT_HATCH_WIDTH,
        color: "black".into(),
    })
}

/// Validate a `class` extension name list: whitespace-separated tokens, each
/// matching `[A-Za-z_][A-Za-z0-9_-]*`. Rejecting everything else keeps the
/// hook free of attribute-injection surface.
fn validate_class(name: &str) -> ER<()> {
    if name.is_empty() {
        return err("class name must not be empty");
    }
    for tok in name.split_whitespace() {
        let mut chars = tok.chars();
        let first = chars.next().unwrap();
        if !(first.is_ascii_alphabetic() || first == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return err(format!(
                "invalid class name `{tok}`: use `[A-Za-z_][A-Za-z0-9_-]*`"
            ));
        }
    }
    Ok(())
}

fn ensure_gradient(style: &mut Style) -> &mut Gradient {
    style.gradient.get_or_insert_with(|| Gradient {
        from: "black".into(),
        to: "white".into(),
        angle: 0.0,
    })
}

fn has_visible_text(lines: &[TextLine]) -> bool {
    lines.iter().any(|line| !line.s.is_empty())
}

fn closed_shape_is_visible(style: &Style) -> bool {
    !style.invis || style.fill.is_some() || style.hatch.is_some() || style.gradient.is_some()
}

fn open_fill_is_visible(style: &Style) -> bool {
    style.fill_open && (style.fill.is_some() || style.hatch.is_some() || style.gradient.is_some())
}

fn stroke_half_width(style: &Style) -> f64 {
    if style.invis {
        0.0
    } else {
        style.thick.unwrap_or(0.8) / 144.0
    }
}

fn painted_bbox(bb: &Bbox, pad: f64) -> Bbox {
    if bb.is_empty() || pad <= 0.0 {
        return *bb;
    }
    let mut out = Bbox::new();
    out.add(bb.min - Point::new(pad, pad));
    out.add(bb.max + Point::new(pad, pad));
    out
}

fn dpic_box_layout_bbox(center: Point, w: f64, h: f64) -> Bbox {
    fn axis(center: f64, extent: f64) -> (f64, f64) {
        let lo = center - extent / 2.0;
        let hi = center + extent / 2.0;
        if lo <= hi { (lo, hi) } else { (0.0, 0.0) }
    }

    let (x0, x1) = axis(center.x, w);
    let (y0, y1) = axis(center.y, h);
    let mut bb = Bbox::new();
    bb.add(Point::new(x0, y0));
    bb.add(Point::new(x1, y1));
    bb
}

fn drawing_painted_bbox(shapes: &[Shape]) -> Bbox {
    let mut out = Bbox::new();
    for sh in shapes {
        out.union(&shape_painted_bbox(sh));
    }
    out
}

fn shape_painted_bbox(sh: &Shape) -> Bbox {
    let mut out = Bbox::new();
    match sh {
        Shape::Box {
            c,
            w,
            h,
            style,
            text,
            ..
        } => {
            let mut bb = Bbox::new();
            bb.add(*c - Point::new(*w / 2.0, *h / 2.0));
            bb.add(*c + Point::new(*w / 2.0, *h / 2.0));
            if closed_shape_is_visible(style) {
                out.union(&painted_bbox(&bb, stroke_half_width(style)));
            }
            out.union(&text_bbox(*c, text));
        }
        Shape::Circle { c, r, style, text } => {
            let mut bb = Bbox::new();
            bb.add(*c - Point::new(*r, *r));
            bb.add(*c + Point::new(*r, *r));
            if closed_shape_is_visible(style) {
                out.union(&painted_bbox(&bb, stroke_half_width(style)));
            }
            out.union(&text_bbox(*c, text));
        }
        Shape::Ellipse {
            c,
            w,
            h,
            style,
            text,
        } => {
            let mut bb = Bbox::new();
            bb.add(*c - Point::new(*w / 2.0, *h / 2.0));
            bb.add(*c + Point::new(*w / 2.0, *h / 2.0));
            if closed_shape_is_visible(style) {
                out.union(&painted_bbox(&bb, stroke_half_width(style)));
            }
            out.union(&text_bbox(*c, text));
        }
        Shape::Path {
            pts,
            closed,
            style,
            text,
            ..
        } => {
            let mut bb = Bbox::new();
            for p in pts {
                bb.add(*p);
            }
            if open_fill_is_visible(style) {
                out.union(&bb);
            }
            if !style.invis {
                out.union(&painted_bbox(&bb, stroke_half_width(style)));
            } else if style.invis_bounds {
                out.union(&bb);
            }
            if !bb.is_empty() {
                let text_at = if *closed {
                    (bb.min + bb.max) * 0.5
                } else if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
                    (*first + *last) * 0.5
                } else {
                    Point::ZERO
                };
                out.union(&text_bbox(text_at, text));
            }
        }
        Shape::Spline {
            pts, style, text, ..
        } => {
            let mut bb = Bbox::new();
            for p in pts {
                bb.add(*p);
            }
            if open_fill_is_visible(style) {
                out.union(&bb);
            }
            if !style.invis {
                out.union(&painted_bbox(&bb, stroke_half_width(style)));
            } else if style.invis_bounds {
                out.union(&bb);
            }
            if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
                out.union(&text_bbox((*first + *last) * 0.5, text));
            }
        }
        Shape::Arc {
            c,
            r,
            a0,
            a1,
            style,
            text,
            ..
        } => {
            let at = |t: f64| *c + Point::new(t.cos(), t.sin()) * *r;
            let mut bb = Bbox::new();
            for k in 0..=12 {
                bb.add(at(*a0 + (*a1 - *a0) * (k as f64 / 12.0)));
            }
            if open_fill_is_visible(style) {
                out.union(&bb);
            }
            if !style.invis {
                out.union(&painted_bbox(&bb, stroke_half_width(style)));
            }
            out.union(&text_bbox(*c, text));
        }
        Shape::Brace {
            cubics,
            label_at,
            style,
            text,
            ..
        } => {
            let bb = cubics_bbox(cubics);
            if !style.invis {
                out.union(&painted_bbox(&bb, stroke_half_width(style)));
            } else if style.invis_bounds {
                out.union(&bb);
            }
            out.union(&text_bbox(*label_at, text));
        }
        Shape::Text { bbox, .. } => out.union(bbox),
    }
    out
}

fn brace_side(unit: Point, side_dir: Option<Dir>) -> Point {
    let right = Point::new(unit.y, -unit.x);
    if let Some(d) = side_dir {
        let target = dir_unit(d);
        if right.x * target.x + right.y * target.y < 0.0 {
            right * -1.0
        } else {
            right
        }
    } else {
        right
    }
}

fn brace_cubics(a: Point, b: Point, depth: Point, pos: f64) -> Vec<[Point; 4]> {
    let v = b - a;
    let len = v.len();
    let depth_len = depth.len();
    if len <= 1e-12 || depth_len <= 1e-12 {
        return vec![[a, a, b, b]];
    }
    let u = v / len;
    let side = depth / depth_len;
    let left_len = len * pos;
    let right_len = len - left_len;
    let depth_len = depth_len.min(left_len * 0.45).min(right_len * 0.45);
    let shoulder = depth_len * 0.45;
    let curl = depth_len - shoulder;
    let k = 0.552_284_749_830_793_6;
    let at = |x: f64, y: f64| a + u * x + side * y;
    let line = |p0: Point, p1: Point| [p0, p0, p1, p1];

    let p0 = at(0.0, 0.0);
    let p1 = at(shoulder, shoulder);
    let p2_x = (left_len - curl).max(shoulder);
    let p2 = at(p2_x, shoulder);
    let cusp = at(left_len, depth_len);
    let p3_x = (left_len + curl).min(len - shoulder);
    let p3 = at(p3_x, shoulder);
    let p4_x = len - shoulder;
    let p4 = at(p4_x, shoulder);
    let p5 = at(len, 0.0);

    vec![
        [
            p0,
            at(0.0, k * shoulder),
            at(shoulder - k * shoulder, shoulder),
            p1,
        ],
        line(p1, p2),
        [
            p2,
            at(p2_x + k * curl, shoulder),
            at(left_len, depth_len - k * curl),
            cusp,
        ],
        [
            cusp,
            at(left_len, depth_len - k * curl),
            at(p3_x - k * curl, shoulder),
            p3,
        ],
        line(p3, p4),
        [
            p4,
            at(p4_x + k * shoulder, shoulder),
            at(len, k * shoulder),
            p5,
        ],
    ]
}

fn brace_cusp(cubics: &[[Point; 4]]) -> Option<Point> {
    cubics.get(2).map(|c| c[3])
}

fn cubic_at(c: &[Point; 4], t: f64) -> Point {
    let mt = 1.0 - t;
    c[0] * (mt * mt * mt)
        + c[1] * (3.0 * mt * mt * t)
        + c[2] * (3.0 * mt * t * t)
        + c[3] * (t * t * t)
}

fn sample_cubics(cubics: &[[Point; 4]], steps: usize) -> Vec<Point> {
    let steps = steps.max(1);
    let mut pts = Vec::new();
    for (i, c) in cubics.iter().enumerate() {
        if i == 0 {
            pts.push(c[0]);
        }
        for step in 1..=steps {
            pts.push(cubic_at(c, step as f64 / steps as f64));
        }
    }
    pts
}

fn cubics_bbox(cubics: &[[Point; 4]]) -> Bbox {
    let mut bb = Bbox::new();
    for c in cubics {
        for p in c {
            bb.add(*p);
        }
    }
    for p in sample_cubics(cubics, 12) {
        bb.add(p);
    }
    bb
}

/// Font attributes seen before their string (they bind to the next one).
#[derive(Default)]
struct PendingStyle {
    bold: bool,
    italic: bool,
    family: Option<String>,
    size_pt: Option<f64>,
    rotate: Option<f64>,
    aligned: bool,
}

/// Line advance width: exact metrics for typeset math, the classic
/// 0.6 em/char estimate otherwise (scaled by the line's font style).
fn text_line_width(line: &TextLine) -> f64 {
    match &line.math {
        Some(m) => m.width,
        None => line.s.chars().count() as f64 * TEXT_CHAR_W * line.width_factor(),
    }
}

fn text_bbox(center: Point, lines: &[TextLine]) -> Bbox {
    let mut bb = Bbox::new();
    if !has_visible_text(lines) {
        return bb;
    }
    let n = lines.len() as f64;
    for (i, line) in lines.iter().enumerate() {
        if line.s.is_empty() {
            continue;
        }
        let w = text_line_width(line);
        let base_y = center.y - (i as f64 - (n - 1.0) / 2.0) * TEXT_LINE_H;
        let y = base_y + line.valign as f64 * (TEXT_XHEIGHT / 2.0 + line.text_offset);
        let x = center.x
            + match line.halign {
                -1 => line.text_offset,
                1 => -line.text_offset,
                _ => 0.0,
            };
        let (min_x, max_x) = match line.halign {
            -1 => (x, x + w),
            1 => (x - w, x),
            _ => (x - w / 2.0, x + w / 2.0),
        };
        match &line.math {
            Some(m) => {
                // mirror the SVG backend's box placement: centered formulas
                // center their ink box on base_y + xheight/2; above/below
                // formulas keep the whole box clear of the reference
                let half = (m.height + m.depth) / 2.0;
                let center = match line.valign {
                    1 => base_y + TEXT_XHEIGHT / 2.0 + line.text_offset + half,
                    -1 => base_y - TEXT_XHEIGHT / 2.0 - line.text_offset - half,
                    _ => base_y + TEXT_XHEIGHT / 2.0,
                };
                bb.add(Point::new(min_x, center - half));
                bb.add(Point::new(max_x, center + half));
            }
            None => {
                let half_h = TEXT_LINE_H * line.height_factor() / 2.0;
                let (min, max) = (Point::new(min_x, y - half_h), Point::new(max_x, y + half_h));
                match line.rotate {
                    Some(deg) => bb.add_rect_rotated(min, max, deg),
                    None => {
                        bb.add(min);
                        bb.add(max);
                    }
                }
            }
        }
    }
    bb
}

fn fitted_text_size(lines: &[TextLine]) -> Option<(f64, f64)> {
    if !has_visible_text(lines) {
        return None;
    }
    let bb = text_bbox(Point::ZERO, lines);
    if bb.is_empty() {
        return None;
    }
    let half_w = bb.min.x.abs().max(bb.max.x.abs());
    let half_h = bb.min.y.abs().max(bb.max.y.abs());
    Some((
        2.0 * half_w + TEXT_CHAR_W,
        2.0 * half_h + TEXT_XHEIGHT / 2.0,
    ))
}

fn text_object_bbox(center: Point, lines: &[TextLine], w: f64, h: f64) -> Bbox {
    let text_bb = text_bbox(center, lines);
    if text_bb.is_empty() {
        return text_bb;
    }
    let min_x = if w > 0.0 {
        center.x - w / 2.0
    } else {
        text_bb.min.x
    };
    let max_x = if w > 0.0 {
        center.x + w / 2.0
    } else {
        text_bb.max.x
    };
    let y_bb = if h > 0.0 {
        text_object_vertical_bbox(center, lines, h)
    } else {
        text_bb
    };
    let min_y = y_bb.min.y;
    let max_y = y_bb.max.y;
    let mut bb = Bbox::new();
    bb.add(Point::new(min_x, min_y));
    bb.add(Point::new(max_x, max_y));
    // rotated lines can exceed the classic w/h grid — cover them too
    if lines.iter().any(|l| l.rotate.is_some()) {
        bb.union(&text_bb);
    }
    bb
}

fn text_object_vertical_bbox(center: Point, lines: &[TextLine], h: f64) -> Bbox {
    let mut bb = Bbox::new();
    if !has_visible_text(lines) {
        return bb;
    }
    let n = lines.len() as f64;
    let v = n - 1.0 + DP_TEXT_RATIO;
    let lineskip = if v.abs() > 1e-12 { h / v } else { 11.0 / 72.0 };
    let xheight = lineskip * DP_TEXT_RATIO;
    let mut baseline_y = center.y + (v * lineskip / 2.0) - xheight;
    for line in lines {
        if line.s.is_empty() {
            baseline_y -= lineskip;
            continue;
        }
        let just_offset = xheight / 2.0 + line.text_offset;
        let y = baseline_y + (line.valign as f64) * just_offset;
        bb.add(Point::new(center.x, y));
        bb.add(Point::new(center.x, y + xheight));
        baseline_y -= lineskip;
    }
    bb
}

/// Render a number the way pic's `%g`/string contexts do (no trailing zeros).
fn fmt_num(v: f64) -> String {
    format!("{v}")
}

fn install_dpic_compat_vars(vars: &mut HashMap<String, f64>) {
    // dpic backend option constants are zero-based in the order used by dpic's
    // own `case(dpicopt, ...)` examples. rpic renders SVG.
    let opts = [
        ("optMFpic", 0.0),
        ("optMpost", 1.0),
        ("optPDF", 2.0),
        ("optPGF", 3.0),
        ("optPict2e", 4.0),
        ("optPS", 5.0),
        ("optPSfrag", 6.0),
        ("optPSTricks", 7.0),
        ("optSVG", 8.0),
        ("optTeX", 9.0),
        ("opttTeX", 10.0),
        ("optxfig", 11.0),
    ];
    for (name, val) in opts {
        vars.insert(name.to_string(), val);
    }
    vars.insert("dpicopt".to_string(), 8.0);
}

/// Map a linear-style `.start`/`.end` anchor to the box/ellipse compass
/// corner it means for an object flowing in `dir` (entry vs exit edge), so
/// `with .start at …` / `with .end at …` edge-align closed objects the way
/// pikchr does — and consistently with the read path (`box_corner`, which
/// returns the stored `self.start`/`self.end`). Other corners pass through.
fn dir_start_end_corner(c: Corner, dir: Dir) -> Corner {
    match c {
        Corner::Start => match dir {
            Dir::Right => Corner::W,
            Dir::Left => Corner::E,
            Dir::Up => Corner::S,
            Dir::Down => Corner::N,
        },
        Corner::End => match dir {
            Dir::Right => Corner::E,
            Dir::Left => Corner::W,
            Dir::Up => Corner::N,
            Dir::Down => Corner::S,
        },
        other => other,
    }
}

/// Normalize a text-alignment angle (degrees) to (-90, 90] so an `aligned`
/// label never renders upside down on a leftward/downward segment.
fn readable_angle(mut deg: f64) -> f64 {
    while deg > 90.0 {
        deg -= 180.0;
    }
    while deg <= -90.0 {
        deg += 180.0;
    }
    deg
}

fn corner_offset(c: Corner, w: f64, h: f64) -> Point {
    let (hw, hh) = (w / 2.0, h / 2.0);
    match c {
        Corner::N => Point::new(0.0, hh),
        Corner::S => Point::new(0.0, -hh),
        Corner::E => Point::new(hw, 0.0),
        Corner::W => Point::new(-hw, 0.0),
        Corner::Ne => Point::new(hw, hh),
        Corner::Se => Point::new(hw, -hh),
        Corner::Nw => Point::new(-hw, hh),
        Corner::Sw => Point::new(-hw, -hh),
        Corner::Center | Corner::Start | Corner::End => Point::ZERO,
    }
}

fn closed_corner_offset(p: Prim, c: Corner, w: f64, h: f64, rad: f64) -> Point {
    match p {
        Prim::Circle | Prim::Ellipse => ellipse_corner_offset(c, w, h),
        Prim::Box => box_corner_offset(c, w, h, rad),
        _ => corner_offset(c, w, h),
    }
}

fn arc_angles(center: Point, start: Point, end: Point, cw: bool) -> (f64, f64) {
    let a0 = (start - center).y.atan2((start - center).x);
    let mut a1 = (end - center).y.atan2((end - center).x);
    if cw {
        while a1 > a0 {
            a1 -= 2.0 * PI;
        }
    } else {
        while a1 < a0 {
            a1 += 2.0 * PI;
        }
    }
    (a0, a1)
}

fn ellipse_corner_offset(c: Corner, w: f64, h: f64) -> Point {
    let (rx, ry) = (w / 2.0, h / 2.0);
    let diag = |sx: f64, sy: f64| Point::new(sx * rx * FRAC_1_SQRT_2, sy * ry * FRAC_1_SQRT_2);
    match c {
        Corner::N => Point::new(0.0, ry),
        Corner::S => Point::new(0.0, -ry),
        Corner::E => Point::new(rx, 0.0),
        Corner::W => Point::new(-rx, 0.0),
        Corner::Ne => diag(1.0, 1.0),
        Corner::Se => diag(1.0, -1.0),
        Corner::Nw => diag(-1.0, 1.0),
        Corner::Sw => diag(-1.0, -1.0),
        Corner::Center | Corner::Start | Corner::End => Point::ZERO,
    }
}

fn box_corner_offset(c: Corner, w: f64, h: f64, rad: f64) -> Point {
    if rad > 0.0 && matches!(c, Corner::Ne | Corner::Se | Corner::Nw | Corner::Sw) {
        let inset = rad.min(w.abs().min(h.abs()) / 2.0) * (1.0 - FRAC_1_SQRT_2);
        let x = w / 2.0 - inset;
        let y = h / 2.0 - inset;
        return match c {
            Corner::Ne => Point::new(x, y),
            Corner::Se => Point::new(x, -y),
            Corner::Nw => Point::new(-x, y),
            Corner::Sw => Point::new(-x, -y),
            _ => Point::ZERO,
        };
    }
    corner_offset(c, w, h)
}

/// The placed-object kind a `PrimObj` selects, or `None` for untyped `Any`
/// (matches every kind).
fn primobj_kind(o: &PrimObj) -> Option<PKind> {
    Some(match o {
        PrimObj::Prim(p) => match p {
            Prim::Box => PKind::Box,
            Prim::Circle => PKind::Circle,
            Prim::Ellipse => PKind::Ellipse,
            Prim::Line | Prim::Arrow => PKind::Line,
            Prim::Move => PKind::Move,
            Prim::Spline => PKind::Spline,
            Prim::Arc => PKind::Arc,
        },
        PrimObj::Brace => PKind::Brace,
        PrimObj::Block | PrimObj::EmptyBrack => PKind::Block,
        PrimObj::Str(_) => PKind::Text,
        PrimObj::Any => return None,
    })
}

fn want_name(k: PKind) -> &'static str {
    match k {
        PKind::Box => "box",
        PKind::Circle => "circle",
        PKind::Ellipse => "ellipse",
        PKind::Line => "line",
        PKind::Move => "move",
        PKind::Spline => "spline",
        PKind::Arc => "arc",
        PKind::Brace => "brace",
        PKind::Block => "block",
        PKind::Text => "text",
    }
}

fn nearest_dir(v: Point) -> Dir {
    if v.x.abs() >= v.y.abs() {
        if v.x >= 0.0 { Dir::Right } else { Dir::Left }
    } else if v.y >= 0.0 {
        Dir::Up
    } else {
        Dir::Down
    }
}

enum PrintfArg {
    Num(f64),
    Str(String),
}

impl PrintfArg {
    fn num(&self) -> f64 {
        match self {
            PrintfArg::Num(v) => *v,
            PrintfArg::Str(s) => s.parse::<f64>().unwrap_or(0.0),
        }
    }

    fn string(&self) -> String {
        match self {
            PrintfArg::Num(v) => fmt_num(*v),
            PrintfArg::Str(s) => s.clone(),
        }
    }
}

/// Minimal printf-style formatter supporting `%d %i %f %e %g %s %%` with
/// optional `.precision`. Width/flags are accepted but ignored.
fn sprintf_fmt(fmt: &str, vals: &[PrintfArg]) -> String {
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    let mut ai = 0usize;
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut spec = String::new();
        while let Some(&n) = chars.peek() {
            if "+-0123456789. #".contains(n) {
                spec.push(n);
                chars.next();
            } else {
                break;
            }
        }
        let conv = match chars.next() {
            Some(c) => c,
            None => break,
        };
        if conv == '%' {
            out.push('%');
            continue;
        }
        let arg = vals.get(ai);
        ai += 1;
        let prec = spec.split('.').nth(1).and_then(|p| {
            p.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<usize>()
                .ok()
        });
        match conv {
            'd' | 'i' => out.push_str(&format!(
                "{}",
                arg.map(PrintfArg::num).unwrap_or(0.0).round() as i64
            )),
            'f' | 'F' => out.push_str(&format!(
                "{:.*}",
                prec.unwrap_or(6),
                arg.map(PrintfArg::num).unwrap_or(0.0)
            )),
            'e' | 'E' => out.push_str(&format!(
                "{:.*e}",
                prec.unwrap_or(6),
                arg.map(PrintfArg::num).unwrap_or(0.0)
            )),
            'g' | 'G' => out.push_str(&format!("{}", arg.map(PrintfArg::num).unwrap_or(0.0))),
            's' => out.push_str(&arg.map(PrintfArg::string).unwrap_or_default()),
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}

fn stringexpr_lit(se: &StringExpr) -> String {
    match se {
        StringExpr::Lit(s) => s.clone(),
        StringExpr::Concat(a, b) => format!("{}{}", stringexpr_lit(a), stringexpr_lit(b)),
        StringExpr::Arg(n) => format!("${n}"),
        StringExpr::Sprintf(fmt, _) => stringexpr_lit(fmt),
        StringExpr::SvgFont(_) => String::new(),
        StringExpr::Rgb(_) | StringExpr::ColorNum(_) => String::new(),
    }
}

fn single_token_macro_string(toks: &[crate::lexer::Spanned]) -> Option<String> {
    let mut toks = toks
        .iter()
        .filter(|s| !matches!(s.tok, token::Token::Newline | token::Token::Eof));
    let tok = &toks.next()?.tok;
    if toks.next().is_some() {
        return None;
    }
    match tok {
        token::Token::Str(s) | token::Token::Name(s) | token::Token::Label(s) => Some(s.clone()),
        _ => None,
    }
}

fn unescape_exec_source(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some('"') = chars.clone().next()
        {
            chars.next();
            out.push('"');
        } else {
            out.push(c);
        }
    }
    out
}

/// Shift a [`Placed`] (and its block members) by `d` and re-index its shape
/// references by `shape_off`, mapping a block's local records into the parent.
fn rebase_placed(pl: &mut Placed, d: Point, shape_off: usize) {
    pl.center = pl.center + d;
    pl.start = pl.start + d;
    pl.end = pl.end + d;
    for p in &mut pl.points {
        *p = *p + d;
    }
    let mut bb = Bbox::new();
    bb.add(pl.bbox.min + d);
    bb.add(pl.bbox.max + d);
    pl.bbox = bb;
    if let Some(s) = pl.shape {
        pl.shape = Some(s + shape_off);
    }
    for m in pl.members.values_mut() {
        rebase_placed(m, d, shape_off);
    }
}

fn scale_placed(pl: &mut Placed, f: f64) {
    pl.center = pl.center * f;
    pl.start = pl.start * f;
    pl.end = pl.end * f;
    for p in &mut pl.points {
        *p = *p * f;
    }
    pl.radius *= f;
    pl.box_rad *= f;
    pl.line_wid *= f;
    pl.line_ht *= f;
    scale_bbox_in_place(&mut pl.bbox, f);
    for m in pl.members.values_mut() {
        scale_placed(m, f);
    }
}

fn translate_shape(sh: &mut Shape, d: Point) {
    let mv = |p: &mut Point| *p = *p + d;
    match sh {
        Shape::Box { c, .. } | Shape::Circle { c, .. } | Shape::Ellipse { c, .. } => mv(c),
        Shape::Path { pts, .. } | Shape::Spline { pts, .. } => {
            for p in pts {
                mv(p);
            }
        }
        Shape::Arc { c, .. } => mv(c),
        Shape::Brace {
            a,
            b,
            cubics,
            label_at,
            ..
        } => {
            mv(a);
            mv(b);
            mv(label_at);
            for cubic in cubics {
                for p in cubic {
                    mv(p);
                }
            }
        }
        Shape::Text { at, bbox, .. } => {
            mv(at);
            bbox.min = bbox.min + d;
            bbox.max = bbox.max + d;
        }
    }
}

fn scale_bbox_in_place(bb: &mut Bbox, f: f64) {
    if bb.is_empty() {
        return;
    }
    let mut b = Bbox::new();
    b.add(bb.min * f);
    b.add(bb.max * f);
    *bb = b;
}

fn scale_style(style: &mut Style, f: f64) {
    let f = f.abs();
    style.arrow_ht *= f;
    style.arrow_wid *= f;
    if let Some(hatch) = &mut style.hatch {
        hatch.sep *= f;
    }
    match &mut style.dash {
        Dash::Dashed(w) => *w *= f,
        Dash::Dotted(Some(w)) => *w *= f,
        Dash::Solid | Dash::Dotted(None) => {}
    }
}

fn multiply_shape_fill_opacity(sh: &mut Shape, opacity: f64) {
    match sh {
        Shape::Box { style, .. }
        | Shape::Circle { style, .. }
        | Shape::Ellipse { style, .. }
        | Shape::Path { style, .. }
        | Shape::Spline { style, .. }
        | Shape::Arc { style, .. }
        | Shape::Brace { style, .. } => {
            style.fill_opacity = Some(style.fill_opacity.unwrap_or(1.0) * opacity);
        }
        Shape::Text { .. } => {}
    }
}

/// Uniformly scale a shape's geometry about the origin (font size unchanged).
fn scale_shape(sh: &mut Shape, f: f64) {
    match sh {
        Shape::Box {
            c,
            w,
            h,
            rad,
            style,
            ..
        } => {
            *c = *c * f;
            *w *= f;
            *h *= f;
            *rad *= f;
            scale_style(style, f);
        }
        Shape::Circle { c, r, style, .. } => {
            *c = *c * f;
            *r *= f;
            scale_style(style, f);
        }
        Shape::Ellipse { c, w, h, style, .. } => {
            *c = *c * f;
            *w *= f;
            *h *= f;
            scale_style(style, f);
        }
        Shape::Path { pts, style, .. } | Shape::Spline { pts, style, .. } => {
            for p in pts {
                *p = *p * f;
            }
            scale_style(style, f);
        }
        Shape::Arc { c, r, style, .. } => {
            *c = *c * f;
            *r *= f;
            scale_style(style, f);
        }
        Shape::Brace {
            a,
            b,
            cubics,
            label_at,
            style,
            ..
        } => {
            *a = *a * f;
            *b = *b * f;
            *label_at = *label_at * f;
            for cubic in cubics {
                for p in cubic {
                    *p = *p * f;
                }
            }
            scale_style(style, f);
        }
        Shape::Text { at, bbox, w, h, .. } => {
            *at = *at * f;
            scale_bbox_in_place(bbox, f);
            *w *= f;
            *h *= f;
        }
    }
}

fn object_uses_bare_distance(kind: &ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::Primitive(Prim::Line | Prim::Arrow | Prim::Move | Prim::Spline)
            | ObjectKind::Brace
            | ObjectKind::Continue
    )
}

fn expr_bare_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Var(name, None) => Some(name.as_str()),
        _ => None,
    }
}

fn suggest_attribute(word: &str) -> Option<&'static str> {
    const WORDS: &[&str] = &[
        "above",
        "at",
        "below",
        "bracepos",
        "by",
        "ccw",
        "center",
        "chop",
        "class",
        "close",
        "color",
        "colored",
        "cw",
        "dashed",
        "diam",
        "dotrad",
        "dotted",
        "down",
        "fill",
        "fit",
        "from",
        "gradient",
        "hatch",
        "ht",
        "invis",
        "labeloffset",
        "left",
        "ljust",
        "opacity",
        "outlined",
        "rad",
        "right",
        "rjust",
        "same",
        "scaled",
        "shaded",
        "thick",
        "to",
        "up",
        "wid",
        "with",
    ];
    crate::diagnostic::closest(word, WORDS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse, parse_in_dir};

    fn draw(src: &str) -> Drawing {
        eval(&parse(src).unwrap()).unwrap()
    }

    fn scalar(src: &str) -> ER<f64> {
        let prog = parse(&format!("x = {src}")).unwrap();
        let mut st = State::new();
        st.eval_stmts(&prog.stmts)?;
        Ok(st.vars["x"])
    }

    fn assert_box_size(shape: &Shape, want_w: f64, want_h: f64) {
        let Shape::Box { w, h, .. } = shape else {
            panic!()
        };
        assert!((*w - want_w).abs() < 1e-9, "w = {w}, want {want_w}");
        assert!((*h - want_h).abs() < 1e-9, "h = {h}, want {want_h}");
    }

    const DEFAULT_STROKE_IN: f64 = 0.8 / 72.0;

    #[test]
    fn zero_iteration_for_body_is_never_parsed() {
        // #196: a dead loop body must not be parsed or macro-expanded —
        // the same deferred rule as dead `if` branches (dpic accepts both).
        let d = draw("for i = 1 to 0 do { bxo }\nbox");
        assert_eq!(d.shapes.len(), 1);

        // a recursive macro in a dead body must not hit the expansion guard
        let d = draw("define f { f() }\nfor i = 1 to 0 do { f() }\nbox");
        assert_eq!(d.shapes.len(), 1);

        // a backwards `by` range that yields no iterations counts too
        let d = draw("for i = 5 to 1 do { bxo }\nbox");
        assert_eq!(d.shapes.len(), 1);
    }

    #[test]
    fn executed_for_body_still_reports_errors_with_structure() {
        let e = eval(&parse("for i = 1 to 2 do { bxo }").unwrap()).unwrap_err();
        assert!(e.msg.contains("expected an object"), "{e}");
        let info = e.info.expect("structured info");
        assert_eq!(info.kind, "expected_token");
        assert_eq!(info.hint.as_deref(), Some("did you mean `box`?"));
    }

    #[test]
    fn pipeline_chains_left_to_right() {
        let d = draw(".PS\nellipse \"document\"\narrow\nbox \"PIC\"\narrow\nbox \"TROFF\"\n.PE");
        // 5 shapes
        assert_eq!(d.shapes.len(), 5);
        // first ellipse centered at (0.375, 0): ellipsewid/2
        if let Shape::Ellipse { c, .. } = &d.shapes[0] {
            assert!((c.x - 0.375).abs() < 1e-9, "ellipse x = {}", c.x);
            assert!(c.y.abs() < 1e-9);
        } else {
            panic!("expected ellipse");
        }
        // bbox grows to the right
        assert!(d.bbox.width() > 2.0);
        assert!((d.bbox.height() - (0.5 + DEFAULT_STROKE_IN)).abs() < 1e-9);
    }

    #[test]
    fn box_at_absolute() {
        let d = draw("box ht 0.3 wid 0.5 at 1,2");
        let Shape::Box { c, w, h, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(*c, Point::new(1.0, 2.0));
        assert!((*w - 0.5).abs() < 1e-9 && (*h - 0.3).abs() < 1e-9);
    }

    #[test]
    fn diamond_is_closed() {
        let d = draw("line up right then down right then down left then up left");
        let Shape::Path { pts, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(pts.len(), 5);
        // returns to start
        assert!(pts[0].dist(*pts.last().unwrap()) < 1e-9);
    }

    #[test]
    fn close_line_marks_polygon_and_uses_bbox_center_anchor() {
        let d = draw(
            "L: line right 1 then up 1 close\nbox wid .1 ht .1 at L.c\nbox wid .1 ht .1 at L.end",
        );
        let Shape::Path { pts, closed, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(*closed);
        assert_eq!(pts.len(), 4);
        assert_eq!(pts[0], Point::ZERO);
        assert_eq!(pts[1], Point::new(1.0, 0.0));
        assert_eq!(pts[2], Point::new(1.0, 1.0));
        assert_eq!(pts[3], Point::ZERO);

        let Shape::Box { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert_eq!(*c, Point::new(0.5, 0.5));

        let Shape::Box { c, .. } = &d.shapes[2] else {
            panic!()
        };
        assert_eq!(*c, Point::ZERO);
    }

    #[test]
    fn close_line_requires_three_vertices_and_ends_path() {
        let err = eval(&parse("line right close").unwrap()).unwrap_err();
        assert!(err.msg.contains("need at least 3 vertices"), "{err}");

        let err = eval(&parse("line right then up close then left").unwrap()).unwrap_err();
        assert!(err.msg.contains("polygon is closed"), "{err}");
    }

    #[test]
    fn corners_and_labels() {
        let d = draw("A: box wid 1 ht 1 at 0,0\nbox wid 0.5 ht 0.5 with .sw at A.ne");
        // second box sw corner at A.ne (0.5,0.5) => its center at (0.75,0.75)
        let Shape::Box { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!(
            (c.x - 0.75).abs() < 1e-9 && (c.y - 0.75).abs() < 1e-9,
            "c = {c:?}"
        );
    }

    #[test]
    fn circle_corner_anchors_are_on_the_circumference() {
        let d = draw("C: circle rad 1 at 0,0\nline from C.sw to C.ne");
        let Shape::Path { pts, .. } = &d.shapes[1] else {
            panic!()
        };
        let a = 1.0 / 2.0_f64.sqrt();
        assert_eq!(pts.len(), 2);
        assert!((pts[0].x + a).abs() < 1e-9 && (pts[0].y + a).abs() < 1e-9);
        assert!((pts[1].x - a).abs() < 1e-9 && (pts[1].y - a).abs() < 1e-9);
    }

    #[test]
    fn type_specific_corner_anchors_match_dpic() {
        let d = draw("L: line from (0,0) to (1,0) then to (1,1)\ncircle rad .01 at L.nw");
        let Shape::Circle { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!(c.dist(Point::new(0.0, 0.0)) < 1e-9, "line nw = {c:?}");

        let d = draw("A: arc cw rad 0.5 from (0,0) to (0.5,0.5)\ncircle rad .01 at A.s");
        let Shape::Circle { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!(c.dist(Point::new(0.5, -0.5)) < 1e-9, "arc s = {c:?}");

        let d = draw("B: box wid 1 ht 1 rad 0.3 at (0,0)\ncircle rad .01 at B.ne");
        let Shape::Circle { c, .. } = &d.shapes[1] else {
            panic!()
        };
        let inset = 0.3 * (1.0 - FRAC_1_SQRT_2);
        let expected = Point::new(0.5 - inset, 0.5 - inset);
        assert!(c.dist(expected) < 1e-9, "rounded box ne = {c:?}");
    }

    #[test]
    fn with_corner_uses_ellipse_geometry_for_placed_object() {
        let d = draw("ellipse; ellipse with .nw at last ellipse.se");
        let Shape::Ellipse { c: first, .. } = &d.shapes[0] else {
            panic!()
        };
        let Shape::Ellipse { c: second, .. } = &d.shapes[1] else {
            panic!()
        };
        let expected = *first + Point::new(0.75 * FRAC_1_SQRT_2, -0.5 * FRAC_1_SQRT_2);
        assert!(
            second.dist(expected) < 1e-9,
            "second center = {second:?}, expected = {expected:?}"
        );
    }

    #[test]
    fn for_loop_repeats() {
        let d = draw("for i = 1 to 3 do { box }");
        assert_eq!(d.shapes.len(), 3);
    }

    #[test]
    fn for_loop_can_assign_subscripted_counter() {
        let d = draw("i = 1\nfor A[i] = 1 to 3 do { i += 1 }\nbox wid A[1] + A[2] + A[3] ht 0.3");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 6.0).abs() < 1e-9, "w = {w}");
    }

    #[test]
    fn if_else_branches() {
        let d1 = draw("x = 1\nif x > 0 then { box } else { circle }");
        assert!(matches!(d1.shapes[0], Shape::Box { .. }));
        let d2 = draw("x = 0\nif x > 0 then { box } else { circle }");
        assert!(matches!(d2.shapes[0], Shape::Circle { .. }));
    }

    #[test]
    fn define_macro_with_args() {
        let d = draw("define elem { box wid $1 }\nelem(0.5)\nelem(1.25)");
        assert_eq!(d.shapes.len(), 2);
        let Shape::Box { w: w0, .. } = &d.shapes[0] else {
            panic!()
        };
        let Shape::Box { w: w1, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!((*w0 - 0.5).abs() < 1e-9 && (*w1 - 1.25).abs() < 1e-9);
    }

    #[test]
    fn define_accepts_arbitrary_delimiter() {
        let d = draw("define elem / box /\nelem");
        assert_eq!(d.shapes.len(), 1);
        assert!(matches!(d.shapes[0], Shape::Box { .. }));
    }

    #[test]
    fn labelled_call_to_multiline_body_macro() {
        // A multi-line `{ … }` body picks up newlines after `{` / before `}`.
        // They must not leak into a labelled call (`Q: m()` -> `Q: ⏎ <obj>`),
        // which would be a parse error. The block's terminals must still resolve.
        let d = draw(
            "define elem {\n  [\n    box wid 0.4 ht 0.2\n    L: last box.w\n    R: last box.e\n  ]\n}\nQ: elem() with .L at (1,1)\n\"x\" at Q.R",
        );
        // the block drew its box, and Q.R (a block sub-label) resolved for the text
        assert!(d.shapes.iter().any(|s| matches!(s, Shape::Box { .. })));
        let Shape::Text { at, .. } = d.shapes.last().unwrap() else {
            panic!()
        };
        // Q placed with .L (west) at (1,1); .R (east) is one box-width to the right
        assert!(
            (at.x - 1.4).abs() < 1e-6 && (at.y - 1.0).abs() < 1e-6,
            "at = {at:?}"
        );
    }

    #[test]
    fn copied_forward_macro_expands_in_deferred_multiline_call() {
        let dir = std::env::temp_dir().join(format!("rpic_forward_macro_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("lib.pic"),
            "define outer { if gate then { inner(0.2,\n  0.3) } }\ndefine inner { box wid $1 ht $2 }\n",
        )
        .unwrap();

        let pic = parse_in_dir(
            "if 1 then { copy \"lib.pic\" }\ngate = 1\nouter()",
            Some(dir.as_path()),
        )
        .unwrap();
        let d = eval(&pic).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        let Shape::Box { w, h, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 0.2).abs() < 1e-9 && (*h - 0.3).abs() < 1e-9);
    }

    #[test]
    fn deferred_body_uses_macro_frame_from_own_expansion() {
        let d = draw(
            "define draw_one { [\n  scalev = 2\n  define project { $1/scalev }\n  if use_it then { box wid project(4) ht 0.1 }\n] }\ndefine draw_two { define project { $1/future_scale } }\nuse_it = 1\ndraw_one()\ndraw_two()",
        );
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 2.0).abs() < 1e-9, "w = {w}");
    }

    #[test]
    fn subscripted_variables_store_by_index() {
        let d = draw("P[1] = 0.4\nP[2] = 0.9\nP[2] += 0.1\nbox wid P[2] ht P[1]");
        let Shape::Box { w, h, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 1.0).abs() < 1e-9 && (*h - 0.4).abs() < 1e-9);
    }

    #[test]
    fn multidimensional_variables_store_by_index_tuple() {
        let d = draw("M[1,2] = 0.7\nj = 2\nM[1,j] += 0.2\nbox wid M[1,2] ht 0.3");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 0.9).abs() < 1e-9, "w = {w}");
    }

    #[test]
    fn subscripted_label_places_resolve_by_index() {
        let d =
            draw("for i = 1 to 2 do { A[i]: circle rad 0.01 at (i,0) }\nline from A[1] to A[2]");
        let Shape::Path { pts, .. } = &d.shapes[2] else {
            panic!()
        };
        assert!(
            pts[0].dist(Point::new(1.0, 0.0)) < 1e-9,
            "start {:?}",
            pts[0]
        );
        assert!(pts[1].dist(Point::new(2.0, 0.0)) < 1e-9, "end {:?}", pts[1]);
    }

    #[test]
    fn for_loop_places_in_a_row() {
        // boxes step right by default; bbox should be ~3*boxwid wide
        let d = draw("for i = 1 to 3 do { box; move }");
        assert!(d.bbox.width() > 2.0);
    }

    #[test]
    fn animate_timing() {
        let d = draw(
            "A: box\narrow\nbox\nanimate A with \"fade\" for 0.5\nanimate last arrow with \"draw\"\nanimate 2nd box with \"pop\" after A",
        );
        assert_eq!(d.anims.len(), 3);
        // A: fade, start 0, dur 0.5, targets shape 0
        assert_eq!(d.anims[0].effect, "fade");
        assert_eq!(d.anims[0].shape, 0);
        assert!((d.anims[0].start).abs() < 1e-9 && (d.anims[0].duration - 0.5).abs() < 1e-9);
        // arrow: draw, sequential after A -> start 0.5, default dur 0.6, shape 1
        assert_eq!(d.anims[1].shape, 1);
        assert!((d.anims[1].start - 0.5).abs() < 1e-9);
        // 2nd box: pop, after A (ends at 0.5) -> start 0.5, shape 2
        assert_eq!(d.anims[2].shape, 2);
        assert!((d.anims[2].start - 0.5).abs() < 1e-9);
    }

    #[test]
    fn animate_repeat_yoyo_ease() {
        let d =
            draw("box\nanimate last box with \"pop\" repeat 3 yoyo ease \"elastic.out(1, 0.3)\"");
        assert_eq!(d.anims.len(), 1);
        let a = &d.anims[0];
        assert_eq!(a.repeat, 3);
        assert!(a.yoyo);
        assert_eq!(a.ease.as_deref(), Some("elastic.out(1, 0.3)"));
        // No warning: yoyo is paired with a repeat.
        assert!(!d.warnings.iter().any(|w| w.kind == "yoyo_without_repeat"));
    }

    #[test]
    fn animate_infinite_repeat_does_not_stall_sequence() {
        // An infinite loop must not push the next animation's start to infinity:
        // sequential timing tracks only the first iteration's end.
        let d = draw(
            "box\nbox\nanimate 1st box with \"fade\" for 0.4 repeat -1\nanimate 2nd box with \"pop\"",
        );
        assert_eq!(d.anims[0].repeat, -1);
        assert!((d.anims[1].start - 0.4).abs() < 1e-9);
    }

    #[test]
    fn animate_yoyo_without_repeat_warns() {
        let d = draw("box\nanimate last box with \"fade\" yoyo");
        assert_eq!(d.anims[0].repeat, 0);
        assert!(d.anims[0].yoyo); // flag is still recorded, just inert in GSAP
        assert!(d.warnings.iter().any(|w| w.kind == "yoyo_without_repeat"));
    }

    #[test]
    fn animate_defaults_leave_repeat_fields_inert() {
        let d = draw("box\nanimate last box with \"fade\"");
        let a = &d.anims[0];
        assert_eq!(a.repeat, 0);
        assert!(!a.yoyo);
        assert_eq!(a.ease, None);
        assert_eq!(a.path, None);
    }

    #[test]
    fn animate_move_records_the_path_shape() {
        // The dot (shape 1) travels along the line (shape 0).
        let d = draw("L: line right 3\nD: dot at L.start\nanimate D with \"move\" along L for 2");
        assert_eq!(d.anims.len(), 1);
        let a = &d.anims[0];
        assert_eq!(a.effect, "move");
        assert_eq!(a.shape, 1);
        assert_eq!(a.path, Some(0));
        assert!((a.duration - 2.0).abs() < 1e-9);
        // `move` is a known effect: no unknown-effect warning.
        assert!(
            !d.warnings
                .iter()
                .any(|w| w.kind == "unknown_animation_effect")
        );
    }

    #[test]
    fn animate_move_without_path_errors() {
        let err = eval(&parse("box\nanimate last box with \"move\"").unwrap()).unwrap_err();
        assert!(err.msg.contains("`move` needs a path"));
    }

    #[test]
    fn animate_along_without_move_warns_and_is_dropped() {
        let d = draw("L: line right 2\nbox\nanimate last box with \"fade\" along L");
        assert!(d.warnings.iter().any(|w| w.kind == "along_without_move"));
        // `along` is ignored for non-move effects — no path leaks into the manifest.
        assert_eq!(d.anims[0].path, None);
    }

    #[test]
    fn animate_highlight_resolves_colour_forms() {
        // Named colour passes through; rgb()/0xRRGGBB resolve to hex.
        let d = draw(
            "box\nbox\nbox\nanimate 1st box with \"highlight\" to \"crimson\"\nanimate 2nd box with \"highlight\" to rgb(255,140,0)\nanimate 3rd box with \"highlight\" to 0x1b5e20",
        );
        assert_eq!(d.anims[0].color.as_deref(), Some("crimson"));
        assert_eq!(d.anims[1].color.as_deref(), Some("#ff8c00"));
        assert_eq!(d.anims[2].color.as_deref(), Some("#1b5e20"));
        assert!(
            !d.warnings
                .iter()
                .any(|w| w.kind == "unknown_animation_effect")
        );
    }

    #[test]
    fn animate_highlight_without_colour_is_allowed() {
        let d = draw("box\nanimate last box with \"highlight\" repeat 1 yoyo");
        assert_eq!(d.anims[0].effect, "highlight");
        assert_eq!(d.anims[0].color, None);
    }

    #[test]
    fn animate_to_without_highlight_warns_and_is_dropped() {
        let d = draw("box\nanimate last box with \"fade\" to \"red\"");
        assert!(d.warnings.iter().any(|w| w.kind == "to_without_highlight"));
        // `to` is ignored for non-highlight effects — no colour in the manifest.
        assert_eq!(d.anims[0].color, None);
    }

    #[test]
    fn animate_stagger_fans_across_block_children() {
        let d = draw(
            "B: [ box \"a\"; box \"b\"; box \"c\" ]\nanimate B with \"fade\" for 0.3 stagger 0.15",
        );
        assert_eq!(d.anims.len(), 3);
        assert_eq!(d.anims[0].shape, 0);
        assert_eq!(d.anims[1].shape, 1);
        assert_eq!(d.anims[2].shape, 2);
        assert!((d.anims[0].start - 0.0).abs() < 1e-9);
        assert!((d.anims[1].start - 0.15).abs() < 1e-9);
        assert!((d.anims[2].start - 0.3).abs() < 1e-9);
        assert!(d.anims.iter().all(|a| a.effect == "fade"));
    }

    #[test]
    fn animate_stagger_skips_invisible_spines() {
        // The explicit `move`s between boxes are invisible: only the 3 boxes
        // (s0, s2, s4) get stagger slots — s1/s3 are skipped.
        let d = draw("B: [ box; move; box; move; box ]\nanimate B with \"pop\" stagger 0.1");
        assert_eq!(d.anims.len(), 3);
        assert_eq!(
            d.anims.iter().map(|a| a.shape).collect::<Vec<_>>(),
            vec![0, 2, 4]
        );
    }

    #[test]
    fn animate_stagger_advances_the_sequence_past_the_last_child() {
        let d = draw(
            "B: [ box; box ]\ncircle\nanimate B with \"pop\" for 0.2 stagger 0.1\nanimate last circle with \"fade\"",
        );
        // children at 0.0 and 0.1 (dur 0.2) → last ends at 0.1+0.2 = 0.3.
        assert!((d.anims[2].start - 0.3).abs() < 1e-9);
        assert_eq!(d.anims[2].effect, "fade");
    }

    #[test]
    fn animate_stagger_without_block_warns_and_animates_single() {
        let d = draw("box\nanimate last box with \"fade\" stagger 0.1");
        assert!(d.warnings.iter().any(|w| w.kind == "stagger_without_block"));
        assert_eq!(d.anims.len(), 1);
        assert_eq!(d.anims[0].shape, 0);
    }

    #[test]
    fn animate_morph_records_the_target_shape() {
        // Box A (shape 0) morphs into circle B (shape 1).
        let d = draw("A: box\nB: circle at A+(2,0)\nanimate A with \"morph\" into B for 1");
        assert_eq!(d.anims.len(), 1);
        assert_eq!(d.anims[0].effect, "morph");
        assert_eq!(d.anims[0].shape, 0);
        assert_eq!(d.anims[0].morph, Some(1));
        assert!(
            !d.warnings
                .iter()
                .any(|w| w.kind == "unknown_animation_effect")
        );
    }

    #[test]
    fn animate_morph_without_target_errors() {
        let err = eval(&parse("box\nanimate last box with \"morph\"").unwrap()).unwrap_err();
        assert!(err.msg.contains("`morph` needs a target"));
    }

    #[test]
    fn animate_into_without_morph_warns_and_is_dropped() {
        let d = draw("A: box\nB: circle at A+(2,0)\nanimate A with \"fade\" into B");
        assert!(d.warnings.iter().any(|w| w.kind == "into_without_morph"));
        assert_eq!(d.anims[0].morph, None);
    }

    #[test]
    fn animate_scroll_sets_the_timeline_hint() {
        let d = draw("box\nanimate last box with \"fade\"\nanimate scroll");
        assert!(d.anim_scroll);
        // it is a directive, not an object animation
        assert_eq!(d.anims.len(), 1);
    }

    #[test]
    fn animate_scroll_defaults_off() {
        let d = draw("box\nanimate last box with \"fade\"");
        assert!(!d.anim_scroll);
    }

    #[test]
    fn animate_slide_records_direction() {
        let d = draw("box\nanimate last box with \"slide\" from left for 0.4");
        assert_eq!(d.anims[0].effect, "slide");
        assert_eq!(d.anims[0].from.as_deref(), Some("left"));
        assert!(!d.anims[0].out);
    }

    #[test]
    fn animate_slide_without_direction_errors() {
        let err = eval(&parse("box\nanimate last box with \"slide\"").unwrap()).unwrap_err();
        assert!(err.msg.contains("`slide` needs a direction"));
    }

    #[test]
    fn animate_from_without_slide_warns_and_is_dropped() {
        let d = draw("box\nanimate last box with \"fade\" from up");
        assert!(d.warnings.iter().any(|w| w.kind == "from_without_slide"));
        assert_eq!(d.anims[0].from, None);
    }

    #[test]
    fn animate_out_is_a_modifier_on_any_effect() {
        let d = draw("box\nbox\nanimate 1st box with \"fade\" out\nanimate 2nd box with \"pop\"");
        assert!(d.anims[0].out);
        assert!(!d.anims[1].out); // default is an entrance
    }

    #[test]
    fn animate_slide_and_out_compose() {
        let d = draw("box\nanimate last box with \"slide\" from down out");
        assert_eq!(d.anims[0].from.as_deref(), Some("down"));
        assert!(d.anims[0].out);
    }

    #[test]
    fn behind_sets_render_layer_without_changing_shape_indices() {
        let d = draw("A: box\nB: box behind A\nanimate B with \"fade\"");
        assert_eq!(d.shapes.len(), 2);
        assert_eq!(d.shape_layers, vec![0, -1]);
        assert_eq!(d.anims.len(), 1);
        assert_eq!(d.anims[0].shape, 1);
    }

    #[test]
    fn behind_keeps_last_and_ordinals_semantic() {
        let d =
            draw("A: box at (0,0)\nB: box behind A at (2,0)\nline from last box.c to 1st box.c");
        let Shape::Path { pts, .. } = &d.shapes[2] else {
            panic!()
        };
        assert_eq!(pts.len(), 2);
        assert!(
            pts[0].dist(Point::new(2.0, 0.0)) < 1e-9,
            "start = {:?}",
            pts[0]
        );
        assert!(
            pts[1].dist(Point::new(0.0, 0.0)) < 1e-9,
            "end = {:?}",
            pts[1]
        );
    }

    #[test]
    fn continue_extends_previous_line() {
        // issue #7: `continue` appends a segment to the last line (no new shape)
        let d = draw("line right 1\ncontinue down 0.5");
        assert_eq!(d.shapes.len(), 1, "should extend, not add a shape");
        let Shape::Path { pts, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(pts.len(), 3);
        assert!(pts[2].dist(Point::new(1.0, -0.5)) < 1e-9, "{:?}", pts[2]);
        // bare continue extends in the current direction by linewid
        let d2 = draw("line right 1\ncontinue");
        let Shape::Path { pts, .. } = &d2.shapes[0] else {
            panic!()
        };
        assert!((pts.last().unwrap().x - 1.5).abs() < 1e-9);
    }

    #[test]
    fn continue_rejects_closed_path() {
        let err = eval(&parse("line right then up then left close\ncontinue right").unwrap())
            .unwrap_err();
        assert!(err.msg.contains("polygon is closed"), "{err}");
    }

    #[test]
    fn arc_from_to_endpoints_and_radius() {
        // issue #6: `arc from A to B` passes through both endpoints
        let d = draw("A:(0,0)\nB:(1,1)\narc from A to B");
        let Shape::Arc { c, r, a0, a1, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*r - 2.0_f64.sqrt() / 2.0).abs() < 1e-9, "r = {r}");
        let s = *c + Point::new(a0.cos(), a0.sin()) * *r;
        let e = *c + Point::new(a1.cos(), a1.sin()) * *r;
        assert!(s.dist(Point::new(0.0, 0.0)) < 1e-9, "start {s:?}");
        assert!(e.dist(Point::new(1.0, 1.0)) < 1e-9, "end {e:?}");

        let d_short = draw("arc from (0,0) to (0.3,0)");
        let Shape::Arc { r, .. } = &d_short.shapes[0] else {
            panic!()
        };
        assert!((*r - 0.25).abs() < 1e-9, "r = {r}");

        let d_custom = draw("arcrad = 0.5\narc from (0,0) to (0.3,0)");
        let Shape::Arc { r, .. } = &d_custom.shapes[0] else {
            panic!()
        };
        assert!((*r - 0.5).abs() < 1e-9, "r = {r}");

        // explicit radius is honored
        let d2 = draw("A:(0,0)\nB:(1,0)\narc from A to B rad 2");
        let Shape::Arc { r, .. } = &d2.shapes[0] else {
            panic!()
        };
        assert!((*r - 2.0).abs() < 1e-9, "r = {r}");
    }

    #[test]
    fn arc_direction_disambiguates_from_to_radius() {
        let d = draw(
            "arc left from (0.5,0) to (0,0.5) rad 0.5\n\
             arc right from (0.5,0) to (0,0.5) rad 0.5 dashed",
        );
        let Shape::Arc {
            c: left,
            a0: left_a0,
            a1: left_a1,
            ..
        } = &d.shapes[0]
        else {
            panic!()
        };
        let Shape::Arc {
            c: right,
            a0: right_a0,
            a1: right_a1,
            ..
        } = &d.shapes[1]
        else {
            panic!()
        };

        assert!(left.dist(Point::new(0.0, 0.0)) < 1e-9, "left = {left:?}");
        assert!(right.dist(Point::new(0.5, 0.5)) < 1e-9, "right = {right:?}");
        assert!((left_a1 - left_a0).abs() < PI);
        assert!((right_a1 - right_a0).abs() > PI);
    }

    #[test]
    fn arc_with_center_at_disambiguates_large_clockwise_sweep() {
        let d = draw("arc cw rad 1 from (0,-1) to (1,0) with .c at (0,0)");
        let Shape::Arc { c, r, a0, a1, .. } = &d.shapes[0] else {
            panic!()
        };
        let start = *c + Point::new(a0.cos(), a0.sin()) * *r;
        let end = *c + Point::new(a1.cos(), a1.sin()) * *r;

        assert!(c.dist(Point::ZERO) < 1e-9, "center = {c:?}");
        assert!((*r - 1.0).abs() < 1e-9, "r = {r}");
        assert!(
            start.dist(Point::new(0.0, -1.0)) < 1e-9,
            "start = {start:?}"
        );
        assert!(end.dist(Point::new(1.0, 0.0)) < 1e-9, "end = {end:?}");
        assert!(*a1 - *a0 < -PI, "sweep = {}", *a1 - *a0);
    }

    #[test]
    fn arc_width_height_attrs_size_arrowheads() {
        let d = draw("arc <-> wid .5 ht .75");
        let Shape::Arc { style, .. } = &d.shapes[0] else {
            panic!()
        };

        assert!(
            (style.arrow_wid - 0.5).abs() < 1e-9,
            "wid = {}",
            style.arrow_wid
        );
        assert!(
            (style.arrow_ht - 0.75).abs() < 1e-9,
            "ht = {}",
            style.arrow_ht
        );
    }

    #[test]
    fn scale_converts_user_units_to_inches() {
        // `scale = 2` means two user units per inch: defaults stay the same
        // physical size, while explicit dimensions and coordinates are halved.
        let d = draw("scale = 2\nbox");
        let Shape::Box { w, h, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(
            (*w - 0.75).abs() < 1e-9 && (*h - 0.5).abs() < 1e-9,
            "{w} x {h}"
        );

        let d = draw("scale = 2\nbox wid 2 ht 1");
        let Shape::Box { w, h, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 1.0).abs() < 1e-9 && (*h - 0.5).abs() < 1e-9);

        let d = draw("scale = 2\nline from (0,0) to (2,0)");
        let Shape::Path { pts, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((pts.last().unwrap().x - 1.0).abs() < 1e-9, "{pts:?}");

        let d = draw("scale = 2\nA: (2,0)\nbox wid A.x ht .2");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 1.0).abs() < 1e-9, "w = {w}");

        let d = draw("scale = 2\nbox wid 2 ht 1\nscale = 1\nbox wid 1 ht .5");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 2.0).abs() < 1e-9, "w = {w}");
        let Shape::Box { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!((c.x - 2.5).abs() < 1e-9, "center = {c:?}");
    }

    #[test]
    fn same_reuses_previous_dims() {
        // issue #4: `box same` reuses the previous box's dimensions
        let d = draw("box wid 1 ht 0.4 at 0,0\nbox same at 2,0");
        let Shape::Box { w, h, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!(
            (*w - 1.0).abs() < 1e-9 && (*h - 0.4).abs() < 1e-9,
            "{w} x {h}"
        );
    }

    #[test]
    fn same_reuses_previous_open_vector() {
        let d = draw("line up 1\nright\nline same");
        let Shape::Path { pts, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!(pts[0].dist(Point::new(0.0, 1.0)) < 1e-9, "{pts:?}");
        assert!(pts[1].dist(Point::new(0.0, 2.0)) < 1e-9, "{pts:?}");
    }

    #[test]
    fn spline_expr_is_tension_not_distance() {
        // issue #63: `spline <expr>` is a dpic tension parameter, not a bare
        // distance. The control polygon (and thus start/end) must be unchanged,
        // and the tension recorded.
        let d = draw("spline 0.5 from 0,0 to 1,1 to 2,0");
        let Shape::Spline { pts, tension, .. } = &d.shapes[0] else {
            panic!("expected a spline")
        };
        assert_eq!(*tension, Some(0.5));
        assert_eq!(pts.len(), 3, "tension must not add a segment: {pts:?}");
        assert!(pts[0].dist(Point::new(0.0, 0.0)) < 1e-9, "{pts:?}");
        assert!(pts[2].dist(Point::new(2.0, 0.0)) < 1e-9, "{pts:?}");
    }

    #[test]
    fn spline_variable_tension_does_not_drift() {
        // The doc/spline.pic idiom: `for x … { spline x from 0,0 … }`. Each
        // tensioned spline must keep the same start/end as the untensioned one
        // (only the curvature changes), instead of `x` shifting the geometry.
        let plain = draw("spline from 0,0 up 1.5 then right 2 then down 1.5");
        let Shape::Spline {
            pts: p0,
            tension: t0,
            ..
        } = &plain.shapes[0]
        else {
            panic!()
        };
        assert_eq!(*t0, None);

        let tensioned = draw("x = 0.6\nspline x from 0,0 up 1.5 then right 2 then down 1.5");
        let Shape::Spline {
            pts: p1,
            tension: t1,
            ..
        } = &tensioned.shapes[0]
        else {
            panic!()
        };
        assert_eq!(*t1, Some(0.6));
        assert_eq!(p0.len(), p1.len());
        assert!(p0.first().unwrap().dist(*p1.first().unwrap()) < 1e-9);
        assert!(p0.last().unwrap().dist(*p1.last().unwrap()) < 1e-9);
    }

    #[test]
    fn chop_trims_line_endpoints() {
        // issue #4: `chop` trims circlerad (0.25) off each end
        let d = draw("circle at 0,0\ncircle at 2,0\nline from 1st circle to 2nd circle chop");
        let Shape::Path { pts, .. } = &d.shapes[2] else {
            panic!()
        };
        assert!((pts[0].x - 0.25).abs() < 1e-9, "start {:?}", pts[0]);
        assert!(
            (pts.last().unwrap().x - 1.75).abs() < 1e-9,
            "end {:?}",
            pts.last()
        );

        let d = draw("line from (0,0) to (2,0) chop 0 chop .5");
        let Shape::Path { pts, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((pts[0].x - 0.0).abs() < 1e-9, "{pts:?}");
        assert!((pts[1].x - 1.5).abs() < 1e-9, "{pts:?}");

        let d = draw("line from (0,0) to (2,0) chop .5 chop 0");
        let Shape::Path { pts, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((pts[0].x - 0.5).abs() < 1e-9, "{pts:?}");
        assert!((pts[1].x - 2.0).abs() < 1e-9, "{pts:?}");

        let d = draw("line from (0,0) to (2,0) chop -.25 chop -.5");
        let Shape::Path { pts, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((pts[0].x + 0.25).abs() < 1e-9, "{pts:?}");
        assert!((pts[1].x - 2.5).abs() < 1e-9, "{pts:?}");
    }

    #[test]
    fn chop_on_zero_length_line_is_ignored_like_dpic() {
        let plain = draw("line from (0,0) to (0,0)");
        let chopped = draw("line from (0,0) to (0,0) chop -0.1");
        assert_eq!(crate::to_svg(&chopped), crate::to_svg(&plain));

        let Shape::Path { pts, .. } = &chopped.shapes[0] else {
            panic!()
        };
        assert_eq!(pts, &[Point::ZERO, Point::ZERO]);
        assert!(pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()));

        let extended = draw("line from (0,0) to (1,0) chop -0.1");
        let Shape::Path { pts, .. } = &extended.shapes[0] else {
            panic!()
        };
        assert!((pts[0].x + 0.1).abs() < 1e-9, "{pts:?}");
        assert!((pts[1].x - 1.1).abs() < 1e-9, "{pts:?}");
    }

    #[test]
    fn unknown_variables_are_errors() {
        assert!(eval(&parse("box wid typo ht 0.2").unwrap()).is_err());
        assert!(eval(&parse("typo += 1").unwrap()).is_err());
    }

    #[test]
    fn rand_advances_and_seed_repeats() {
        let mut st = State::new();
        let a = st.eval_expr(&Expr::Rand(None)).unwrap();
        let b = st.eval_expr(&Expr::Rand(None)).unwrap();
        assert!((0.0..1.0).contains(&a));
        assert!((0.0..1.0).contains(&b));
        assert_ne!(a, b);

        let seeded_a = st
            .eval_expr(&Expr::Rand(Some(Box::new(Expr::Num(1.0)))))
            .unwrap();
        let seeded_b = st
            .eval_expr(&Expr::Rand(Some(Box::new(Expr::Num(1.0)))))
            .unwrap();
        assert_eq!(seeded_a, seeded_b);
        assert!((seeded_a - 0.840_187_717).abs() < 1e-9, "{seeded_a}");
        let next = st.eval_expr(&Expr::Rand(None)).unwrap();
        assert!((next - 0.394_382_927).abs() < 1e-9, "{next}");
    }

    #[test]
    fn arithmetic_matches_dpic_edge_cases() {
        assert_eq!(scalar("5.5 % 2").unwrap(), 0.0);
        assert_eq!(scalar("2.5 % 2").unwrap(), 1.0);
        assert_eq!(scalar("-2.5 % 2").unwrap(), -1.0);
        assert!(scalar("5 % 0.4").is_err());

        let mut st = State::new();
        st.eval_stmts(&parse("x = 5.5; x %= 2").unwrap().stmts)
            .unwrap();
        assert_eq!(st.vars["x"], 0.0);

        assert_eq!(scalar("sign(0)").unwrap(), 1.0);
        assert_eq!(scalar("sign(-0.1)").unwrap(), -1.0);

        assert_eq!(scalar("(-2)^3").unwrap(), -8.0);
        assert_eq!(scalar("(-2)^2").unwrap(), 4.0);
        assert_eq!(scalar("0^0").unwrap(), 1.0);
        assert!(scalar("(-2)^0.5").is_err());
        assert!(scalar("0^-1").is_err());
    }

    #[test]
    fn standalone_text_occupies_invisible_box() {
        let d = draw("textwid = 1; textht = .2\n\"x\"\nbox wid .2 ht .2");
        let Shape::Text { at, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((at.x - 0.5).abs() < 1e-9, "text at {at:?}");
        let Shape::Box { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!((c.x - 1.1).abs() < 1e-9, "box center {c:?}");
    }

    #[test]
    fn text_position_modifies_the_preceding_string_only() {
        let d = draw("\"LLLL\" ljust");
        let Shape::Text { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].halign, -1);

        let d = draw("\"RRRR\" rjust");
        let Shape::Text { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].halign, 1);

        let d = draw("box wid 1 ht .6 \"AAAA\" above \"BBBB\" below");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].valign, 1);
        assert_eq!(text[1].valign, -1);

        let d = draw("box \"AAAA\" above \"BBBB\"");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].valign, 1);
        assert_eq!(text[1].valign, 0);
    }

    #[test]
    fn fit_sizes_closed_objects_to_preceding_text() {
        let d = draw(
            "box \"wide label\" fit\n\
             ellipse \"one\" \"two\" \"three\" fit\n\
             circle \"wide label\" fit",
        );

        let Shape::Box { w, h, text, .. } = &d.shapes[0] else {
            panic!()
        };
        let (want_w, want_h) = fitted_text_size(text).unwrap();
        assert!((*w - want_w).abs() < 1e-9, "box w = {w}, want {want_w}");
        assert!((*h - want_h).abs() < 1e-9, "box h = {h}, want {want_h}");

        let Shape::Ellipse { w, h, text, .. } = &d.shapes[1] else {
            panic!()
        };
        let (want_w, want_h) = fitted_text_size(text).unwrap();
        assert!((*w - want_w).abs() < 1e-9, "ellipse w = {w}, want {want_w}");
        assert!((*h - want_h).abs() < 1e-9, "ellipse h = {h}, want {want_h}");

        let Shape::Circle { r, text, .. } = &d.shapes[2] else {
            panic!()
        };
        let (want_w, want_h) = fitted_text_size(text).unwrap();
        let want_r = want_w.hypot(want_h) / 2.0;
        assert!((*r - want_r).abs() < 1e-9, "circle r = {r}, want {want_r}");
    }

    #[test]
    fn fit_respects_explicit_dimensions_and_text_order() {
        let d = draw("box wid 1 ht .2 \"very long label\" fit");
        assert_box_size(&d.shapes[0], 1.0, 0.2);

        let before = draw("box \"short\" fit");
        let after = draw("box \"short\" fit \"this later text does not affect fit\"");
        let Shape::Box {
            w: before_w,
            h: before_h,
            ..
        } = &before.shapes[0]
        else {
            panic!()
        };
        let Shape::Box {
            w: after_w,
            h: after_h,
            ..
        } = &after.shapes[0]
        else {
            panic!()
        };
        assert!((*before_w - *after_w).abs() < 1e-9);
        assert!((*before_h - *after_h).abs() < 1e-9);
    }

    #[test]
    fn fit_without_preceding_visible_text_errors() {
        let err = eval(&parse("box fit").unwrap()).unwrap_err();
        assert!(err.msg.contains("visible text"), "{}", err.msg);

        let err = eval(&parse("box \"\" fit").unwrap()).unwrap_err();
        assert!(err.msg.contains("visible text"), "{}", err.msg);
    }

    #[test]
    fn brace_draws_curly_annotation_between_points() {
        let d = draw("brace from (0,0) to (2,0) down \"n\" wid .25 bracepos .25");
        let Shape::Brace {
            a,
            b,
            cubics,
            label_at,
            text,
            ..
        } = &d.shapes[0]
        else {
            panic!()
        };
        assert!(a.dist(Point::new(0.0, 0.0)) < 1e-9, "a = {a:?}");
        assert!(b.dist(Point::new(2.0, 0.0)) < 1e-9, "b = {b:?}");
        assert_eq!(text[0].s, "n");
        assert!(label_at.y < -0.25, "label_at = {label_at:?}");
        assert!(
            (cubics[2][3].x - 0.5).abs() < 1e-9,
            "cusp = {:?}",
            cubics[2][3]
        );
        assert!(cubics[2][3].y < -0.2, "cusp = {:?}", cubics[2][3]);
        assert!(d.bbox.min.y < -0.25, "bbox = {:?}", d.bbox);
    }

    #[test]
    fn brace_side_words_choose_absolute_side() {
        let up = draw("brace from (0,0) to (2,0) up wid .2");
        let down = draw("brace from (0,0) to (2,0) down wid .2");
        let Shape::Brace {
            label_at: up_label, ..
        } = &up.shapes[0]
        else {
            panic!()
        };
        let Shape::Brace {
            label_at: down_label,
            ..
        } = &down.shapes[0]
        else {
            panic!()
        };
        assert!(up_label.y > 0.0, "up label = {up_label:?}");
        assert!(down_label.y < 0.0, "down label = {down_label:?}");
    }

    #[test]
    fn brace_labeloffset_moves_label_outward_from_cusp() {
        let base = draw("brace from (0,0) to (2,0) up \"n\" wid .25");
        let far = draw("brace from (0,0) to (2,0) up \"n\" wid .25 labeloffset .2");
        let Shape::Brace {
            label_at: base_label,
            ..
        } = &base.shapes[0]
        else {
            panic!()
        };
        let Shape::Brace {
            label_at: far_label,
            ..
        } = &far.shapes[0]
        else {
            panic!()
        };
        assert!(
            (far_label.y - base_label.y - 0.2).abs() < 1e-9,
            "base = {base_label:?}, far = {far_label:?}"
        );
    }

    #[test]
    fn brace_compass_anchors_use_curve_bbox() {
        let d = draw(
            "B: brace from (0,0) to (2,0) up wid .25 bracepos .25\n\
             circle rad .01 at B.nw\n\
             circle rad .01 at B.ne\n\
             circle rad .01 at B.n\n\
             circle rad .01 at B.c",
        );
        let circles: Vec<Point> = d
            .shapes
            .iter()
            .skip(1)
            .map(|shape| {
                let Shape::Circle { c, .. } = shape else {
                    panic!()
                };
                *c
            })
            .collect();

        assert!(circles[0].x.abs() < 1e-9, "nw = {:?}", circles[0]);
        assert!((circles[1].x - 2.0).abs() < 1e-9, "ne = {:?}", circles[1]);
        assert!(
            (circles[0].y - circles[1].y).abs() < 1e-9,
            "nw = {:?}, ne = {:?}",
            circles[0],
            circles[1]
        );
        assert!(
            (circles[2].x - 1.0).abs() < 1e-9 && circles[2].y > 0.2,
            "n = {:?}",
            circles[2]
        );
        assert!(
            (circles[3].x - 0.5).abs() < 1e-9 && circles[3].y > 0.2,
            "c = {:?}",
            circles[3]
        );
    }

    #[test]
    fn brace_has_open_object_anchors_and_length() {
        let d = draw(
            "B: brace from (0,0) to (2,0) down wid .25\n\
             box wid (B.len) ht .1 at B.c\n\
             line from B.start to B.end",
        );
        assert_box_size(&d.shapes[1], 2.0, 0.1);
        let Shape::Path { pts, .. } = &d.shapes[2] else {
            panic!()
        };
        assert!(pts[0].dist(Point::new(0.0, 0.0)) < 1e-9);
        assert!(pts[1].dist(Point::new(2.0, 0.0)) < 1e-9);
    }

    #[test]
    fn bracepos_must_be_inside_segment() {
        let err = eval(&parse("brace from (0,0) to (1,0) bracepos 1").unwrap()).unwrap_err();
        assert!(err.msg.contains("bracepos"), "{}", err.msg);
    }

    #[test]
    fn style_globals_and_dash_lengths_apply() {
        let d = draw("linethick = 3\nline dashed .2\nline dotted .05");
        let Shape::Path { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(style.thick, Some(3.0));
        assert_eq!(style.dash, Dash::Dashed(0.2));
        let Shape::Path { style, .. } = &d.shapes[1] else {
            panic!()
        };
        assert_eq!(style.dash, Dash::Dotted(Some(0.05)));
    }

    #[test]
    fn hatch_style_records_pattern_attributes() {
        let d = draw("box crosshatch hatchangle 30 hatchsep .05 hatchwidth 1.5 hatchcolor red");
        let Shape::Box { style, .. } = &d.shapes[0] else {
            panic!()
        };
        let hatch = style.hatch.as_ref().expect("expected hatch style");
        assert!(hatch.cross);
        assert!((hatch.angle - 30.0).abs() < 1e-9);
        assert!((hatch.sep - 0.05).abs() < 1e-9);
        assert!((hatch.width - 1.5).abs() < 1e-9);
        assert_eq!(hatch.color, "red");
        assert!(style.fill_open);
    }

    #[test]
    fn opacity_style_records_fill_opacity() {
        let d = draw("box fill .8 opacity .4");
        let Shape::Box { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(style.fill_opacity, Some(0.4));
    }

    #[test]
    fn block_opacity_multiplies_child_fill_opacity() {
        let d = draw("[ box opacity .5; circle ] opacity .5");
        let Shape::Box { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(style.fill_opacity, Some(0.25));
        let Shape::Circle { style, .. } = &d.shapes[1] else {
            panic!()
        };
        assert_eq!(style.fill_opacity, Some(0.5));
    }

    #[test]
    fn opacity_must_be_between_zero_and_one() {
        let err = eval(&parse("box opacity 1.1").unwrap()).unwrap_err();
        assert!(err.msg.contains("opacity"), "{}", err.msg);
        let err = eval(&parse("box opacity -0.1").unwrap()).unwrap_err();
        assert!(err.msg.contains("opacity"), "{}", err.msg);
    }

    #[test]
    fn standalone_text_rejects_opacity() {
        let err = eval(&parse("\"note\" opacity .5").unwrap()).unwrap_err();
        assert!(err.msg.contains("filled regions"), "{}", err.msg);
    }

    #[test]
    fn color_attribute_expands_runtime_macro_string() {
        let d = draw(
            "r = 0; g = 0; b = 0.6\n\
             if dpicopt == optSVG then {\n\
               define customcolor { sprintf(\"rgb(%g,%g,%g)\", int(r*255), int(g*255), int(b*255)) }\n\
             }\n\
             arc color customcolor",
        );
        let Shape::Arc { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(style.stroke.as_deref(), Some("rgb(0,0,153)"));
        assert_eq!(style.fill, Some(Fill::Color("rgb(0,0,153)".into())));
    }

    #[test]
    fn color_attribute_accepts_dpictools_rgbstring_macro_call() {
        let d = draw(
            "if dpicopt == optSVG then {\n\
               define rgbstring { sprintf(\"rgb(%g,%g,%g)\", int(($1)*255+0.5), int(($2)*255+0.5), int(($3)*255+0.5)) }\n\
             }\n\
             circle shaded rgbstring(1,0.84,0) outlined \"black\"",
        );
        let Shape::Circle { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(style.fill, Some(Fill::Color("rgb(255,214,0)".into())));
        assert_eq!(style.stroke.as_deref(), Some("black"));
    }

    #[test]
    fn ps_width_scales_drawing() {
        // issue #4, dpic oracle: `.PS 6` scales the painted picture, so the
        // box geometry is slightly under 6in once default stroke is reserved.
        let d = draw(".PS 6\nbox\n.PE");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 5.912_408_759).abs() < 1e-9, "w = {w}");
        assert!(
            (d.bbox.width() - 5.923_519_870).abs() < 1e-9,
            "w = {}",
            d.bbox.width()
        );
    }

    #[test]
    fn text_extent_in_bbox() {
        // issue #5: a bare label must yield a non-degenerate bbox (no clipping)
        let d = draw("\"a long label here\"");
        assert!(d.bbox.width() > 0.5, "w = {}", d.bbox.width());
        assert!(d.bbox.height() > 0.1, "h = {}", d.bbox.height());
        // text wider than its box widens the bbox beyond the box
        let d2 = draw("box wid 0.2 ht 0.2 \"a very wide label\"");
        assert!(d2.bbox.width() > 0.3, "w = {}", d2.bbox.width());
    }

    #[test]
    fn text_object_width_bounds_rendered_bbox() {
        // dpic oracle: a standalone text object's `wid` controls its bbox;
        // the literal text width is not used when an explicit width is given.
        let d = draw("\"abcdefghij\" wid 0.1");
        assert!(
            (d.bbox.width() - 0.1).abs() < 1e-9,
            "w = {}",
            d.bbox.width()
        );

        let d = draw(".PS 1\n\"abcdefghij\" wid 0.1\n.PE");
        assert!(
            (d.bbox.width() - 1.0).abs() < 1e-9,
            "w = {}",
            d.bbox.width()
        );
    }

    #[test]
    fn text_position_and_offset_expand_bbox_in_the_rendered_direction() {
        let d = draw("textoffset = 0.1\n\"abc\" ljust at (0,0)");
        assert!(d.bbox.min.x >= 0.1 - 1e-9, "{:?}", d.bbox);

        let d = draw("textoffset = 0.1\n\"abc\" rjust at (0,0)");
        assert!(d.bbox.max.x <= -0.1 + 1e-9, "{:?}", d.bbox);

        let d = draw("textoffset = 0.1\n\"abc\" above at (0,0)");
        assert!(d.bbox.min.y > 0.0, "{:?}", d.bbox);

        let d = draw("textoffset = 0.1\n\"abc\" below at (0,0)");
        assert!(d.bbox.max.y < 0.0, "{:?}", d.bbox);
    }

    #[test]
    fn invisible_geometry_does_not_expand_drawing_bbox() {
        let d = draw("box invis wid 1000 ht 1000 at (0,0)\nbox wid 1 ht 1 at (0,0)");
        assert!(
            (d.bbox.width() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
            "w = {}",
            d.bbox.width()
        );
        assert!(
            (d.bbox.height() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
            "h = {}",
            d.bbox.height()
        );

        let d2 = draw("line invis from (0,0) to (1000,1000)\nbox wid 1 ht 1 at (0,0)");
        assert!(
            (d2.bbox.width() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
            "w = {}",
            d2.bbox.width()
        );
        assert!(
            (d2.bbox.height() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
            "h = {}",
            d2.bbox.height()
        );

        let d3 = draw("I: box invis wid 1000 ht 1000 at (0,0)\nbox wid 1 ht 1 with .sw at I.ne");
        let Shape::Box { c, .. } = &d3.shapes[1] else {
            panic!()
        };
        assert!(c.dist(Point::new(500.5, 500.5)) < 1e-9, "c = {c:?}");
        assert!(
            (d3.bbox.width() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
            "w = {}",
            d3.bbox.width()
        );
        assert!(
            (d3.bbox.height() - (1.0 + DEFAULT_STROKE_IN)).abs() < 1e-9,
            "h = {}",
            d3.bbox.height()
        );
    }

    #[test]
    fn move_expands_drawing_bbox_like_dpic() {
        let d = draw("line from (0,0) to (1,0)\nmove left 0.4 from (0,0)");
        assert!(
            (d.bbox.width() - (1.4 + DEFAULT_STROKE_IN / 2.0)).abs() < 1e-9,
            "w = {}",
            d.bbox.width()
        );
    }

    #[test]
    fn division_by_zero_errors() {
        // a zero divisor must error rather than silently produce NaN coordinates
        assert!(eval(&parse("box wid 1/0").unwrap()).is_err());
        assert!(eval(&parse("A:(0,0)\nB:(0,0)\nx = (B.x-A.x)/(B.y-A.y)").unwrap()).is_err());
    }

    #[test]
    fn non_finite_numeric_values_error() {
        let literal = parse("box wid 1e999 ht 1").unwrap_err();
        assert!(literal.msg.contains("not finite"), "{literal}");

        for src in [
            "box wid exp(1000) ht 1",
            "box wid sqrt(-1) ht 1",
            "scale = exp(1000)\nbox",
            "x = 1e308\nx *= 1e308\nbox wid x",
        ] {
            let err = eval(&parse(src).unwrap()).unwrap_err();
            assert!(err.msg.contains("non-finite"), "{src}: {err}");
        }
    }

    #[test]
    fn place_dot_in_coordinate_pair() {
        // (A.x, A.y - 1) — place scalar accessors inside a coordinate pair (issue #3)
        let d = draw("A: box wid 1 ht 1 at 2,3\nbox wid 0.2 ht 0.2 at (A.x, A.y - 1)");
        let Shape::Box { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!(
            (c.x - 2.0).abs() < 1e-9 && (c.y - 2.0).abs() < 1e-9,
            "c = {c:?}"
        );
    }

    #[test]
    fn bare_coordinate_pair_places_label() {
        let d = draw("P: 1,2\nbox wid 0.2 ht 0.2 at P");
        let Shape::Box { c, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(c.dist(Point::new(1.0, 2.0)) < 1e-9, "c = {c:?}");
    }

    #[test]
    fn block_sub_labels_resolve() {
        // `B.A` and `B.A.corner` reach a labelled object inside a block
        let d = draw("B: [ A: box wid 1 ht 1 at 0,0 ]\nbox wid 0.2 ht 0.2 with .sw at B.A.ne");
        let Shape::Box { c, .. } = &d.shapes.last().unwrap() else {
            panic!()
        };
        // the block is placed with its center at (0.5,0); inner A.ne is then
        // (1.0,0.5), and the small box centers 0.1 beyond that corner.
        assert!(
            (c.x - 1.1).abs() < 1e-9 && (c.y - 0.6).abs() < 1e-9,
            "c = {c:?}"
        );
    }

    #[test]
    fn block_can_anchor_on_own_member() {
        // The block bbox center is not A.c, so this catches the two-pass anchor
        // resolution rather than accidentally aligning the block center.
        let d = draw("P:(2,3)\n[ A: box wid 1 ht 1 at 0,0; circle rad 0.1 at 2,0 ] with .A.c at P");
        let Shape::Box { c, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(c.dist(Point::new(2.0, 3.0)) < 1e-9, "A center = {c:?}");
        let Shape::Circle { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!(c.dist(Point::new(4.0, 3.0)) < 1e-9, "circle center = {c:?}");
    }

    #[test]
    fn block_pair_anchor_uses_local_coordinates() {
        // dpic oracle: for `[ ... ] with (x,y) at P`, `(x,y)` is a point in
        // the block's local coordinate system, not an offset from its center.
        let d = draw("[ box wid 2 ht 1 at (1,0) ] with (0,0) at (10,20)");
        let Shape::Box { c, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(c.dist(Point::new(11.0, 20.0)) < 1e-9, "c = {c:?}");
    }

    #[test]
    fn block_layout_matches_dpic_for_negative_box_width() {
        let d = draw("move 1\n[ box wid -0.5 ht 0.5 ]; box wid 0.75 ht 0.75");
        let boxes: Vec<Point> = d
            .shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Box { c, .. } => Some(*c),
                _ => None,
            })
            .collect();
        assert_eq!(boxes.len(), 2);
        assert!(
            boxes[0].dist(Point::new(0.75, 0.0)) < 1e-9,
            "negative box center = {:?}",
            boxes[0]
        );
        assert!(
            boxes[1].dist(Point::new(1.375, 0.0)) < 1e-9,
            "following box center = {:?}",
            boxes[1]
        );
    }

    #[test]
    fn block_object_renders_attached_text() {
        let d = draw("[ box ] \"block label\"");
        assert!(d.shapes.iter().any(|s| {
            matches!(
                s,
                Shape::Text { text, .. } if text.iter().any(|line| line.s == "block label")
            )
        }));
    }

    #[test]
    fn block_anchors_ignore_attached_text_extents() {
        // dpic oracle: text contributes to the painted bbox, but not to block
        // anchors such as `last [].s`; those come from the geometric objects.
        let d = draw(
            r#"B: [ right; box "{\bf veryveryverywide}"; move; box ]
box wid 0.1 ht 0.1 at B.s"#,
        );
        let Shape::Box { c, .. } = d.shapes.last().unwrap() else {
            panic!()
        };
        assert!(c.dist(Point::new(1.0, -0.25)) < 1e-9, "c = {c:?}");
    }

    #[test]
    fn nested_macro_block_can_reference_parent_label() {
        let d = draw(
            "define marker { [ P: circle rad 0.01 at $1.start ] with .P at $1.start }\n[ A: arrow from (0,0) to (1,0); marker(A) ]",
        );
        let Shape::Path { pts, .. } = &d.shapes[0] else {
            panic!()
        };
        let Shape::Circle { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!(
            c.dist(pts[0]) < 1e-9,
            "circle = {c:?}, arrow start = {:?}",
            pts[0]
        );
    }

    #[test]
    fn position_vector_arithmetic() {
        // (w,h)/2 and p + q with correct precedence
        let d = draw("box wid 0.2 ht 0.2 at (2,4)/2 + (1,0)");
        let Shape::Box { c, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(
            (c.x - 2.0).abs() < 1e-9 && (c.y - 2.0).abs() < 1e-9,
            "c = {c:?}"
        );
    }

    #[test]
    fn interpolation_angle_brackets() {
        let d = draw("A:(0,0)\nB:(2,0)\nbox wid 0.1 ht 0.1 at 0.5 <A,B>");
        let Shape::Box { c, .. } = &d.shapes.last().unwrap() else {
            panic!()
        };
        assert!((c.x - 1.0).abs() < 1e-9, "c = {c:?}");
    }

    #[test]
    fn string_equality_in_if() {
        // the `"$1"==""` default-argument idiom (here without a macro)
        let d1 = draw("if \"a\" == \"\" then { box } else { circle }");
        assert!(matches!(d1.shapes[0], Shape::Circle { .. }));
        let d2 = draw("if \"\" == \"\" then { box } else { circle }");
        assert!(matches!(d2.shapes[0], Shape::Box { .. }));
    }

    #[test]
    fn dpicopt_defaults_to_svg_backend() {
        let d = draw("if dpicopt == optSVG then { box } else { circle }");
        assert!(matches!(d.shapes[0], Shape::Box { .. }));
    }

    #[test]
    fn svg_font_stub_and_string_sprintf_are_harmless() {
        let d = draw("box sprintf(\"x%s\", svg_font(\"Times\", 12))");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].s, "x");
    }

    #[test]
    fn inch_suffix_and_bare_distance() {
        // `.5i` is half an inch; `move 1` / `move -0.1` advance the pen
        let d = draw("box wid .5i ht .5i");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 0.5).abs() < 1e-9, "w = {w}");
        let d2 = draw("right\nmove 1\nbox at Here");
        let Shape::Box { c, .. } = &d2.shapes.last().unwrap() else {
            panic!()
        };
        assert!(c.x > 0.9, "moved to {c:?}");
    }

    #[test]
    fn embedded_assignment_returns_value() {
        let d = draw("if (s = 3) > 1 then { box wid s ht 0.1 }");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 3.0).abs() < 1e-9, "w = {w}");
    }

    #[test]
    fn copy_includes_a_file() {
        // `copy "file"` splices another pic file relative to the base directory
        let dir = std::env::temp_dir().join(format!("rpic_copy_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("inc.pic"), "box wid 0.5 ht 0.5\n").unwrap();
        let pic = parse_in_dir("copy \"inc.pic\"\ncircle", Some(dir.as_path())).unwrap();
        let d = eval(&pic).unwrap();
        assert!(d.shapes.iter().any(|s| matches!(s, Shape::Box { .. })));
        assert!(d.shapes.iter().any(|s| matches!(s, Shape::Circle { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paren_position_coordinate() {
        // `.x`/`.y` on a parenthesised position expression
        let d = draw("A:(0,0)\nB:(3,1)\nbox wid (B-A).x ht (B-A).y at 0,-2");
        let Shape::Box { w, h, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(
            (*w - 3.0).abs() < 1e-9 && (*h - 1.0).abs() < 1e-9,
            "{w} x {h}"
        );
    }

    #[test]
    fn arrowhead_size_follows_globals() {
        // arrowht/arrowwid control the rendered arrowhead, not a hardcoded size
        let d = draw("arrowht = 0.3; arrowwid = 0.2\narrow right 1");
        let Shape::Path { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(
            (style.arrow_ht - 0.3).abs() < 1e-9,
            "ht = {}",
            style.arrow_ht
        );
        assert!(
            (style.arrow_wid - 0.2).abs() < 1e-9,
            "wid = {}",
            style.arrow_wid
        );
    }

    #[test]
    fn scaling_existing_geometry_scales_arrowhead_metadata() {
        // manual/man35 draws at a temporary scale, then restores `scale`.
        // The points are scaled at restore time, and the arrowhead dimensions
        // attached to the already-emitted path must scale with them.
        let factor = 6.6 / 8.2;
        let d = draw("scale = 6.6/8.2\nline <-\nscale = 1");
        let Shape::Path { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(
            (style.arrow_ht - 0.1 * factor).abs() < 1e-9,
            "ht = {}",
            style.arrow_ht
        );
        assert!(
            (style.arrow_wid - 0.05 * factor).abs() < 1e-9,
            "wid = {}",
            style.arrow_wid
        );
    }

    #[test]
    fn dpic_default_env_values_are_readable() {
        assert!((scalar("textoffset").unwrap() - 2.0 / 72.0).abs() < 1e-9);
        assert!((scalar("textht").unwrap() - (11.0 / 72.0) * 0.66).abs() < 1e-9);
        assert!((scalar("arrowhead").unwrap() - 1.0).abs() < 1e-9);
        assert!((scalar("linethick").unwrap() - 0.8).abs() < 1e-9);
        assert_eq!(scalar("margin").unwrap(), 0.0);
        assert_eq!(scalar("topmargin").unwrap(), 0.0);
        assert_eq!(scalar("rightmargin").unwrap(), 0.0);
        assert_eq!(scalar("bottommargin").unwrap(), 0.0);
        assert_eq!(scalar("leftmargin").unwrap(), 0.0);
    }

    #[test]
    fn font_attrs_bind_per_string_like_ljust() {
        // after the string: binds to the preceding one
        let d = draw("box \"a\" bold \"b\" italic fontsize 9");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(text[0].bold && !text[0].italic && text[0].size_pt.is_none());
        assert!(text[1].italic && !text[1].bold && text[1].size_pt == Some(9.0));
        // before any string: binds to the next one only
        let d = draw("box bold \"a\" \"b\"");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(text[0].bold && !text[1].bold);
        // mono and font "…" set the family
        let d = draw("box \"a\" mono \"b\" font \"Georgia\"");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].family.as_deref(), Some("monospace"));
        assert_eq!(text[1].family.as_deref(), Some("Georgia"));
    }

    #[test]
    fn font_attrs_feed_fit_and_bbox() {
        let plain = draw("box \"word\" fit");
        let bold = draw("box \"word\" bold fit");
        let big = draw("box \"word\" fontsize 22 fit");
        assert!(bold.bbox.width() > plain.bbox.width());
        assert!(big.bbox.width() > bold.bbox.width());
        assert!(big.bbox.height() > plain.bbox.height());
        // standalone text: default height follows the fontsize ratio
        let plain = draw("\"word\"");
        let big = draw("\"word\" fontsize 22");
        assert!(big.bbox.height() > 1.5 * plain.bbox.height());
    }

    #[test]
    fn font_attrs_reject_bad_sizes() {
        let e = eval(&parse("box \"x\" fontsize 0").unwrap()).unwrap_err();
        assert!(e.msg.contains("positive number of points"), "{}", e.msg);
        let e = eval(&parse("box \"x\" fontsize -3").unwrap()).unwrap_err();
        assert!(e.msg.contains("positive number of points"), "{}", e.msg);
    }

    #[test]
    fn font_attr_words_stay_usable_as_variables() {
        // `bold`/`italic`/`mono` only act in attribute position; as plain
        // variables in expressions they keep their classic meaning
        let d = draw("bold = 2\nbox wid bold \"x\"");
        assert!((d.bbox.width() - 2.0).abs() < 0.05, "{}", d.bbox.width());
    }

    #[test]
    fn with_start_end_edge_aligns_closed_objects() {
        // #240: `with .start at X` / `with .end at X` edge-align, not center.
        // right dir: B.start (its .w) on A.end=(1,0) → B centered at 1.2
        let d = draw("A: box wid 1 ht 0.5\nB: box wid 0.4 ht 0.4 with .start at A.end");
        let Shape::Box { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!((c.x - 1.2).abs() < 1e-9, "B.c.x = {}", c.x);
        // .end anchor: B.end (its .e) on A.start=(1.5,0) → B centered at 1.3
        let d = draw("A: box wid 1 ht 0.5 at (2,0)\nB: box wid 0.4 ht 0.4 with .end at A.start");
        let Shape::Box { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!((c.x - 1.3).abs() < 1e-9, "B.c.x = {}", c.x);
        // vertical up: .start maps to the south edge → B centered at y=0.7
        let d =
            draw("up\nA: box wid 0.5 ht 1 at (0,0)\nB: box wid 0.4 ht 0.4 with .start at A.end");
        let Shape::Box { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!((c.y - 0.7).abs() < 1e-9, "B.c.y = {}", c.y);
        // .c anchor unchanged: B centered on A.e=(1,0)
        let d = draw("A: box wid 1 ht 0.5\nB: box wid 0.4 with .c at A.e");
        let Shape::Box { c, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!((c.x - 1.0).abs() < 1e-9, "B.c.x = {}", c.x);
    }

    #[test]
    fn previous_is_a_synonym_for_last() {
        // #240: pikchr `previous` == `last`
        let a = crate::to_svg(&draw("box\ncircle at previous.e rad 0.1"));
        let b = crate::to_svg(&draw("box\ncircle at last.e rad 0.1"));
        assert_eq!(a, b);
        // `previous box`, `2nd previous box` parse and resolve
        assert!(eval(&parse("box; box\ncircle at previous box.n").unwrap()).is_ok());
        assert!(eval(&parse("box; box; box\ncircle at 2nd previous box.n").unwrap()).is_ok());
    }

    #[test]
    fn aligned_rotates_label_to_segment_angle() {
        // #240: aligned sets the label rotation to the segment angle, readable
        let d = draw("line from (0,0) to (2,2) \"up\" aligned");
        let Shape::Path { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(
            (text[0].rotate.unwrap() - 45.0).abs() < 1e-6,
            "{:?}",
            text[0].rotate
        );
        // horizontal → no rotation (upright, byte-identical to a plain label)
        let d = draw("line right 2 \"flat\" aligned");
        let Shape::Path { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].rotate, None);
        // leftward → normalized to stay readable (not 180)
        let d = draw("line from (2,0) to (0,0) \"back\" aligned");
        let Shape::Path { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].rotate, None); // 180 → 0 (upright)
    }

    #[test]
    fn big_small_size_labels() {
        // #240: pikchr big/small sugar over fontsize (1.5× / 0.7× of 11pt)
        let d = draw("box \"a\" big \"b\" small");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].size_pt, Some(16.5));
        assert!((text[1].size_pt.unwrap() - 7.7).abs() < 1e-9);
        // no ignored_attribute warning
        let d = draw("box \"a\" big");
        assert!(d.warnings.is_empty());
    }

    #[test]
    fn rotated_binds_per_string_and_grows_fit() {
        let d = draw("box \"a\" rotated 45 \"b\"");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].rotate, Some(45.0));
        assert_eq!(text[1].rotate, None);
        // a rotated label needs a taller fit box
        let plain = draw("box \"long caption\" fit");
        let rot = draw("box \"long caption\" rotated 30 fit");
        assert!(rot.bbox.height() > plain.bbox.height());
        // standalone rotated text: canvas covers the rotated extent
        let plain = draw("\"long caption text\"");
        let rot = draw("\"long caption text\" rotated 90");
        assert!(rot.bbox.height() > 2.0 * plain.bbox.height());
    }

    #[test]
    fn color_literals_evaluate_to_hex() {
        let d = draw("box shaded rgb(27,94,32)");
        let Shape::Box { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(style.fill, Some(Fill::Color("#1b5e20".into())));
        let d = draw("box shaded 0x1B5E20");
        let Shape::Box { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(style.fill, Some(Fill::Color("#1b5e20".into())));
        // expressions inside rgb()
        let d = draw("v = 200\nbox shaded rgb(v, v/2, 0)");
        let Shape::Box { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(style.fill, Some(Fill::Color("#c86400".into())));
    }

    #[test]
    fn color_literals_reject_out_of_range() {
        let e = eval(&parse("box shaded rgb(300,0,0)").unwrap()).unwrap_err();
        assert!(e.msg.contains("0-255"), "{}", e.msg);
        let e = eval(&parse("box shaded 0x1FFFFFF").unwrap()).unwrap_err();
        assert!(e.msg.contains("0-0xFFFFFF"), "{}", e.msg);
    }

    #[test]
    fn color_literal_words_stay_classic_elsewhere() {
        // `rotated` as a variable; `rgb` as a macro name; quoted colors as before
        let d = draw("rotated = 2\nbox wid rotated");
        assert!((d.bbox.width() - 2.0).abs() < 0.05);
        let d = draw("define rgb { box }\nrgb");
        assert_eq!(d.shapes.len(), 1);
        let d = draw("box shaded \"#1b5e20\"");
        let Shape::Box { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(style.fill, Some(Fill::Color("#1b5e20".into())));
    }

    #[test]
    fn hex_number_literals_lex_as_numbers() {
        // 0x literals are plain numbers everywhere, not just colors
        let d = draw("box wid 0x2 ht 0x1");
        assert!((d.bbox.width() - 2.0).abs() < 0.05, "{}", d.bbox.width());
    }

    #[test]
    fn canvas_stmt_fixes_the_page_rect() {
        let d = draw("canvas from (0,0) to (4,3)\nbox at (1,1)");
        let c = d.canvas.unwrap();
        assert_eq!((c.min.x, c.min.y, c.max.x, c.max.y), (0.0, 0.0, 4.0, 3.0));
        // corners in either order, and place references both work
        let d = draw("F: box wid 3 ht 2 at (1.5,1) invis\ncanvas from F.ne to F.sw");
        let c = d.canvas.unwrap();
        assert_eq!((c.min.x, c.min.y, c.max.x, c.max.y), (0.0, 0.0, 3.0, 2.0));
        // last statement wins
        let d = draw("canvas from (0,0) to (9,9)\ncanvas from (0,0) to (1,1)\nbox");
        assert_eq!(d.canvas.unwrap().max.x, 1.0);
    }

    #[test]
    fn canvas_stmt_is_inert_as_a_variable() {
        // `canvas = 3` stays a plain assignment; only `canvas from …` triggers
        let d = draw("canvas = 3\nbox wid canvas");
        assert!(d.canvas.is_none());
        // painted bbox includes the stroke — compare loosely
        assert!((d.bbox.width() - 3.0).abs() < 0.05, "{}", d.bbox.width());
    }

    #[test]
    fn canvas_stmt_rejects_degenerate_rects() {
        let e = eval(&parse("canvas from (0,0) to (0,3)").unwrap()).unwrap_err();
        assert!(e.msg.contains("positive width and height"), "{}", e.msg);
    }

    #[test]
    fn canvas_stmt_scales_with_the_picture() {
        // `scale = 2`: canvas given in user units, halved internally
        let d = draw("scale = 2\ncanvas from (0,0) to (8,6)\nbox");
        let c = d.canvas.unwrap();
        assert!((c.max.x - 4.0).abs() < 1e-9 && (c.max.y - 3.0).abs() < 1e-9);
        // maxps clamps the *page* (the fixed canvas), not the content bbox
        let d = draw("maxpswid = 2\ncanvas from (0,0) to (4,3)\nbox wid 1");
        let c = d.canvas.unwrap();
        assert!((c.width() - 2.0).abs() < 1e-6, "{}", c.width());
        assert!((d.bbox.width() - 0.5).abs() < 0.05, "{}", d.bbox.width());
    }

    #[test]
    fn canvas_stmt_propagates_out_of_blocks_translated() {
        // canvas is global like variables; a block's rect lands in parent space
        let d =
            draw("box wid 1 ht 1 at (0.5,0.5)\n[ canvas from (0,0) to (2,1) ] with .sw at (0,0)");
        let c = d.canvas.unwrap();
        assert!((c.min.x - 0.0).abs() < 1e-9, "{}", c.min.x);
        assert!((c.width() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn canvas_margin_vars_are_scaled_dimensions() {
        let d = draw("margin = 1; topmargin = 0.5; rightmargin = 0.25; line right");
        assert_eq!(
            d.canvas_margin,
            CanvasMargin {
                top: 1.5,
                right: 1.25,
                bottom: 1.0,
                left: 1.0,
            }
        );

        let d = draw("scale = 2; margin = 1; topmargin = 1; line right");
        assert_eq!(
            d.canvas_margin,
            CanvasMargin {
                top: 1.0,
                right: 0.5,
                bottom: 0.5,
                left: 0.5,
            }
        );

        let d = draw("margin = 1; scale = 2; line right");
        assert_eq!(
            d.canvas_margin,
            CanvasMargin {
                top: 1.0,
                right: 1.0,
                bottom: 1.0,
                left: 1.0,
            }
        );
    }

    #[test]
    fn print_statements_collect_diagnostics() {
        let d = draw("print 5.5\nprint 5.5%2\nprint \"hello\"\nprint sprintf(\"x=%g\", 1.25)");
        assert_eq!(d.diagnostics, ["5.5", "0", "hello", "x=1.25"]);

        let d = draw("[ print \"inside\"; box ]\nprint 7");
        assert_eq!(d.diagnostics, ["inside", "7"]);
    }

    #[test]
    fn command_and_sh_are_silent_noops() {
        // Policy (#129): `command` raw backend text is never injected and `sh`
        // is never executed. Both are tolerated so dpic sources keep
        // compiling, and they emit no diagnostic lines and no shapes.
        let d = draw("box wid 1 ht 1\nsh \"echo hi\"\ncommand \"</g>\"\nbox wid 1 ht 1");
        assert!(d.diagnostics.is_empty(), "{:?}", d.diagnostics);
        assert_eq!(d.shapes.len(), 2);

        // Geometry flows across the skipped directives unchanged: the second
        // box lands exactly where it would without them.
        let plain = draw("box wid 1 ht 1\nbox wid 1 ht 1");
        assert_eq!(d.bbox, plain.bbox);
    }

    #[test]
    fn gradient_style_records_stops_and_angle() {
        let d = draw("box gradient \"steelblue\" \"white\" gradientangle 45");
        let Shape::Box { style, .. } = &d.shapes[0] else {
            panic!()
        };
        let g = style.gradient.as_ref().expect("expected gradient");
        assert_eq!(g.from, "steelblue");
        assert_eq!(g.to, "white");
        assert!((g.angle - 45.0).abs() < 1e-9);
        assert!(style.fill_open);

        // gradientangle alone creates the default black-to-white gradient,
        // mirroring how `hatchangle` alone creates a default hatch
        let d = draw("box gradientangle 90");
        let Shape::Box { style, .. } = &d.shapes[0] else {
            panic!()
        };
        let g = style.gradient.as_ref().unwrap();
        assert_eq!((g.from.as_str(), g.to.as_str()), ("black", "white"));
    }

    fn fake_math(tex: &str, font_pt: f64) -> Result<crate::math::MathSpan, String> {
        if tex.contains("boom") {
            return Err("fake parse error".into());
        }
        let em = font_pt / 72.0;
        Ok(crate::math::MathSpan {
            svg: format!("<svg width=\"9.6\" height=\"14.08\"><!--{tex}--></svg>"),
            width: 2.0 * em,
            height: 0.8 * em,
            depth: 0.2 * em,
        })
    }

    #[test]
    fn texlabels_routes_dollar_labels_through_the_math_hook() {
        crate::math::set_math_renderer(fake_math);

        // off by default: no math span even with a renderer registered
        let d = draw("box \"$x$\"");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(text[0].math.is_none());

        // on: fully $-delimited labels are typeset; others stay literal
        let d = draw("texlabels = 1\nbox \"$x$\" \"plain\" \"$a$b$\"");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        let m = text[0].math.as_ref().expect("math span");
        assert!((m.width - 2.0 * 11.0 / 72.0).abs() < 1e-9);
        assert!(text[0].s.contains("$x$")); // literal kept for fallback
        assert!(text[1].math.is_none());
        assert!(text[2].math.is_none()); // inner `$` disqualifies

        // exact metrics drive the text bbox (2 em wide, not 3 chars * 0.6 em)
        let d = draw("texlabels = 1\n\"$x$\" at (0,0)");
        assert!(
            (d.bbox.width() - 2.0 * 11.0 / 72.0).abs() < 0.02,
            "{}",
            d.bbox.width()
        );

        // renderer failure: literal fallback plus a diagnostic, never an error
        let d = draw("texlabels = 1\nbox \"$boom$\"");
        let Shape::Box { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(text[0].math.is_none());
        assert!(
            d.diagnostics.iter().any(|l| l.contains("fake parse error")),
            "{:?}",
            d.diagnostics
        );
    }

    #[test]
    fn dot_is_a_solid_circle_with_dotrad_default() {
        let d = draw("dot at (0.5, 0.5)");
        let Shape::Circle { r, style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((r - 0.035).abs() < 1e-9);
        assert_eq!(style.fill, Some(Fill::Gray(0.0)));

        // dotrad env var + attribute overrides
        let d = draw("dotrad = 0.06\ndot\ndot rad 0.1 shaded \"red\"");
        let Shape::Circle { r, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((r - 0.06).abs() < 1e-9);
        let Shape::Circle { r, style, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!((r - 0.1).abs() < 1e-9);
        assert_eq!(style.fill, Some(Fill::Color("red".into())));

        // contextual: dot stays usable as a variable
        let d = draw("dot = 2\nbox wid dot ht 0.3");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((w - 2.0).abs() < 1e-9);

        // dots are circles for ordinals
        let d = draw("dot at (0,0)\nbox at last circle + (0.5, 0)");
        assert_eq!(d.shapes.len(), 2);
    }

    #[test]
    fn class_attribute_and_statement_set_shape_classes() {
        // inline attribute, and append composition
        let d = draw("box class \"critical\" class \"hot\"");
        assert_eq!(d.shape_classes[0].as_deref(), Some("critical hot"));

        // statement form by label, by ordinal, and reaching macro-drawn shapes
        let d = draw(
            "define wire { line right 0.5 }\nA: box\nwire()\nclass A \"node\"\nclass last line \"bus\"",
        );
        assert_eq!(d.shape_classes[0].as_deref(), Some("node"));
        assert_eq!(d.shape_classes[1].as_deref(), Some("bus"));

        // `class` stays usable as a plain variable
        let d = draw("class = 2\nbox wid class ht 1");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((w - 2.0).abs() < 1e-9);
    }

    #[test]
    fn class_validates_names_and_targets() {
        let e = eval(&parse("box class \"a<b\"").unwrap()).unwrap_err();
        assert!(e.msg.contains("invalid class name"), "{e}");

        let e = eval(&parse("box class \"2fast\"").unwrap()).unwrap_err();
        assert!(e.msg.contains("invalid class name"), "{e}");

        let e = eval(&parse("A: (0,0)\nclass A \"x\"").unwrap()).unwrap_err();
        assert!(e.msg.contains("no drawn shape"), "{e}");
    }

    #[test]
    fn class_composes_with_animate_on_the_same_shape() {
        // The class hook and the animation layer share the `s<N>` contract:
        // both must resolve to the same shape index, and adding a class must
        // not disturb the animation target.
        let d = draw(
            "A: box\ncircle\nanimate A with \"pop\"\nclass A \"critical\"\nanimate last circle with \"fade\"\nclass last circle \"soft\"",
        );
        assert_eq!(d.anims.len(), 2);
        assert_eq!(d.anims[0].shape, 0);
        assert_eq!(d.anims[1].shape, 1);
        assert_eq!(d.shape_classes[0].as_deref(), Some("critical"));
        assert_eq!(d.shape_classes[1].as_deref(), Some("soft"));
    }

    #[test]
    fn class_inside_block_survives_flattening() {
        let d = draw("[ box class \"in\"; circle ]");
        assert_eq!(d.shape_classes[0].as_deref(), Some("in"));
        assert_eq!(d.shape_classes[1], None);

        let e = eval(&parse("[ box ] class \"x\"").unwrap()).unwrap_err();
        assert!(e.msg.contains("block"), "{e}");
    }

    #[test]
    fn open_object_width_height_attrs_are_arrowhead_dimensions() {
        let d = draw("arrowwid = 0.2; arrowht = 0.3\nA: line right 2\nbox wid (A.wid) ht (A.ht)");
        assert_box_size(&d.shapes[1], 0.2, 0.3);

        let d = draw("arrowwid = 0.12; arrowht = 0.34\nA: move right 2\nbox wid (A.wid) ht (A.ht)");
        assert_box_size(&d.shapes[1], 0.12, 0.34);

        let d = draw(
            "arrowwid = 0.23; arrowht = 0.31\nA: spline from (0,0) to (2,1)\nbox wid (A.wid) ht (A.ht)",
        );
        assert_box_size(&d.shapes[1], 0.23, 0.31);
    }

    #[test]
    fn radius_and_diameter_attrs_are_type_specific() {
        let d = draw("B: box rad 0.1 wid 1 ht 1\nbox wid (B.rad) ht 0.3");
        assert_box_size(&d.shapes[1], 0.1, 0.3);

        let d = draw("C: arc rad 0.7 from (0,0) to (0,1.4)\nbox wid (C.rad) ht (C.diam)");
        assert_box_size(&d.shapes[1], 0.7, 1.4);
    }

    #[test]
    fn invalid_type_scalar_attrs_match_dpic_zero() {
        let prog = parse(
            "E: ellipse wid 2 ht 1\nB: box wid 1 ht 1\nA: arc rad .5\n\
             e_rad = E.rad\ne_diam = E.diam\nb_diam = B.diam\na_len = A.len",
        )
        .unwrap();
        let mut st = State::new();
        st.eval_stmts(&prog.stmts).unwrap();
        assert_eq!(st.vars["e_rad"], 0.0);
        assert_eq!(st.vars["e_diam"], 0.0);
        assert_eq!(st.vars["b_diam"], 0.0);
        assert_eq!(st.vars["a_len"], 0.0);
    }

    #[test]
    fn arrowhead_type_open_vs_filled() {
        // default is a filled head; `arrowhead = 0` is an open (two-stroke) head
        let d = draw("arrow right 1");
        let Shape::Path { style, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(style.arrow_filled, "default should be filled");
        let d2 = draw("arrowhead = 0\narrow right 1");
        let Shape::Path { style, .. } = &d2.shapes[0] else {
            panic!()
        };
        assert!(!style.arrow_filled, "arrowhead=0 should be open");

        let d3 = draw("arrowhead = 0\nline <- 1 up");
        let Shape::Path { style, .. } = &d3.shapes[0] else {
            panic!()
        };
        assert!(style.arrow_filled, "`<- 1` should override the global");

        let d4 = draw("line <- 0 up");
        let Shape::Path { style, .. } = &d4.shapes[0] else {
            panic!()
        };
        assert!(!style.arrow_filled, "`<- 0` should override the global");
    }

    #[test]
    fn maxps_clamps_oversized_drawing() {
        // larger than the default 8.5x11in page → scaled down to fit
        let d = draw("box wid 20 ht 30");
        assert!(
            d.bbox.width() <= 8.5 + 1e-6 && d.bbox.height() <= 11.0 + 1e-6,
            "{}x{}",
            d.bbox.width(),
            d.bbox.height()
        );
        // raising the limits disables the clamp
        let d2 = draw("maxpsht = 200; maxpswid = 50\nbox wid 20 ht 30");
        assert!(
            (d2.bbox.height() - (30.0 + DEFAULT_STROKE_IN)).abs() < 1e-6,
            "h = {}",
            d2.bbox.height()
        );
        // a small drawing is untouched
        let d3 = draw("box wid 2 ht 1");
        assert!((d3.bbox.width() - (2.0 + DEFAULT_STROKE_IN)).abs() < 1e-6);

        let d4 = draw("maxpswid = 2; maxpsht = 100\nmargin = 1\nbox wid 1 ht 0.5");
        assert!(
            d4.bbox.width() + d4.canvas_margin.horizontal() <= 2.0 + 1e-6,
            "canvas width = {}",
            d4.bbox.width() + d4.canvas_margin.horizontal()
        );
        assert!(
            d4.canvas_margin.left < 1.0 && d4.canvas_margin.right < 1.0,
            "{:?}",
            d4.canvas_margin
        );
    }

    #[test]
    fn block_variable_assignments_are_local() {
        let d = draw("x = 1\n[ x = 5 ]\nbox wid x ht 0.3");
        let Shape::Box { w, .. } = d.shapes.last().unwrap() else {
            panic!()
        };
        assert!((*w - 1.0).abs() < 1e-9, "w = {w}");

        assert!(eval(&parse("[ x = 5 ]\nbox wid x ht 0.3").unwrap()).is_err());
    }

    #[test]
    fn block_env_assignments_are_local() {
        let d = draw("[ boxwid = 2; box ]\nbox");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 2.0).abs() < 1e-9, "inner w = {w}");
        let Shape::Box { w, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!((*w - 0.75).abs() < 1e-9, "outer w = {w}");
    }

    #[test]
    fn block_mutating_var_assignments_update_inherited_vars() {
        let d = draw("x = 1\n[ x := 5 ]\nbox wid x ht 0.3");
        let Shape::Box { w, .. } = d.shapes.last().unwrap() else {
            panic!()
        };
        assert!((*w - 5.0).abs() < 1e-9, "w = {w}");

        let d = draw("x = 1\n[ x += 2 ]\nbox wid x ht 0.3");
        let Shape::Box { w, .. } = d.shapes.last().unwrap() else {
            panic!()
        };
        assert!((*w - 3.0).abs() < 1e-9, "w = {w}");

        assert!(eval(&parse("[ x = 1; x += 2 ]\nbox wid x ht 0.3").unwrap()).is_err());

        let d = draw("boxwid = 0.75\n[ boxwid := 2; box ]\nbox");
        let Shape::Box { w, .. } = &d.shapes[1] else {
            panic!()
        };
        assert!((*w - 0.75).abs() < 1e-9, "outer w = {w}");
    }

    #[test]
    fn figuras_examples_compile() {
        // a few of André Leite's circuit_macros figures (examples/figuras/),
        // adapted with the compatibility shim — they must keep compiling/drawing.
        for src in [
            include_str!("../../../examples/figuras/fig01.pic"),
            include_str!("../../../examples/figuras/fig36.pic"),
            include_str!("../../../examples/figuras/fig40.pic"),
        ] {
            let d = eval(&parse(src).unwrap()).unwrap();
            assert!(!d.shapes.is_empty());
        }
    }

    #[test]
    fn figuras_element_examples_compile() {
        // André Leite's circuit_macros figures that use the *element API*
        // (resistor(dir len), bi_tr, opamp, …). These render with the circuit
        // library (-c) plus the compatibility shim, which reuses the native
        // element geometry. The shim is `copy`-d in by each file; here we splice
        // it in directly and prepend the circuit library.
        let shim = include_str!("../../../examples/figuras/circuit_macros.pic");
        for body in [
            include_str!("../../../examples/figuras/fig21.pic"),
            include_str!("../../../examples/figuras/fig23.pic"),
            include_str!("../../../examples/figuras/fig26.pic"),
            include_str!("../../../examples/figuras/fig27.pic"),
            include_str!("../../../examples/figuras/fig28.pic"),
            include_str!("../../../examples/figuras/fig30.pic"),
            include_str!("../../../examples/figuras/fig33.pic"),
            include_str!("../../../examples/figuras/fig45.pic"),
            include_str!("../../../examples/figuras/fig46.pic"),
            include_str!("../../../examples/figuras/fig09.pic"),
            include_str!("../../../examples/figuras/fig11.pic"),
        ] {
            let body = body.replace("copy \"circuit_macros.pic\"", shim);
            let src = format!("{}\n{}", crate::CIRCUITS, body);
            let d = eval(&parse(&src).unwrap()).unwrap();
            assert!(!d.shapes.is_empty());
        }
    }

    #[test]
    fn lib3d_examples_compile() {
        // The lib3D shim (3D -> 2D axonometric projection) and its demos must
        // keep compiling and drawing. The demos `copy` the shim; splice it in.
        let shim = include_str!("../../../examples/lib3d/lib3d.pic");
        for body in [
            include_str!("../../../examples/lib3d/frame.pic"),
            include_str!("../../../examples/lib3d/views.pic"),
        ] {
            let src = body.replace("copy \"lib3d.pic\"", shim);
            let d = eval(&parse(&src).unwrap()).unwrap();
            assert!(!d.shapes.is_empty());
        }
    }

    #[test]
    fn brace_ncount_as_place() {
        // `{expr}th last box` — a brace-counted ordinal used as a place
        let d = draw(
            "box at 0,0\nbox at 2,0\nbox at 4,0\narrow from {2}th last box.e to {1}th last box.w",
        );
        let Shape::Path { pts, .. } = d.shapes.last().unwrap() else {
            panic!()
        };
        assert!(pts[0].x > 2.0 && pts.last().unwrap().x < 4.0, "{pts:?}");
    }

    #[test]
    fn dpic_unit_suffix() {
        // `72bp__` == 72 * scale/72 == 1 inch
        let d = draw("box wid 72bp__ ht 0.3");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 1.0).abs() < 1e-9, "w = {w}");
    }

    #[test]
    fn block_sees_outer_labels() {
        // a label defined before a block is visible (read-only) inside it
        let d = draw("A: (0,0)\n[ line from A to (2,0) ]");
        assert!(
            d.shapes.iter().any(|s| matches!(s, Shape::Path { .. })),
            "block should draw a line referencing the outer label A"
        );
        // outer labels must not pollute the block's `last`/nth: a box drawn
        // before the block isn't the block's `last box`.
        assert!(eval(&parse("box\n[ circle; \"x\" at last box ]").unwrap()).is_err());
    }

    #[test]
    fn arg_count_macro() {
        // `$+` is the number of arguments passed to the current macro
        let d = draw("define cnt { $+ }\nx = cnt(a, b, c)\nbox wid x ht 0.3");
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 3.0).abs() < 1e-9, "w = {w}");
    }

    #[test]
    fn exec_evaluates_generated_pic_in_macro_arg_scope() {
        let d = draw(
            "define array { for i_array=2 to $+ do { exec sprintf(\"$1[%g] = $%g\", i_array-1, i_array) } }\narray(a, 0, 1, 3)\nbox wid a[2] ht a[3]",
        );
        let Shape::Box { w, h, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(
            (*w - 1.0).abs() < 1e-9 && (*h - 3.0).abs() < 1e-9,
            "{w} x {h}"
        );
    }

    #[test]
    fn exec_unescapes_generated_quoted_text() {
        let d = draw("exec sprintf(\"\\\"x\\\" at Here\")");
        let Shape::Text { text, .. } = &d.shapes[0] else {
            panic!()
        };
        assert_eq!(text[0].s, "x");
    }

    #[test]
    fn macro_token_pasting_concatenates_adjacent_args() {
        let d = draw("define mark { $1$2: (1,0) }\nmark(A,B)\nbox wid 0.2 ht 0.2 at AB");
        let Shape::Box { c, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(c.dist(Point::new(1.0, 0.0)) < 1e-9, "c = {c:?}");
    }

    #[test]
    fn macro_string_substitution_preserves_dot_prefixed_arguments() {
        let d = draw("define label { \"$1\"; \"$2\" }\nlabel(.ne,above)");
        let labels: Vec<&str> = d
            .shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text { text, .. } => Some(text[0].s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, [".ne", "above"]);
    }

    #[test]
    fn recursive_macro_terminates() {
        // a self-calling macro bounded by `if`: textual pre-expansion would
        // diverge, but lazy (eval-time) expansion of the taken branch stops it.
        let d = draw("define rec { if $1 <= 0 then { circle } else { box; rec($1-1) } }\nrec(3)");
        let boxes = d
            .shapes
            .iter()
            .filter(|s| matches!(s, Shape::Box { .. }))
            .count();
        let circles = d
            .shapes
            .iter()
            .filter(|s| matches!(s, Shape::Circle { .. }))
            .count();
        assert_eq!((boxes, circles), (3, 1), "shapes = {:?}", d.shapes.len());
    }

    #[test]
    fn default_argument_idiom() {
        // empty argument: the dead `else { w = $1 }` becomes `w =`, which must
        // not be parsed because the then-branch is taken.
        let d = draw(
            "define b { if \"$1\"==\"\" then { w = 1 } else { w = $1 }\n box wid w ht 0.2 }\nb()",
        );
        let Shape::Box { w, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 1.0).abs() < 1e-9, "w = {w}");
        // and with an argument supplied, the else-branch value is used
        let d2 = draw(
            "define b { if \"$1\"==\"\" then { w = 1 } else { w = $1 }\n box wid w ht 0.2 }\nb(2.5)",
        );
        let Shape::Box { w, .. } = &d2.shapes[0] else {
            panic!()
        };
        assert!((*w - 2.5).abs() < 1e-9, "w = {w}");
    }

    #[test]
    fn last_ordinal() {
        let d = draw("box at 0,0\nbox at 2,0\narrow from 1st box.e to 2nd box.w");
        let Shape::Path { pts, .. } = &d.shapes[2] else {
            panic!()
        };
        // from first box east edge to second box west edge
        assert!(pts[0].x > 0.0 && pts.last().unwrap().x < 2.0);
    }

    #[test]
    fn untyped_last_references_any_kind() {
        // `last.c` after a circle resolves to that circle (no `last circle`).
        let d = draw("circle rad 0.5 at (3,1)\n\"x\" at last.c");
        let Shape::Text { at, .. } = d.shapes.last().unwrap() else {
            panic!()
        };
        assert!((at.x - 3.0).abs() < 1e-9 && (at.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn untyped_last_corner_after_box() {
        // `last.n` (north of the most recent object, whatever its kind).
        let d = draw("box wid 2 ht 1 at (0,0)\n\"y\" at last.n");
        let Shape::Text { at, .. } = d.shapes.last().unwrap() else {
            panic!()
        };
        assert!((at.x - 0.0).abs() < 1e-9 && (at.y - 0.5).abs() < 1e-9);
    }

    #[test]
    fn untyped_nth_last_spans_kinds() {
        // `2nd last` counts across kinds: box, then circle -> 2nd last is the box.
        let d = draw("box at (0,0)\ncircle at (2,0)\n\"z\" at 2nd last.c");
        let Shape::Text { at, .. } = d.shapes.last().unwrap() else {
            panic!()
        };
        assert!((at.x - 0.0).abs() < 1e-9, "x = {}", at.x);
    }

    #[test]
    fn typed_last_still_filters_by_kind() {
        // an explicit type keyword keeps filtering: `last box` skips the circle.
        let d = draw("box at (0,0)\ncircle at (2,0)\n\"w\" at last box.c");
        let Shape::Text { at, .. } = d.shapes.last().unwrap() else {
            panic!()
        };
        assert!((at.x - 0.0).abs() < 1e-9, "x = {}", at.x);
    }

    #[test]
    fn untyped_last_with_no_object_errors() {
        assert!(eval(&parse("\"q\" at last.c").unwrap()).is_err());
    }
}
