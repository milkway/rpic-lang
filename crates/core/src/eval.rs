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
use crate::geom::{Bbox, Point};
use crate::ir::*;
use crate::token::{self, Corner, Dir, EnvVar, Func1, Func2, LineType, Prim};

/// An evaluation error.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalError {
    pub msg: String,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

type ER<T> = Result<T, EvalError>;

fn err<T>(msg: impl Into<String>) -> ER<T> {
    Err(EvalError { msg: msg.into() })
}

/// Evaluate a parsed picture into a [`Drawing`].
pub fn eval(pic: &Picture) -> ER<Drawing> {
    let mut st = State::new();
    st.macros = pic.macros.clone();
    st.base_dir = pic.base_dir.clone();
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
    let mut d = Drawing {
        shapes: st.shapes,
        bbox: st.bbox,
        anims: st.anims,
        diagnostics: st.diagnostics,
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
        let (w, h) = (d.bbox.width(), d.bbox.height());
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
        d.bbox = drawing_painted_bbox(&d.shapes);
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
    d.bbox = drawing_painted_bbox(&d.shapes);
}

/// The dimension variables that track `scale`.
const SCALED_VARS: [EnvVar; 17] = [
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
            (Textht, 0.0),
            (Textoffset, 2.0 / 72.0),
            (Textwid, 0.0),
            (Arrowhead, 1.0),
            (Fillval, 0.5),
            (Linethick, 0.8),
            (Maxpsht, 11.0),
            (Maxpswid, 8.5),
            (Scale, 1.0),
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
    /// Index of the primary shape in `shapes` (None for point-only labels).
    shape: Option<usize>,
    /// For blocks: inner labels (sub-objects), translated into parent
    /// coordinates, so `B.A` / `last [].Outer` resolve. Empty otherwise.
    members: HashMap<String, Placed>,
}

impl Placed {
    fn corner(&self, c: Corner) -> Point {
        match self.kind {
            PKind::Circle | PKind::Ellipse => self.ellipse_corner(c),
            PKind::Line | PKind::Move | PKind::Spline => self.linear_corner(c),
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
            PKind::Line | PKind::Move | PKind::Spline => self.line_wid,
            _ => self.bbox.width(),
        }
    }

    fn attr_height(&self) -> f64 {
        match self.kind {
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
            PKind::Line | PKind::Move | PKind::Spline => self.start.dist(self.end),
            _ => 0.0,
        }
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
    base_dir: Option<std::path::PathBuf>,
    /// Labels visible from an enclosing scope (read-only, in absolute parent
    /// coordinates). A block may reference outer labels but must not let them
    /// affect its own `last`/nth/bbox, so they live here, not in `placed`.
    outer_labels: HashMap<String, Placed>,
    shapes: Vec<Shape>,
    placed: Vec<Placed>,
    labels: HashMap<String, usize>,
    /// Visible geometry and text only; this becomes the final drawing/viewBox.
    bbox: Bbox,
    /// All evaluated geometry, including invisible helpers, for block sizing
    /// and anchor placement.
    layout_bbox: Bbox,
    // animation state
    anims: Vec<Anim>,
    diagnostics: Vec<String>,
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
            base_dir: None,
            outer_labels: HashMap::new(),
            shapes: Vec::new(),
            placed: Vec::new(),
            labels: HashMap::new(),
            bbox: Bbox::new(),
            layout_bbox: Bbox::new(),
            anims: Vec::new(),
            diagnostics: Vec::new(),
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
        crate::parser::parse_body_tokens(body, &mut self.macros, self.base_dir.as_deref())
            .map_err(|e| EvalError { msg: e.to_string() })
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
                    shape: None,
                    members: HashMap::new(),
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
                let body = self.parse_body(body)?;
                let from = self.eval_expr(from)?;
                let to = self.eval_expr(to)?;
                let by = self.eval_expr(by)?;
                let mut v = from;
                let mut iters = 0u64;
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
                    self.eval_stmts(&body)?;
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
                    self.base_dir.as_deref(),
                    arg_frame.as_deref(),
                )
                .map_err(|e| EvalError { msg: e.to_string() })?;
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
                })?;
                *self.anim_end.get(&sh).unwrap_or(&0.0)
            }
        };
        if let Some(d) = &a.delay {
            start += self.eval_expr(d)?;
        }
        let end = start + dur;
        self.anim_cursor = end;
        self.anim_end.insert(shape, end);
        self.anims.push(Anim {
            shape,
            effect: stringexpr_lit(&a.effect),
            start,
            duration: dur,
        });
        Ok(())
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
        match &obj.kind {
            ObjectKind::Primitive(p) => match p {
                Prim::Box | Prim::Circle | Prim::Ellipse => self.closed(*p, obj),
                Prim::Line | Prim::Arrow | Prim::Move | Prim::Spline => self.open(*p, obj),
                Prim::Arc => self.arc(obj),
            },
            ObjectKind::Text => self.text_obj(obj),
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
            })?;
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
                Attr::Dist(e) => {
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

        let pidx = self.placed.iter().position(|pl| pl.shape == Some(idx));
        if let Some(pi) = pidx {
            self.placed[pi].end = end;
            self.placed[pi].bbox.add(end);
            self.placed[pi].center = (self.placed[pi].start + end) * 0.5;
            self.placed[pi].points.extend_from_slice(&new);
        }
        Ok(pidx.unwrap_or(idx))
    }

    fn closed(&mut self, p: Prim, obj: &Object) -> ER<usize> {
        let style = self.style_of(obj)?;
        let text = self.text_of(obj)?;
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
                let def_r = prev
                    .map(|(pw, _)| pw / 2.0)
                    .unwrap_or(self.env_dim(EnvVar::Circlerad)?);
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
        w *= scale;
        h *= scale;
        rad *= scale;

        let extent = if horizontal(dir) { w } else { h };
        let center = self.place_center(obj, dir, extent, w, h)?;

        let mut bb = Bbox::new();
        bb.add(center - Point::new(w / 2.0, h / 2.0));
        bb.add(center + Point::new(w / 2.0, h / 2.0));
        self.layout_bbox.union(&bb);
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
        self.shapes.push(shape);

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

    fn open(&mut self, p: Prim, obj: &Object) -> ER<usize> {
        let mut style = self.style_of(obj)?;
        let text = self.text_of(obj)?;
        let line_wid = style.arrow_wid;
        let line_ht = style.arrow_ht;
        let is_move = matches!(p, Prim::Move);
        if is_move {
            style.invis = true;
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
                    let d = self.eval_pos(pos)?;
                    pend = pend + (d - Point::ZERO);
                    any = true;
                }
                Attr::Dist(e) => {
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
                if start_chop < 0.0 || l0 > start_chop {
                    pts[0] = pts[0] + d0 / l0 * start_chop;
                }
            }
            if end_chop != 0.0 {
                let d1 = pts[n - 2] - pts[n - 1];
                let l1 = d1.len();
                if end_chop < 0.0 || l1 > end_chop {
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
        }

        let end = *pts.last().unwrap();
        let center = (pts[0] + end) * 0.5;
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
                arrows,
                style,
                text,
            }
        };
        self.shapes.push(shape);

        self.pos = end;
        self.dir = last_dir;
        let sh = self.shapes.len() - 1;
        let idx = self.record(kind, center, bb, pts[0], end, 0.0, Some(sh));
        self.placed[idx].points = pts;
        self.placed[idx].line_wid = line_wid;
        self.placed[idx].line_ht = line_ht;
        Ok(idx)
    }

    fn arc(&mut self, obj: &Object) -> ER<usize> {
        let style = self.style_of(obj)?;
        let text = self.text_of(obj)?;
        let start = self.find_from(obj)?.unwrap_or(self.pos);
        let cw = obj.attrs.iter().any(|a| matches!(a, Attr::Cw));
        let rad_attr = self.dim(obj, DimKind::Rad)?;
        let to = self.dest_of(obj)?;

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
            let hd = (r * r - (clen / 2.0) * (clen / 2.0)).max(0.0).sqrt();
            let u = chord / clen;
            let perp = Point::new(-u.y, u.x);
            let mid = (start + end) * 0.5;
            let center = if cw { mid - perp * hd } else { mid + perp * hd };
            let a0 = (start - center).y.atan2((start - center).x);
            let mut a1 = (end - center).y.atan2((end - center).x);
            // keep the requested handedness (ccw: a1 > a0, cw: a1 < a0)
            if cw {
                while a1 > a0 {
                    a1 -= 2.0 * PI;
                }
            } else {
                while a1 < a0 {
                    a1 += 2.0 * PI;
                }
            }
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

        self.shapes.push(Shape::Arc {
            c: center,
            r,
            a0,
            a1,
            cw,
            arrows,
            style,
            text,
        });

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

    fn text_obj(&mut self, obj: &Object) -> ER<usize> {
        let text = self.text_of(obj)?;
        let dir = self.dir_of(obj);
        let w = self.env_dim(EnvVar::Textwid)?;
        let h = self.env_dim(EnvVar::Textht)? * text.len().max(1) as f64;
        let extent = if horizontal(dir) { w } else { h };
        let at = self.place_center(obj, dir, extent, w, h)?;
        let mut bb = Bbox::new();
        bb.add(at - Point::new(w / 2.0, h / 2.0));
        bb.add(at + Point::new(w / 2.0, h / 2.0));
        self.layout_bbox.union(&bb);
        self.union_text(at, &text);
        self.shapes.push(Shape::Text { at, text });
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
        self.layout_bbox.union(&bb);
        self.bbox.union(&bb);
    }

    fn block(&mut self, stmts: &[Stmt], obj: &Object) -> ER<usize> {
        let block_text = self.text_of(obj)?;
        // Evaluate the block in a local scope at its own origin. Labels from
        // the containing scope are visible for references such as `$1.start`
        // inside macro-generated blocks, but are not captured as new members.
        let mut sub = State::new();
        sub.env = self.env.clone();
        sub.vars = self.vars.clone();
        sub.inherited_vars = self.vars.keys().cloned().collect();
        sub.export_vars.clear();
        sub.macros = self.macros.clone();
        sub.base_dir = self.base_dir.clone();
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
        for mut sh in sub.shapes.into_iter() {
            translate_shape(&mut sh, shift);
            self.shapes.push(sh);
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
        self.union_text(target, &block_text);
        if has_visible_text(&block_text) {
            self.shapes.push(Shape::Text {
                at: target,
                text: block_text,
            });
        }

        let half = dir_unit(dir) * (extent / 2.0);
        let start = target - half;
        let end = target + half;
        self.pos = end;
        self.dir = dir;
        let idx = self.record(PKind::Block, target, bb, start, end, 0.0, shape);
        self.placed[idx].members = members;
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
            shape,
            members: HashMap::new(),
        });
        idx
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
        if let Some(at) = self.at_of(obj)? {
            return Ok(at);
        }
        for a in &obj.attrs {
            if let Attr::With { anchor, at } = a {
                let ap = self.eval_pos(at)?;
                let off = match anchor {
                    WithAnchor::Corner(c) => corner_offset(*c, w, h),
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
                    WithAnchor::Corner(c) => corner_offset(*c, w, h),
                    WithAnchor::Pair(x, y) => Point::new(self.expr_dim(x)?, self.expr_dim(y)?),
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
                    let name = stringexpr_lit(se);
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
                Attr::Dim(DimKind::Thick, e) => s.thick = Some(self.eval_expr(e)?),
                Attr::Arrowhead(_, Some(e)) => {
                    s.arrow_filled = self.eval_expr(e)?.round() as i64 != 0;
                }
                _ => {}
            }
        }
        Ok(s)
    }

    fn text_of(&mut self, obj: &Object) -> ER<Vec<TextLine>> {
        let mut lines: Vec<TextLine> = Vec::new();
        let mut pending_halign = 0i8;
        let mut pending_valign = 0i8;
        for a in &obj.attrs {
            match a {
                Attr::TextPos(tp) => {
                    if let Some(line) = lines.last_mut() {
                        apply_text_pos(&mut line.halign, &mut line.valign, *tp);
                    } else {
                        apply_text_pos(&mut pending_halign, &mut pending_valign, *tp);
                    }
                }
                Attr::Text(se) => {
                    let s = self.eval_stringexpr(se)?;
                    lines.push(TextLine {
                        s,
                        halign: pending_halign,
                        valign: pending_valign,
                        text_offset: self.env_dim(EnvVar::Textoffset)?,
                    });
                    pending_halign = 0;
                    pending_valign = 0;
                }
                _ => {}
            }
        }
        Ok(lines)
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
            Place::Name { name, subscript } => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                Ok(self.resolve_label(&key)?.center)
            }
            Place::Nth { count, obj } => {
                let idx = self.nth_index(count, obj)?;
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
            Place::Name { name, subscript } => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.resolve_label(&key)
            }
            Place::Nth { count, obj } => {
                let idx = self.nth_index(count, obj)?;
                Ok(self.placed[idx].clone())
            }
            Place::Corner(inner, _) | Place::CornerOf(_, inner) => self.resolve_obj(inner),
            Place::Member(base, sub) => {
                let b = self.resolve_obj(base)?;
                let key = match sub.as_ref() {
                    Place::Name { name, subscript } => {
                        self.indexed_name(name, subscript.as_deref())?
                    }
                    _ => return err("a block sub-label must be a name"),
                };
                b.members.get(&key).cloned().ok_or_else(|| EvalError {
                    msg: format!("no sub-label `{key}` in that block"),
                })
            }
            Place::Here => err("`Here` is a point, not an object"),
        }
    }

    fn place_index(&mut self, p: &Place) -> ER<usize> {
        match p {
            Place::Name { name, subscript } => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.label_index(&key)
            }
            Place::Nth { count, obj } => self.nth_index(count, obj),
            Place::Corner(inner, _) | Place::CornerOf(_, inner) => self.place_index(inner),
            Place::Here => err("`Here` is a point, not an object"),
            Place::Member(_, _) => err("block sub-labels (B.A) are not supported yet"),
        }
    }

    fn label_index(&self, name: &str) -> ER<usize> {
        self.labels.get(name).copied().ok_or_else(|| EvalError {
            msg: format!("unknown label `{name}`"),
        })
    }

    /// Resolve a label to its [`Placed`], falling back to enclosing-scope labels
    /// (absolute coordinates) so a block can reference outer labels.
    fn resolve_label(&self, key: &str) -> ER<Placed> {
        if let Some(&idx) = self.labels.get(key) {
            Ok(self.placed[idx].clone())
        } else if let Some(pl) = self.outer_labels.get(key) {
            Ok(pl.clone())
        } else {
            err(format!("unknown label `{key}`"))
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

    fn nth_index(&mut self, count: &Nth, obj: &PrimObj) -> ER<usize> {
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
                        return err("ordinal out of range");
                    }
                    matches[matches.len() - 1 - k]
                } else {
                    if k >= matches.len() {
                        return err("ordinal out of range");
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
        Ok(match e {
            Expr::Num(v) => *v,
            Expr::Str(_) => return err("a string is only valid as an `==`/`!=` operand"),
            Expr::Index(_) => return err("a comma subscript is only valid inside `name[...]`"),
            Expr::Var(name, subscript) => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.vars.get(&key).copied().ok_or_else(|| EvalError {
                    msg: format!("variable not found `{key}`"),
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
        })
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
    Ok(match op {
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
    })
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

fn has_visible_text(lines: &[TextLine]) -> bool {
    lines.iter().any(|line| !line.s.is_empty())
}

fn closed_shape_is_visible(style: &Style) -> bool {
    !style.invis || style.fill.is_some()
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
            pts, style, text, ..
        }
        | Shape::Spline {
            pts, style, text, ..
        } => {
            let mut bb = Bbox::new();
            for p in pts {
                bb.add(*p);
            }
            if !style.invis {
                out.union(&painted_bbox(&bb, stroke_half_width(style)));
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
            if !style.invis {
                out.union(&painted_bbox(&bb, stroke_half_width(style)));
            }
            out.union(&text_bbox(*c, text));
        }
        Shape::Text { at, text } => out.union(&text_bbox(*at, text)),
    }
    out
}

fn text_bbox(center: Point, lines: &[TextLine]) -> Bbox {
    let mut bb = Bbox::new();
    if !has_visible_text(lines) {
        return bb;
    }
    const EM: f64 = 11.0 / 72.0;
    let char_w = 0.6 * EM;
    let line_h = 1.2 * EM;
    let xheight = 0.66 * EM;
    let n = lines.len() as f64;
    for (i, line) in lines.iter().enumerate() {
        if line.s.is_empty() {
            continue;
        }
        let w = line.s.chars().count() as f64 * char_w;
        let base_y = center.y - (i as f64 - (n - 1.0) / 2.0) * line_h;
        let y = base_y + line.valign as f64 * (xheight / 2.0 + line.text_offset);
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
        bb.add(Point::new(min_x, y - line_h / 2.0));
        bb.add(Point::new(max_x, y + line_h / 2.0));
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
        Shape::Text { at, .. } => mv(at),
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
    match &mut style.dash {
        Dash::Dashed(w) => *w *= f,
        Dash::Dotted(Some(w)) => *w *= f,
        Dash::Solid | Dash::Dotted(None) => {}
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
        Shape::Text { at, .. } => *at = *at * f,
    }
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
    fn invisible_geometry_and_moves_do_not_expand_drawing_bbox() {
        let d =
            draw("box invis wid 1000 ht 1000 at (0,0)\nmove to (0,-1000)\nbox wid 1 ht 1 at (0,0)");
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
    fn division_by_zero_errors() {
        // a zero divisor must error rather than silently produce NaN coordinates
        assert!(eval(&parse("box wid 1/0").unwrap()).is_err());
        assert!(eval(&parse("A:(0,0)\nB:(0,0)\nx = (B.x-A.x)/(B.y-A.y)").unwrap()).is_err());
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
        assert!((scalar("arrowhead").unwrap() - 1.0).abs() < 1e-9);
        assert!((scalar("linethick").unwrap() - 0.8).abs() < 1e-9);
    }

    #[test]
    fn print_statements_collect_diagnostics() {
        let d = draw("print 5.5\nprint 5.5%2\nprint \"hello\"\nprint sprintf(\"x=%g\", 1.25)");
        assert_eq!(d.diagnostics, ["5.5", "0", "hello", "x=1.25"]);

        let d = draw("[ print \"inside\"; box ]\nprint 7");
        assert_eq!(d.diagnostics, ["inside", "7"]);
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
