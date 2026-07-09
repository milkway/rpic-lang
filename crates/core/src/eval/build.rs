//! `State` methods that build placed primitives — closed shapes,
//! lines/splines, arcs, text, blocks and braces. Split out of the eval
//! core to keep the module readable (#323).

use super::*;

impl State {
    /// `continue`: append another segment to the most recent line/spline,
    /// extending it from its current end in the current (or given) direction.
    pub(super) fn continue_obj(&mut self, obj: &Object) -> ER<usize> {
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
    pub(super) fn closed_dot(&mut self, obj: &Object) -> ER<usize> {
        self.closed_impl(Prim::Circle, obj, true)
    }

    pub(super) fn closed(&mut self, p: Prim, obj: &Object) -> ER<usize> {
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
        self.push_shape(shape, layer)?;

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

    pub(super) fn brace(&mut self, obj: &Object) -> ER<usize> {
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
        self.push_shape(shape, layer)?;

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

    pub(super) fn open(&mut self, p: Prim, obj: &Object) -> ER<usize> {
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
        self.push_shape(shape, layer)?;

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

    pub(super) fn arc(&mut self, obj: &Object) -> ER<usize> {
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
        )?;

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

    pub(super) fn text_obj(&mut self, obj: &Object) -> ER<usize> {
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
        )?;
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

    pub(super) fn block(&mut self, stmts: &[Stmt], obj: &Object) -> ER<usize> {
        let block_text = self.text_of(obj)?;
        let block_fill_opacity = self.style_of(obj)?.fill_opacity;
        // Evaluate the block in a local scope at its own origin. Labels from
        // the containing scope are visible for references such as `$1.start`
        // inside macro-generated blocks, but are not captured as new members.
        let mut sub = State::with_limits(self.limits);
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
            self.push_shape(sh, layer + layer_shift)?;
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
            )?;
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

    fn push_shape(&mut self, shape: Shape, layer: i32) -> ER<()> {
        if self.shapes.len() >= self.limits.max_shapes {
            return err(format!(
                "drawing exceeded {} shapes",
                self.limits.max_shapes
            ));
        }
        self.shapes.push(shape);
        self.shape_layers.push(layer);
        self.shape_classes.push(None);
        self.shape_spans.push(self.current_span.clone());
        Ok(())
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
}
