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
//! turn; `rand()` is deterministic (0.5); unknown variables read as 0.

use std::collections::HashMap;
use std::f64::consts::PI;

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
    let mut d = Drawing {
        shapes: st.shapes,
        bbox: st.bbox,
        anims: st.anims,
    };
    apply_ps_size(&mut d, want_w, want_h);
    Ok(d)
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
    let mut bb = Bbox::new();
    bb.add(d.bbox.min * factor);
    bb.add(d.bbox.max * factor);
    d.bbox = bb;
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
            (Textoffset, 0.05),
            (Textwid, 0.0),
            (Arrowhead, 2.0),
            (Fillval, 0.5),
            (Linethick, -1.0),
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
    /// Index of the primary shape in `shapes` (None for point-only labels).
    shape: Option<usize>,
    /// For blocks: inner labels (sub-objects), translated into parent
    /// coordinates, so `B.A` / `last [].Outer` resolve. Empty otherwise.
    members: HashMap<String, Placed>,
}

impl Placed {
    fn corner(&self, c: Corner) -> Point {
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
}

// ---- evaluator state -------------------------------------------------------

struct State {
    pos: Point,
    dir: Dir,
    vars: HashMap<String, f64>,
    env: EnvVars,
    macros: Macros,
    base_dir: Option<std::path::PathBuf>,
    shapes: Vec<Shape>,
    placed: Vec<Placed>,
    labels: HashMap<String, usize>,
    bbox: Bbox,
    // animation state
    anims: Vec<Anim>,
    anim_cursor: f64,
    anim_end: HashMap<usize, f64>,
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
            env: EnvVars::new(),
            macros: HashMap::new(),
            base_dir: None,
            shapes: Vec::new(),
            placed: Vec::new(),
            labels: HashMap::new(),
            bbox: Bbox::new(),
            anims: Vec::new(),
            anim_cursor: 0.0,
            anim_end: HashMap::new(),
        }
    }

    fn eval_stmts(&mut self, stmts: &[Stmt]) -> ER<()> {
        for s in stmts {
            self.eval_stmt(s)?;
        }
        Ok(())
    }

    /// Parse a deferred `if`/`for` body now, expanding macros along this path.
    fn parse_body(&self, body: &Body) -> ER<Vec<Stmt>> {
        crate::parser::parse_body_tokens(body, &self.macros, self.base_dir.as_deref())
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
                    self.vars.insert(var.clone(), v);
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
                    self.eval_expr(e)?;
                }
                PrintItem::Str(_) => {}
            },
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

    fn eval_assignment(&mut self, a: &Assignment) -> ER<()> {
        let rhs = self.eval_expr(&a.value)?;
        match &a.target {
            AssignTarget::Var(name, subscript) => {
                let key = self.indexed_name(name, subscript.as_ref())?;
                let cur = *self.vars.get(&key).unwrap_or(&0.0);
                let val = apply_op(a.op, cur, rhs);
                self.vars.insert(key, val);
            }
            AssignTarget::Env(e) => {
                let cur = self.env.get(*e);
                let val = apply_op(a.op, cur, rhs);
                if matches!(e, EnvVar::Scale) {
                    // Changing `scale` rescales all scaled dimension variables by
                    // the ratio (dpic semantics), so default sizes follow.
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
        let deflen_h = self.env.get(EnvVar::Linewid);
        let deflen_v = self.env.get(EnvVar::Lineht);

        let mut pts = vec![start];
        let mut pend = Point::ZERO;
        let mut any = false;
        let mut last_dir = self.dir;
        for a in &obj.attrs {
            match a {
                Attr::Direction(d, opt) => {
                    let dist = match opt {
                        Some(e) => self.eval_expr(e)?,
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
                    let dist = self.eval_expr(e)?;
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
        match &mut self.shapes[idx] {
            Shape::Path { pts: p, .. } | Shape::Spline { pts: p, .. } => p.extend_from_slice(&new),
            _ => unreachable!(),
        }
        let end = *pts.last().unwrap();
        for q in &new {
            self.bbox.add(*q);
        }
        self.pos = end;
        self.dir = last_dir;

        let pidx = self.placed.iter().position(|pl| pl.shape == Some(idx));
        if let Some(pi) = pidx {
            self.placed[pi].end = end;
            self.placed[pi].bbox.add(end);
            self.placed[pi].center = (self.placed[pi].start + end) * 0.5;
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
                    .unwrap_or(self.env.get(EnvVar::Circlerad));
                let r = self.dim(obj, DimKind::Rad)?.unwrap_or(def_r);
                let r = self.dim(obj, DimKind::Diam)?.map(|d| d / 2.0).unwrap_or(r);
                w = 2.0 * r;
                h = 2.0 * r;
                rad = r;
            }
            Prim::Ellipse => {
                let (dw, dh) = prev.unwrap_or((
                    self.env.get(EnvVar::Ellipsewid),
                    self.env.get(EnvVar::Ellipseht),
                ));
                w = self.dim(obj, DimKind::Wid)?.unwrap_or(dw);
                h = self.dim(obj, DimKind::Ht)?.unwrap_or(dh);
                rad = 0.0;
            }
            _ => {
                let (dw, dh) =
                    prev.unwrap_or((self.env.get(EnvVar::Boxwid), self.env.get(EnvVar::Boxht)));
                w = self.dim(obj, DimKind::Wid)?.unwrap_or(dw);
                h = self.dim(obj, DimKind::Ht)?.unwrap_or(dh);
                rad = self
                    .dim(obj, DimKind::Rad)?
                    .unwrap_or(self.env.get(EnvVar::Boxrad));
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
        self.bbox.union(&bb);

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
        Ok(self.record(kind, center, bb, start, end, 0.0, Some(sh)))
    }

    fn open(&mut self, p: Prim, obj: &Object) -> ER<usize> {
        let style = self.style_of(obj)?;
        let text = self.text_of(obj)?;
        let is_move = matches!(p, Prim::Move);
        let (deflen_h, deflen_v) = if is_move {
            (self.env.get(EnvVar::Movewid), self.env.get(EnvVar::Moveht))
        } else {
            (self.env.get(EnvVar::Linewid), self.env.get(EnvVar::Lineht))
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
                        Some(e) => self.eval_expr(e)?,
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
                    let dist = self.eval_expr(e)?;
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
            // bare line/arrow/move in the current direction
            let dist = if horizontal(self.dir) {
                deflen_h
            } else {
                deflen_v
            };
            pts.push(start + dir_unit(self.dir) * dist);
            last_dir = self.dir;
        }

        // `chop`: trim each end (default circlerad) so connectors meet shapes cleanly
        if let Some(amt) = self.chop_of(obj)?
            && pts.len() >= 2
            && amt > 0.0
        {
            let n = pts.len();
            let d0 = pts[1] - pts[0];
            let l0 = d0.len();
            if l0 > amt {
                pts[0] = pts[0] + d0 / l0 * amt;
            }
            let d1 = pts[n - 2] - pts[n - 1];
            let l1 = d1.len();
            if l1 > amt {
                pts[n - 1] = pts[n - 1] + d1 / l1 * amt;
            }
        }

        // arrowheads
        let arrows = self.arrows_of(obj, matches!(p, Prim::Arrow));

        let mut bb = Bbox::new();
        for pt in &pts {
            bb.add(*pt);
        }
        self.bbox.union(&bb);

        let end = *pts.last().unwrap();
        let center = (pts[0] + end) * 0.5;
        self.union_text(center, &text);
        let kind = match p {
            Prim::Spline => PKind::Spline,
            Prim::Move => PKind::Move,
            _ => PKind::Line,
        };
        let mut style = style;
        if is_move {
            style.invis = true;
        }
        let shape = if matches!(p, Prim::Spline) {
            Shape::Spline {
                pts: pts.clone(),
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
        Ok(self.record(kind, center, bb, pts[0], end, 0.0, Some(sh)))
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
            // arc from `start` to `to`, optional radius (default: a 90° arc)
            let chord = end - start;
            let clen = chord.len();
            if clen < 1e-9 {
                return err("degenerate arc: `from` and `to` coincide");
            }
            let r = rad_attr
                .unwrap_or(clen / std::f64::consts::SQRT_2)
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
            let r = rad_attr.unwrap_or(self.env.get(EnvVar::Arcrad));
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
        self.bbox.union(&bb);
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
        Ok(self.record(PKind::Arc, center, bb, start, end, 0.0, Some(sh)))
    }

    fn text_obj(&mut self, obj: &Object) -> ER<usize> {
        let text = self.text_of(obj)?;
        let at = self.at_of(obj)?.unwrap_or(self.pos);
        let mut bb = Bbox::new();
        bb.add(at);
        self.bbox.union(&bb);
        self.union_text(at, &text);
        self.shapes.push(Shape::Text { at, text });
        let sh = self.shapes.len() - 1;
        Ok(self.record(PKind::Text, at, bb, at, at, 0.0, Some(sh)))
    }

    /// Union an estimated text extent (centered at `center`) into the drawing
    /// bbox, so wide labels and bare text objects aren't clipped by the SVG
    /// viewBox. Uses the same 11pt font the SVG backend renders with; the
    /// average glyph width is approximated (slightly generous to avoid clipping).
    fn union_text(&mut self, center: Point, lines: &[TextLine]) {
        if lines.is_empty() {
            return;
        }
        const EM: f64 = 11.0 / 72.0; // font height in inches (matches svg FONT_PT)
        let char_w = 0.6 * EM;
        let line_h = 1.2 * EM;
        let cols = lines.iter().map(|l| l.s.chars().count()).max().unwrap_or(0) as f64;
        let half = Point::new(cols * char_w / 2.0, lines.len() as f64 * line_h / 2.0);
        self.bbox.add(center + half);
        self.bbox.add(center - half);
    }

    fn block(&mut self, stmts: &[Stmt], obj: &Object) -> ER<usize> {
        // evaluate the block in a fresh local scope at its own origin
        let mut sub = State::new();
        sub.env = self.env.clone();
        sub.vars = self.vars.clone();
        sub.macros = self.macros.clone();
        sub.base_dir = self.base_dir.clone();
        sub.eval_stmts(stmts)?;

        let sub_bb = if sub.bbox.is_empty() {
            let mut b = Bbox::new();
            b.add(Point::ZERO);
            b
        } else {
            sub.bbox
        };
        let local_center = (sub_bb.min + sub_bb.max) * 0.5;
        let w = sub_bb.width();
        let h = sub_bb.height();

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
        bb.add(sub_bb.min + shift);
        bb.add(sub_bb.max + shift);
        self.bbox.union(&bb);

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
                return Ok(Some(self.eval_expr(e)?));
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

    /// The `chop` amount (explicit value, or `circlerad`), if the object has it.
    fn chop_of(&mut self, obj: &Object) -> ER<Option<f64>> {
        for a in &obj.attrs {
            if let Attr::Chop(opt) = a {
                let amt = match opt {
                    Some(e) => self.eval_expr(e)?,
                    None => self.env.get(EnvVar::Circlerad),
                };
                return Ok(Some(amt));
            }
        }
        Ok(None)
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
                    WithAnchor::Pair(x, y) => Point::new(self.eval_expr(x)?, self.eval_expr(y)?),
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
                    WithAnchor::Pair(x, y) => Point::new(self.eval_expr(x)?, self.eval_expr(y)?),
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
        let mut s = Style::default();
        for a in &obj.attrs {
            match a {
                Attr::LineStyle(lt, _) => match lt {
                    LineType::Solid => s.dash = Dash::Solid,
                    LineType::Dashed => s.dash = Dash::Dashed,
                    LineType::Dotted => s.dash = Dash::Dotted,
                    LineType::Invis => s.invis = true,
                },
                Attr::Fill(opt) => {
                    let g = match opt {
                        Some(e) => self.eval_expr(e)?,
                        None => self.env.get(EnvVar::Fillval),
                    };
                    s.fill = Some(Fill::Gray(g));
                }
                Attr::Color(kind, se) => {
                    let name = stringexpr_lit(se);
                    match kind {
                        token::Color::Outlined => s.stroke = Some(name),
                        token::Color::Colored => {
                            s.stroke = Some(name.clone());
                            s.fill = Some(Fill::Color(name));
                        }
                        token::Color::Shaded => s.fill = Some(Fill::Color(name)),
                    }
                }
                Attr::Dim(DimKind::Thick, e) => s.thick = Some(self.eval_expr(e)?),
                _ => {}
            }
        }
        Ok(s)
    }

    fn text_of(&mut self, obj: &Object) -> ER<Vec<TextLine>> {
        let mut lines = Vec::new();
        let mut halign = 0i8;
        let mut valign = 0i8;
        for a in &obj.attrs {
            match a {
                Attr::TextPos(tp) => match tp {
                    token::TextPos::Ljust => halign = -1,
                    token::TextPos::Rjust => halign = 1,
                    token::TextPos::Center => {
                        halign = 0;
                        valign = 0;
                    }
                    token::TextPos::Above => valign = 1,
                    token::TextPos::Below => valign = -1,
                },
                Attr::Text(se) => {
                    let s = self.eval_stringexpr(se)?;
                    lines.push(TextLine { s, halign, valign });
                }
                _ => {}
            }
        }
        Ok(lines)
    }

    // ---- positions & places ------------------------------------------------

    fn eval_pos(&mut self, pos: &Position) -> ER<Point> {
        match pos {
            Position::Pair(x, y) => Ok(Point::new(self.eval_expr(x)?, self.eval_expr(y)?)),
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
                let idx = self.label_index(&key)?;
                Ok(self.placed[idx].center)
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
                let idx = self.label_index(&key)?;
                Ok(self.placed[idx].clone())
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

    fn label_key(&mut self, label: &Label) -> ER<String> {
        self.indexed_name(&label.name, label.subscript.as_ref())
    }

    fn indexed_name(&mut self, name: &str, subscript: Option<&Expr>) -> ER<String> {
        match subscript {
            Some(e) => Ok(format!("{name}[{}]", fmt_num(self.eval_expr(e)?))),
            None => Ok(name.to_string()),
        }
    }

    fn nth_index(&mut self, count: &Nth, obj: &PrimObj) -> ER<usize> {
        let want = primobj_kind(obj);
        let matches: Vec<usize> = self
            .placed
            .iter()
            .enumerate()
            .filter(|(_, pl)| pl.kind == want)
            .map(|(i, _)| i)
            .collect();
        if matches.is_empty() {
            return err(format!("no {:?} object to reference", want_name(want)));
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
            StringExpr::SvgFont(args) => {
                for e in args {
                    self.eval_printf_arg(e)?;
                }
                String::new()
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
        Ok(match e {
            Expr::Num(v) => *v,
            Expr::Str(_) => return err("a string is only valid as an `==`/`!=` operand"),
            Expr::Var(name, subscript) => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                *self.vars.get(&key).unwrap_or(&0.0)
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
                    BinOp::Mod => {
                        if y == 0.0 {
                            return err("modulo by zero");
                        }
                        x % y
                    }
                    BinOp::Pow => x.powf(y),
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
                        if x > 0.0 {
                            1.0
                        } else if x < 0.0 {
                            -1.0
                        } else {
                            0.0
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
            Expr::Rand(_) => 0.5, // deterministic placeholder
            Expr::Assign(name, subscript, v) => {
                let val = self.eval_expr(v)?;
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.vars.insert(key, val);
                val
            }
            Expr::DotX(loc) => self.eval_loc(loc)?.x,
            Expr::DotY(loc) => self.eval_loc(loc)?.y,
            Expr::PlaceAttr(place, param) => {
                let pl = self.resolve_obj(place)?;
                match param {
                    token::Param::Width => pl.bbox.width(),
                    token::Param::Height => pl.bbox.height(),
                    token::Param::Radius => pl.bbox.width() / 2.0,
                    token::Param::Diameter => pl.bbox.width(),
                    token::Param::Length => pl.start.dist(pl.end),
                    token::Param::Thickness => pl.thick,
                }
            }
        })
    }
}

// ---- free helpers ----------------------------------------------------------

fn apply_op(op: AssignOp, cur: f64, rhs: f64) -> f64 {
    match op {
        AssignOp::Set => rhs,
        AssignOp::Add => cur + rhs,
        AssignOp::Sub => cur - rhs,
        AssignOp::Mul => cur * rhs,
        AssignOp::Div => cur / rhs,
        AssignOp::Rem => cur % rhs,
    }
}

fn bool_f(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
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

fn primobj_kind(o: &PrimObj) -> PKind {
    match o {
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
    }
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

/// Shift a [`Placed`] (and its block members) by `d` and re-index its shape
/// references by `shape_off`, mapping a block's local records into the parent.
fn rebase_placed(pl: &mut Placed, d: Point, shape_off: usize) {
    pl.center = pl.center + d;
    pl.start = pl.start + d;
    pl.end = pl.end + d;
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

/// Uniformly scale a shape's geometry about the origin (font size unchanged).
fn scale_shape(sh: &mut Shape, f: f64) {
    match sh {
        Shape::Box { c, w, h, rad, .. } => {
            *c = *c * f;
            *w *= f;
            *h *= f;
            *rad *= f;
        }
        Shape::Circle { c, r, .. } => {
            *c = *c * f;
            *r *= f;
        }
        Shape::Ellipse { c, w, h, .. } => {
            *c = *c * f;
            *w *= f;
            *h *= f;
        }
        Shape::Path { pts, .. } | Shape::Spline { pts, .. } => {
            for p in pts {
                *p = *p * f;
            }
        }
        Shape::Arc { c, r, .. } => {
            *c = *c * f;
            *r *= f;
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
        assert!((d.bbox.height() - 0.5).abs() < 1e-9);
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
    fn for_loop_repeats() {
        let d = draw("for i = 1 to 3 do { box }");
        assert_eq!(d.shapes.len(), 3);
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
    fn subscripted_variables_store_by_index() {
        let d = draw("P[1] = 0.4\nP[2] = 0.9\nP[2] += 0.1\nbox wid P[2] ht P[1]");
        let Shape::Box { w, h, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!((*w - 1.0).abs() < 1e-9 && (*h - 0.4).abs() < 1e-9);
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
        let s = *c + Point::new(a0.cos(), a0.sin()) * *r;
        let e = *c + Point::new(a1.cos(), a1.sin()) * *r;
        assert!(s.dist(Point::new(0.0, 0.0)) < 1e-9, "start {s:?}");
        assert!(e.dist(Point::new(1.0, 1.0)) < 1e-9, "end {e:?}");
        // explicit radius is honored
        let d2 = draw("A:(0,0)\nB:(1,0)\narc from A to B rad 2");
        let Shape::Arc { r, .. } = &d2.shapes[0] else {
            panic!()
        };
        assert!((*r - 2.0).abs() < 1e-9, "r = {r}");
    }

    #[test]
    fn scale_rescales_default_dims() {
        // issue #4: `scale = 2` doubles the default box size
        let d = draw("scale = 2\nbox");
        let Shape::Box { w, h, .. } = &d.shapes[0] else {
            panic!()
        };
        assert!(
            (*w - 1.5).abs() < 1e-9 && (*h - 1.0).abs() < 1e-9,
            "{w} x {h}"
        );
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
    }

    #[test]
    fn ps_width_scales_drawing() {
        // issue #4: `.PS 6` scales the whole picture to 6 units wide
        let d = draw(".PS 6\nbox\n.PE");
        assert!(
            (d.bbox.width() - 6.0).abs() < 1e-6,
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
    fn recursive_macro_terminates() {
        // a self-calling macro bounded by `if`: textual pre-expansion would
        // diverge, but lazy (eval-time) expansion of the taken branch stops it.
        let d = draw(
            "define rec { if $1 <= 0 then { circle } else { box; rec($1-1) } }\nrec(3)",
        );
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
}
