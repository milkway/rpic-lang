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
    st.eval_stmts(&pic.stmts)?;
    Ok(Drawing {
        shapes: st.shapes,
        bbox: st.bbox,
        anims: st.anims,
    })
}

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
        State {
            pos: Point::ZERO,
            dir: Dir::Right,
            vars: HashMap::new(),
            env: EnvVars::new(),
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
                });
                self.labels.insert(label.name.clone(), idx);
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
                    self.labels.insert(l.name.clone(), idx);
                }
            }
            Stmt::Animate(a) => self.eval_animate(a)?,
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                if self.eval_expr(cond)? != 0.0 {
                    self.eval_stmts(then_body)?;
                } else if let Some(e) = else_body {
                    self.eval_stmts(e)?;
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
                    self.eval_stmts(body)?;
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
            AssignTarget::Var(name, _sub) => {
                let cur = *self.vars.get(name).unwrap_or(&0.0);
                let val = apply_op(a.op, cur, rhs);
                self.vars.insert(name.clone(), val);
            }
            AssignTarget::Env(e) => {
                let cur = self.env.get(*e);
                self.env.set(*e, apply_op(a.op, cur, rhs));
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
        }
    }

    fn closed(&mut self, p: Prim, obj: &Object) -> ER<usize> {
        let style = self.style_of(obj)?;
        let text = self.text_of(obj)?;
        let dir = self.dir_of(obj);
        let scale = self.scale_of(obj)?;

        // dimensions
        let (mut w, mut h, mut rad);
        match p {
            Prim::Circle => {
                let r = self
                    .dim(obj, DimKind::Rad)?
                    .unwrap_or(self.env.get(EnvVar::Circlerad));
                let r = self.dim(obj, DimKind::Diam)?.map(|d| d / 2.0).unwrap_or(r);
                w = 2.0 * r;
                h = 2.0 * r;
                rad = r;
            }
            Prim::Ellipse => {
                w = self
                    .dim(obj, DimKind::Wid)?
                    .unwrap_or(self.env.get(EnvVar::Ellipsewid));
                h = self
                    .dim(obj, DimKind::Ht)?
                    .unwrap_or(self.env.get(EnvVar::Ellipseht));
                rad = 0.0;
            }
            _ => {
                // box
                w = self
                    .dim(obj, DimKind::Wid)?
                    .unwrap_or(self.env.get(EnvVar::Boxwid));
                h = self
                    .dim(obj, DimKind::Ht)?
                    .unwrap_or(self.env.get(EnvVar::Boxht));
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
        let start = self.from_of(obj)?.unwrap_or(self.pos);
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

        // arrowheads
        let arrows = self.arrows_of(obj, matches!(p, Prim::Arrow));

        let mut bb = Bbox::new();
        for pt in &pts {
            bb.add(*pt);
        }
        self.bbox.union(&bb);

        let end = *pts.last().unwrap();
        let center = (pts[0] + end) * 0.5;
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
        let start = self.from_of(obj)?.unwrap_or(self.pos);
        let r = self
            .dim(obj, DimKind::Rad)?
            .unwrap_or(self.env.get(EnvVar::Arcrad));
        let cw = obj.attrs.iter().any(|a| matches!(a, Attr::Cw));
        let din = dir_unit(self.dir);
        // center sits a radius to the left (ccw) or right (cw) of the heading
        let normal = if cw {
            Point::new(din.y, -din.x)
        } else {
            Point::new(-din.y, din.x)
        };
        let center = start + normal * r;
        let a0 = (start - center).y.atan2((start - center).x);
        let sweep = if cw { -PI / 2.0 } else { PI / 2.0 };
        let a1 = a0 + sweep;
        let end = center + Point::new(a1.cos(), a1.sin()) * r;

        let arrows = self.arrows_of(obj, false);

        let mut bb = Bbox::new();
        bb.add(start);
        bb.add(end);
        for k in 0..=8 {
            let t = a0 + sweep * (k as f64 / 8.0);
            bb.add(center + Point::new(t.cos(), t.sin()) * r);
        }
        self.bbox.union(&bb);

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

        // new heading is rotated by the sweep
        let new = din.rotate(sweep);
        self.dir = nearest_dir(new);
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
        self.shapes.push(Shape::Text { at, text });
        let sh = self.shapes.len() - 1;
        Ok(self.record(PKind::Text, at, bb, at, at, 0.0, Some(sh)))
    }

    fn block(&mut self, stmts: &[Stmt], obj: &Object) -> ER<usize> {
        // evaluate the block in a fresh local scope at its own origin
        let mut sub = State::new();
        sub.env = self.env.clone();
        sub.vars = self.vars.clone();
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
        let target = self.place_center(obj, dir, extent, w, h)?;
        let shift = target - local_center;

        let first_shape = self.shapes.len();
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
        Ok(self.record(PKind::Block, target, bb, start, end, 0.0, shape))
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
        });
        idx
    }

    // ---- attribute helpers -------------------------------------------------

    fn dim(&mut self, obj: &Object, kind: DimKind) -> ER<Option<f64>> {
        for a in &obj.attrs {
            if let Attr::Dim(k, e) = a {
                if *k == kind {
                    return Ok(Some(self.eval_expr(e)?));
                }
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

    fn from_of(&mut self, obj: &Object) -> ER<Option<Point>> {
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
            Position::Place(loc, shifts) => {
                let mut p = self.eval_loc(loc)?;
                for s in shifts {
                    let d = self.eval_loc(&s.loc)?;
                    p = match s.sign {
                        Sign::Plus => p + d,
                        Sign::Minus => p - d,
                    };
                }
                Ok(p)
            }
            Position::Between { frac, a, b, .. } => {
                let f = self.eval_expr(frac)?;
                let pa = self.eval_pos(a)?;
                let pb = self.eval_pos(b)?;
                Ok(pa.lerp(pb, f))
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
            Place::Name { name, .. } => {
                let idx = self.label_index(name)?;
                Ok(self.placed[idx].center)
            }
            Place::Nth { count, obj } => {
                let idx = self.nth_index(count, obj)?;
                Ok(self.placed[idx].center)
            }
            Place::Corner(inner, c) => {
                let idx = self.place_index(inner)?;
                Ok(self.placed[idx].corner(*c))
            }
            Place::CornerOf(c, inner) => {
                let idx = self.place_index(inner)?;
                Ok(self.placed[idx].corner(*c))
            }
            Place::Member(_, _) => err("block sub-labels (B.A) are not supported yet"),
        }
    }

    fn place_index(&mut self, p: &Place) -> ER<usize> {
        match p {
            Place::Name { name, .. } => self.label_index(name),
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
                    vals.push(self.eval_expr(e)?);
                }
                sprintf_fmt(&f, &vals)
            }
        })
    }

    // ---- expressions -------------------------------------------------------

    fn eval_expr(&mut self, e: &Expr) -> ER<f64> {
        Ok(match e {
            Expr::Num(v) => *v,
            Expr::Var(name) => *self.vars.get(name).unwrap_or(&0.0),
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
                let x = self.eval_expr(a)?;
                let y = self.eval_expr(b)?;
                match op {
                    BinOp::Add => x + y,
                    BinOp::Sub => x - y,
                    BinOp::Mul => x * y,
                    BinOp::Div => x / y,
                    BinOp::Mod => x % y,
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
            Expr::DotX(loc) => self.eval_loc(loc)?.x,
            Expr::DotY(loc) => self.eval_loc(loc)?.y,
            Expr::PlaceAttr(place, param) => {
                let idx = self.place_index(place)?;
                let pl = &self.placed[idx];
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

/// Minimal printf-style formatter supporting `%d %i %f %e %g %%` with optional
/// `.precision`. Width/flags are accepted but ignored; arguments are numeric.
fn sprintf_fmt(fmt: &str, vals: &[f64]) -> String {
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
        let v = vals.get(ai).copied().unwrap_or(0.0);
        ai += 1;
        let prec = spec.split('.').nth(1).and_then(|p| {
            p.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<usize>()
                .ok()
        });
        match conv {
            'd' | 'i' => out.push_str(&format!("{}", v.round() as i64)),
            'f' | 'F' => out.push_str(&format!("{:.*}", prec.unwrap_or(6), v)),
            'e' | 'E' => out.push_str(&format!("{:.*e}", prec.unwrap_or(6), v)),
            'g' | 'G' => out.push_str(&format!("{v}")),
            's' => out.push_str(&format!("{v}")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

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
    fn last_ordinal() {
        let d = draw("box at 0,0\nbox at 2,0\narrow from 1st box.e to 2nd box.w");
        let Shape::Path { pts, .. } = &d.shapes[2] else {
            panic!()
        };
        // from first box east edge to second box west edge
        assert!(pts[0].x > 0.0 && pts.last().unwrap().x < 2.0);
    }
}
