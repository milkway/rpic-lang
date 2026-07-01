//! SVG 1.1 backend. Renders a [`Drawing`] to an SVG string.
//!
//! Reference semantics taken from dpic's `svg.c`: y-axis flipped (SVG is
//! y-down), arrowheads emitted as filled polygons, splines as cubic Béziers,
//! gray fills via pic's `0 = black … 1 = white` convention. Coordinates scale
//! by 96 px/inch (dpic's `dpPPI`).

use crate::geom::{Bbox, Point};
use crate::ir::*;

const PPI: f64 = 96.0;
const FONT_PT: f64 = 11.0;

/// Render a drawing to an SVG document string.
pub fn to_svg(d: &Drawing) -> String {
    let mut r = Svg::new(d);
    r.render(d);
    r.finish()
}

struct Svg {
    out: String,
    west: f64,
    north: f64,
    pad: f64,
}

impl Svg {
    fn new(d: &Drawing) -> Self {
        let raw = drawing_svg_bounds(&d.shapes);
        let (west, north) = if raw.is_empty() {
            (0.0, 0.0)
        } else {
            (raw.min.x, raw.max.y)
        };
        Svg {
            out: String::new(),
            west,
            north,
            pad: d.prelude_thick.max(0.0) / 144.0,
        }
    }

    /// Map a pic point to SVG pixel space (y flipped).
    fn p(&self, p: Point) -> Point {
        Point::new(
            (p.x - self.west + 2.0 * self.pad) * PPI,
            (self.north - p.y + self.pad) * PPI,
        )
    }

    fn render(&mut self, d: &Drawing) {
        let raw = drawing_svg_bounds(&d.shapes);
        let (w, h) = if raw.is_empty() {
            (6.0 * self.pad * PPI, 6.0 * self.pad * PPI)
        } else {
            (
                (raw.width() + 6.0 * self.pad) * PPI,
                (raw.height() + 6.0 * self.pad) * PPI,
            )
        };
        self.out.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" font-family=\"sans-serif\" font-size=\"{}\" fill=\"none\">\n",
            num(w),
            num(h),
            num(w),
            num(h),
            num(FONT_PT * PPI / 72.0),
        ));
        for (i, s) in d.shapes.iter().enumerate() {
            self.out.push_str(&format!("<g id=\"s{i}\">\n"));
            self.shape(s);
            self.out.push_str("</g>\n");
        }
    }

    fn finish(mut self) -> String {
        self.out.push_str("</svg>\n");
        self.out
    }

    fn shape(&mut self, s: &Shape) {
        match s {
            Shape::Box {
                c,
                w,
                h,
                rad,
                style,
                text,
            } => {
                if closed_shape_is_visible(style) {
                    let box_w = w.abs();
                    let box_h = h.abs();
                    let tl = self.p(Point::new(c.x - box_w / 2.0, c.y + box_h / 2.0));
                    let mut attrs = format!(
                        "x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                        num(tl.x),
                        num(tl.y),
                        num(box_w * PPI),
                        num(box_h * PPI)
                    );
                    if *rad > 0.0 {
                        attrs.push_str(&format!(" rx=\"{}\"", num(rad * PPI)));
                    }
                    self.out
                        .push_str(&format!("<rect {} {}/>\n", attrs, self.paint(style)));
                }
                self.text(*c, text);
            }
            Shape::Circle {
                c, r, style, text, ..
            } => {
                if closed_shape_is_visible(style) {
                    let cc = self.p(*c);
                    self.out.push_str(&format!(
                        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" {}/>\n",
                        num(cc.x),
                        num(cc.y),
                        num(r * PPI),
                        self.paint(style)
                    ));
                }
                self.text(*c, text);
            }
            Shape::Ellipse {
                c,
                w,
                h,
                style,
                text,
            } => {
                if closed_shape_is_visible(style) {
                    let cc = self.p(*c);
                    self.out.push_str(&format!(
                        "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" {}/>\n",
                        num(cc.x),
                        num(cc.y),
                        num(w / 2.0 * PPI),
                        num(h / 2.0 * PPI),
                        self.paint(style)
                    ));
                }
                self.text(*c, text);
            }
            Shape::Path {
                pts,
                arrows,
                style,
                text,
            } => {
                if pts.len() >= 2 {
                    let stroke_pts = self.path_stroke_points(pts, *arrows, style);
                    if pts.len() == 2 {
                        if !style.invis {
                            let a = stroke_pts[0];
                            let b = stroke_pts[1];
                            self.out.push_str(&format!(
                                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" {}/>\n",
                                num(a.x),
                                num(a.y),
                                num(b.x),
                                num(b.y),
                                self.stroke(style)
                            ));
                        }
                    } else {
                        let pstr: Vec<String> = pts
                            .iter()
                            .map(|p| {
                                let q = self.p(*p);
                                format!("{},{}", num(q.x), num(q.y))
                            })
                            .collect();
                        if style.fill_open {
                            self.out.push_str(&format!(
                                "<polyline points=\"{}\" fill=\"{}\" stroke-width=\"0\" stroke=\"black\"/>\n",
                                pstr.join(" "),
                                self.fill_value(style)
                            ));
                        }
                        if !style.invis {
                            let stroke_pstr: Vec<String> = stroke_pts
                                .iter()
                                .map(|p| format!("{},{}", num(p.x), num(p.y)))
                                .collect();
                            self.out.push_str(&format!(
                                "<polyline points=\"{}\" fill=\"none\" {}/>\n",
                                stroke_pstr.join(" "),
                                self.stroke(style)
                            ));
                        }
                    }
                    if !style.invis {
                        self.arrowheads(pts, *arrows, style);
                    }
                }
                if let Some(c) = midpoint(pts) {
                    self.text(c, text);
                }
            }
            Shape::Spline {
                pts,
                tension,
                arrows,
                style,
                text,
            } => {
                if pts.len() >= 2 {
                    if style.fill_open {
                        let d = self.spline_path(pts, *tension);
                        self.out.push_str(&format!(
                            "<path d=\"{}\" fill=\"{}\" stroke-width=\"0\" stroke=\"black\"/>\n",
                            d,
                            self.fill_value(style)
                        ));
                    }
                    if !style.invis {
                        let stroke_pts = self.path_stroke_points(pts, *arrows, style);
                        let d = spline_path_points(&stroke_pts, *tension);
                        self.out.push_str(&format!(
                            "<path d=\"{}\" fill=\"none\" {}/>\n",
                            d,
                            self.stroke(style)
                        ));
                        self.arrowheads(pts, *arrows, style);
                    }
                }
                if let Some(c) = midpoint(pts) {
                    self.text(c, text);
                }
            }
            Shape::Arc {
                c,
                r,
                a0,
                a1,
                cw: _,
                arrows,
                style,
                text,
            } => {
                let start0 = *c + Point::new(a0.cos(), a0.sin()) * *r;
                let end0 = *c + Point::new(a1.cos(), a1.sin()) * *r;
                let arc_angle0 = *a1 - *a0;
                if style.fill_open {
                    let d = self.arc_path(start0, end0, *r, arc_angle0);
                    self.out.push_str(&format!(
                        "<path d=\"{}\" fill=\"{}\" stroke-width=\"0\" stroke=\"black\"/>\n",
                        d,
                        self.fill_value(style)
                    ));
                }
                if !style.invis {
                    let mut start = start0;
                    let mut end = end0;
                    let color = attr(&style.stroke.clone().unwrap_or_else(|| "black".into()));
                    let mut head_paths = String::new();
                    if *r > 1e-9 && matches!(arrows, Arrowheads::Start | Arrowheads::Both) {
                        let head =
                            self.arc_arrowhead_path(*c, start, *r, arc_angle0, style, &color);
                        start = head.point;
                        head_paths.push_str(&head.path);
                    }
                    if *r > 1e-9 && matches!(arrows, Arrowheads::End | Arrowheads::Both) {
                        let head = self.arc_arrowhead_path(*c, end, -*r, arc_angle0, style, &color);
                        end = head.point;
                        head_paths.push_str(&head.path);
                    }
                    self.out.push_str(&head_paths);
                    let arc_angle = arc_angle_between(*c, start, end, arc_angle0);
                    let d = self.arc_path(start, end, *r, arc_angle);
                    self.out.push_str(&format!(
                        "<path d=\"{}\" fill=\"none\" {}/>\n",
                        d,
                        self.stroke(style)
                    ));
                }
                self.text(*c, text);
            }
            Shape::Text { at, text, .. } => self.text(*at, text),
        }
    }

    // ---- painting ----------------------------------------------------------

    fn path_stroke_points(&self, pts: &[Point], arrows: Arrowheads, style: &Style) -> Vec<Point> {
        let mut out: Vec<Point> = pts.iter().map(|p| self.p(*p)).collect();
        if out.len() < 2 {
            return out;
        }
        let n = out.len();
        if style.arrow_filled {
            if matches!(arrows, Arrowheads::End | Arrowheads::Both)
                && let Some((_, _, _, p)) = filled_arrowhead_points(out[n - 1], out[n - 2], style)
            {
                out[n - 1] = p;
            }
            if matches!(arrows, Arrowheads::Start | Arrowheads::Both)
                && let Some((_, _, _, p)) = filled_arrowhead_points(out[0], out[1], style)
            {
                out[0] = p;
            }
        } else {
            if matches!(arrows, Arrowheads::End | Arrowheads::Both)
                && let Some((_, p, _)) = open_arrowhead_points(out[n - 1], out[n - 2], style)
            {
                out[n - 1] = p;
            }
            if matches!(arrows, Arrowheads::Start | Arrowheads::Both)
                && let Some((_, p, _)) = open_arrowhead_points(out[0], out[1], style)
            {
                out[0] = p;
            }
        }
        out
    }

    fn stroke(&self, style: &Style) -> String {
        let color = attr(&style.stroke.clone().unwrap_or_else(|| "black".into()));
        let mut s = format!(
            "stroke=\"{}\" stroke-width=\"{}\"",
            color,
            num(thick_px(style))
        );
        let thick = thick_px(style);
        match style.dash {
            Dash::Solid => {}
            Dash::Dashed(w) => s.push_str(&format!(
                " stroke-dasharray=\"{},{}\"",
                num(w * PPI * 7.0 / 6.0),
                num(w * PPI * 5.0 / 6.0)
            )),
            Dash::Dotted(w) => {
                let gap = w.map(|w| w * PPI).unwrap_or(thick * 5.0);
                s.push_str(&format!(
                    " stroke-dasharray=\"0.5,{}\" stroke-linecap=\"round\"",
                    num(gap)
                ));
            }
        }
        s
    }

    /// stroke + fill for closed shapes.
    fn paint(&self, style: &Style) -> String {
        let fill = self.fill_value(style);
        if style.invis {
            return format!("fill=\"{}\" stroke=\"none\"", fill);
        }
        format!("fill=\"{}\" {}", fill, self.stroke(style))
    }

    fn fill_value(&self, style: &Style) -> String {
        match &style.fill {
            None => "none".to_string(),
            Some(Fill::Gray(g)) => {
                let v = (g.clamp(0.0, 1.0) * 255.0).round() as u32;
                format!("rgb({v},{v},{v})")
            }
            Some(Fill::Color(c)) => attr(c),
        }
    }

    fn arrowheads(&mut self, pts: &[Point], arrows: Arrowheads, style: &Style) {
        if pts.len() < 2 {
            return;
        }
        let color = attr(&style.stroke.clone().unwrap_or_else(|| "black".into()));
        let head = |tip: Point, from: Point, out: &mut String| {
            let t = self.p(tip);
            let f = self.p(from);
            if style.arrow_filled {
                let Some((l, p, r, _)) = filled_arrowhead_points(t, f, style) else {
                    return;
                };
                out.push_str(&format!(
                    "<polygon stroke-width=\"0\" points=\"{},{} {},{} {},{}\" fill=\"{}\"/>\n",
                    num(l.x),
                    num(l.y),
                    num(p.x),
                    num(p.y),
                    num(r.x),
                    num(r.y),
                    color
                ));
            } else {
                let Some((l, p, r)) = open_arrowhead_points(t, f, style) else {
                    return;
                };
                out.push_str(&format!(
                    "<polyline points=\"{},{} {},{} {},{}\" fill=\"none\" {}/>\n",
                    num(l.x),
                    num(l.y),
                    num(p.x),
                    num(p.y),
                    num(r.x),
                    num(r.y),
                    self.stroke(style)
                ));
            }
        };
        let mut buf = String::new();
        if matches!(arrows, Arrowheads::End | Arrowheads::Both) {
            head(pts[pts.len() - 1], pts[pts.len() - 2], &mut buf);
        }
        if matches!(arrows, Arrowheads::Start | Arrowheads::Both) {
            head(pts[0], pts[1], &mut buf);
        }
        self.out.push_str(&buf);
    }

    fn arc_path(&self, start: Point, end: Point, r: f64, angle: f64) -> String {
        let start = self.p(start);
        let end = self.p(end);
        let large = if angle.abs() > std::f64::consts::PI {
            1
        } else {
            0
        };
        let sweep = if angle >= 0.0 { 0 } else { 1 };
        format!(
            "M {} {} A {} {} 0 {} {} {} {}",
            num(start.x),
            num(start.y),
            num(r.abs() * PPI),
            num(r.abs() * PPI),
            large,
            sweep,
            num(end.x),
            num(end.y)
        )
    }

    fn arc_to(&self, end: Point, r: f64, angle: f64, ccw: f64) -> String {
        let end = self.p(end);
        let large = if angle.abs() > std::f64::consts::PI {
            1
        } else {
            0
        };
        let sweep = if ccw > 0.0 { 0 } else { 1 };
        format!(
            " A {} {} 0 {} {} {} {}",
            num(r.abs() * PPI),
            num(r.abs() * PPI),
            large,
            sweep,
            num(end.x),
            num(end.y)
        )
    }

    fn arc_arrowhead_path(
        &self,
        c: Point,
        point: Point,
        signed_r: f64,
        angle: f64,
        style: &Style,
        color: &str,
    ) -> ArcHead {
        let atyp = if style.arrow_filled { 2 } else { 0 };
        let mut geom = arc_head_geometry(
            c,
            point,
            atyp,
            style.arrow_ht,
            style.arrow_wid,
            style.thick.filter(|t| *t > 0.0).unwrap_or(0.8),
            signed_r,
            angle,
        );
        let r = signed_r.abs();
        let mut d = String::new();
        if atyp == 0 && geom.lwi < ((geom.wid - geom.lwi) / 2.0) {
            d.push_str(&format!("M {}", self.pos(geom.px)));
            let q = prop(geom.ai, geom.ci, r + geom.lwi, -geom.lwi, r);
            d.push_str(&self.arc_to(q, r + geom.lwi, 0.0, geom.ccw));
            d.push_str(&format!(" L {}", self.pos(geom.ai)));
            d.push_str(&self.arc_to(point, r, 0.0, -geom.ccw));
            d.push_str(&self.arc_to(geom.ao, r, 0.0, geom.ccw));
            d.push_str(&format!(
                " L {}",
                self.pos(prop(geom.ao, geom.co, r - geom.lwi, geom.lwi, r))
            ));
            d.push_str(&self.arc_to(geom.px, r - geom.lwi, 1.0, -geom.ccw));
        } else {
            let q = (geom.ao + geom.ai) * 0.5;
            d.push_str(&format!("M {} L {}", self.pos(q), self.pos(geom.ai)));
            d.push_str(&self.arc_to(point, r, 0.0, -geom.ccw));
            d.push_str(&self.arc_to(geom.ao, r, 0.0, geom.ccw));
            d.push_str(&format!(" L {}", self.pos(q)));
        }
        geom.path = format!(
            "<path stroke-width=\"0\" stroke=\"{}\" fill=\"{}\" d=\"{}\"/>\n",
            color, color, d
        );
        geom
    }

    fn pos(&self, p: Point) -> String {
        let p = self.p(p);
        format!("{},{}", num(p.x), num(p.y))
    }

    /// dpic-compatible spline path. The control-point construction matches
    /// `dpic`'s SVG backend (verified against `dpic -v` output); since both the
    /// model→SVG transform and the constructions are affine, we build directly
    /// in SVG space.
    fn spline_path(&self, pts: &[Point], tension: Option<f64>) -> String {
        let q: Vec<Point> = pts.iter().map(|p| self.p(*p)).collect();
        spline_path_points(&q, tension)
    }

    fn text(&mut self, center: Point, lines: &[TextLine]) {
        if lines.is_empty() {
            return;
        }
        let c = self.p(center);
        let font_px = FONT_PT * PPI / 72.0;
        let lh = font_px * 1.2;
        let xheight = font_px * 0.66;
        let n = lines.len() as f64;
        for (i, line) in lines.iter().enumerate() {
            let dy = (i as f64 - (n - 1.0) / 2.0) * lh;
            let anchor = match line.halign {
                -1 => "start",
                1 => "end",
                _ => "middle",
            };
            let just_offset = xheight / 2.0 + line.text_offset * PPI;
            let x = c.x
                + match line.halign {
                    -1 => line.text_offset * PPI,
                    1 => -line.text_offset * PPI,
                    _ => 0.0,
                };
            let y = c.y + dy - (line.valign as f64) * just_offset;
            self.out.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" dominant-baseline=\"central\" fill=\"black\">{}</text>\n",
                num(x),
                num(y),
                anchor,
                escape(&line.s)
            ));
        }
    }
}

// ---- helpers ---------------------------------------------------------------

fn drawing_svg_bounds(shapes: &[Shape]) -> Bbox {
    let mut out = Bbox::new();
    for sh in shapes {
        out.union(&shape_svg_bounds(sh));
    }
    out
}

fn shape_svg_bounds(sh: &Shape) -> Bbox {
    let mut out = Bbox::new();
    match sh {
        Shape::Box {
            c,
            w,
            h,
            style,
            text: _,
            ..
        } => {
            if closed_shape_is_visible(style) {
                out.add(*c - Point::new(*w / 2.0, *h / 2.0));
                out.add(*c + Point::new(*w / 2.0, *h / 2.0));
            }
        }
        Shape::Circle {
            c,
            r,
            style,
            text: _,
        } => {
            if closed_shape_is_visible(style) {
                out.add(*c - Point::new(*r, *r));
                out.add(*c + Point::new(*r, *r));
            }
        }
        Shape::Ellipse {
            c,
            w,
            h,
            style,
            text: _,
        } => {
            if closed_shape_is_visible(style) {
                out.add(*c - Point::new(*w / 2.0, *h / 2.0));
                out.add(*c + Point::new(*w / 2.0, *h / 2.0));
            }
        }
        Shape::Path {
            pts,
            style,
            text: _,
            ..
        }
        | Shape::Spline {
            pts,
            style,
            text: _,
            ..
        } => {
            if !style.invis {
                for p in pts {
                    out.add(*p);
                }
            }
        }
        Shape::Arc {
            c,
            r,
            a0,
            a1,
            style,
            text: _,
            ..
        } => {
            if !style.invis {
                for k in 0..=12 {
                    let t = *a0 + (*a1 - *a0) * (k as f64 / 12.0);
                    out.add(*c + Point::new(t.cos(), t.sin()) * *r);
                }
            }
        }
        Shape::Text { bbox, .. } => out.union(bbox),
    }
    out
}

fn spline_path_points(q: &[Point], tension: Option<f64>) -> String {
    let n = q.len();
    // Fewer than 3 control points: just a straight polyline.
    if n < 3 {
        let mut d = format!("M {} {}", num(q[0].x), num(q[0].y));
        for p in &q[1..] {
            d.push_str(&format!(" L {} {}", num(p.x), num(p.y)));
        }
        return d;
    }
    match tension {
        None => classic_spline(q),
        Some(t) => tensioned_spline(q, t),
    }
}

fn thick_px(style: &Style) -> f64 {
    // style.thick is in points; default ~0.8pt.
    let pt = style.thick.filter(|t| *t > 0.0).unwrap_or(0.8);
    pt * PPI / 72.0
}

struct ArcHead {
    point: Point,
    path: String,
    ao: Point,
    ai: Point,
    co: Point,
    ci: Point,
    px: Point,
    ccw: f64,
    lwi: f64,
    wid: f64,
}

#[allow(clippy::too_many_arguments)]
fn arc_head_geometry(
    c: Point,
    point: Point,
    atyp: i32,
    ht: f64,
    wid: f64,
    lth: f64,
    signed_r: f64,
    angle: f64,
) -> ArcHead {
    let ccw = if signed_r * angle > 0.0 { 1.0 } else { -1.0 };
    let r = signed_r.abs();
    let ht = ht.abs().min(2.0 * r);
    let mut wid = if atyp == 0 {
        wid.abs().min(r)
    } else {
        wid.abs()
    };
    let lwi = lth.abs() / 72.0;
    wid = wid.max(lwi);

    let ha = if r == 0.0 { 0.0 } else { ht / r };
    let q = Point::new(ha.cos(), ccw * ha.sin());
    let ac = affine(point.x - c.x, point.y - c.y, c, q);
    let ao = prop(c, ac, wid / -2.0, r + wid / 2.0, r);
    let ai = prop(c, ac, wid / 2.0, r - wid / 2.0, r);
    let co = arc_ctr(ao, point, c, ccw);
    let ci = arc_ctr(ai, point, c, ccw);

    let adjusted = if wid == 0.0 {
        ao
    } else if r == 0.0 {
        c
    } else {
        let t = (wid.min(lwi) / wid) * ht / r;
        let q = Point::new(t.cos(), ccw * t.sin());
        affine(point.x - c.x, point.y - c.y, c, q)
    };

    let px = if atyp == 0 {
        let mut px = c_intersect(co, r - lwi, ci, r + lwi, ccw);
        if px.dist(point) > ac.dist(point) {
            px = ac;
        }
        px
    } else {
        let t = if r == 0.0 {
            0.0
        } else {
            std::f64::consts::FRAC_PI_2.min((ht / r) * 2.0 / 3.0)
        };
        let q = Point::new(t.cos(), ccw * t.sin());
        let mut px = affine(point.x - c.x, point.y - c.y, c, q);
        if px.dist(point) < adjusted.dist(point) {
            px = adjusted;
        }
        px
    };

    ArcHead {
        point: adjusted,
        path: String::new(),
        ao,
        ai,
        co,
        ci,
        px,
        ccw,
        lwi,
        wid,
    }
}

fn affine(x: f64, y: f64, origin: Point, cs: Point) -> Point {
    Point::new(
        origin.x + cs.x * x - cs.y * y,
        origin.y + cs.y * x + cs.x * y,
    )
}

fn arc_ctr(aa: Point, p: Point, cc: Point, ccw: f64) -> Point {
    let a = aa - p;
    let c = cc - p;
    let asq = a.x * a.x + a.y * a.y;
    let rsq = c.x * c.x + c.y * c.y;
    if asq == 0.0 || rsq == 0.0 {
        return cc;
    }
    let qy = ccw * (a.x * c.x + a.y * c.y) / (asq * rsq).sqrt();
    let qx = (1.0 - qy * qy).max(0.0).sqrt();
    let br = (1.0 - (asq / (rsq * 4.0))).max(0.0).sqrt();
    let ax = (aa + p) * 0.5;
    affine(br * c.x, br * c.y, ax, Point::new(qx, qy))
}

fn c_intersect(c1: Point, r1: f64, c2: Point, r2: f64, ccw: f64) -> Point {
    let dx = c1.x - c2.x;
    let dy = c1.y - c2.y;
    let cls = dx * dx + dy * dy;
    if cls == 0.0 {
        return c1;
    }
    let cq = (cls + r1 * r1 - r2 * r2) / 2.0;
    let mut f = cq / cls;
    let x = Point::new((1.0 - f) * c1.x + f * c2.x, (1.0 - f) * c1.y + f * c2.y);
    f = ((cls * r1 * r1 - cq * cq).max(0.0)).sqrt() / cls;
    Point::new(x.x + dy * f * ccw, x.y - dx * f * ccw)
}

fn arc_angle_between(c: Point, start: Point, end: Point, old_angle: f64) -> f64 {
    let a0 = (start - c).y.atan2((start - c).x);
    let mut da = (end - c).y.atan2((end - c).x) - a0;
    while da <= -std::f64::consts::PI {
        da += 2.0 * std::f64::consts::PI;
    }
    while da > std::f64::consts::PI {
        da -= 2.0 * std::f64::consts::PI;
    }
    if da < 0.0 && old_angle > 0.0 {
        da += 2.0 * std::f64::consts::PI;
    } else if da > 0.0 && old_angle < 0.0 {
        da -= 2.0 * std::f64::consts::PI;
    }
    da
}

fn open_arrowhead_points(tip: Point, shaft: Point, style: &Style) -> Option<(Point, Point, Point)> {
    let mut u = tip - shaft;
    let len = u.len();
    if len < 1e-9 {
        return None;
    }
    u = u / len;
    let perp = Point::new(-u.y, u.x);
    let ht = style.arrow_ht * PPI;
    let wid = style.arrow_wid * PPI;
    let ltu = thick_px(style);
    let po = if wid.abs() < 1e-12 {
        0.0
    } else {
        (ltu * (ht * ht + wid * wid / 4.0).sqrt() / wid).min(ht)
    };
    let point = tip - u * po;
    let h = ht - ltu / 2.0;
    let x = h - po;
    let v = if ht.abs() < 1e-12 {
        0.0
    } else {
        (wid / 2.0) * x / ht
    };
    let left = tip - u * h - perp * v;
    let right = tip - u * h + perp * v;
    let y = if ht.abs() < 1e-12 {
        0.0
    } else {
        ht - po + (ltu * wid / ht / 4.0)
    };
    Some((
        prop(point, left, x - y, y, x),
        point,
        prop(point, right, x - y, y, x),
    ))
}

fn filled_arrowhead_points(
    tip: Point,
    shaft: Point,
    style: &Style,
) -> Option<(Point, Point, Point, Point)> {
    let mut u = tip - shaft;
    let len = u.len();
    if len < 1e-9 {
        return None;
    }
    u = u / len;
    let perp = Point::new(-u.y, u.x);
    let ht = style.arrow_ht * PPI;
    let wid = style.arrow_wid * PPI;
    let ltu = thick_px(style);
    let po = if wid.abs() < 1e-12 {
        0.0
    } else {
        (ltu * (ht * ht + wid * wid / 4.0).sqrt() / wid).min(ht)
    };
    let point = tip - u * po;
    let h = ht - ltu / 2.0;
    let x = h - po;
    let v = if ht.abs() < 1e-12 {
        0.0
    } else {
        (wid / 2.0) * x / ht
    };
    let left = tip - u * h - perp * v;
    let right = tip - u * h + perp * v;
    let t = if x.abs() < 1e-12 { 1.0 } else { ht / x };
    let left_full = tip + (left - point) * t;
    let right_full = tip + (right - point) * t;
    Some((left_full, tip, right_full, point))
}

fn prop(p1: Point, p2: Point, a: f64, b: f64, c: f64) -> Point {
    if c.abs() < 1e-12 {
        p2
    } else {
        (p1 * a + p2 * b) / c
    }
}

fn midpoint(pts: &[Point]) -> Option<Point> {
    if pts.is_empty() {
        None
    } else {
        Some((pts[0] + pts[pts.len() - 1]) * 0.5)
    }
}

fn closed_shape_is_visible(style: &Style) -> bool {
    !style.invis || style.fill.is_some()
}

/// Classic pic spline (no tension), matching dpic's `svgsplinesegment`.
/// Dpic emits one SVG `C` command with multiple cubic segments. The control
/// points are fixed fractions along each original segment, not a raised
/// quadratic through midpoint knots.
fn classic_spline(q: &[Point]) -> String {
    let segs = q.len() - 1;
    let mut d = format!("M {} {} C", num(q[0].x), num(q[0].y));
    for i in 0..segs {
        let a = q[i];
        let b = q[i + 1];
        let mut add = |p: Point| d.push_str(&format!(" {} {}", num(p.x), num(p.y)));
        if i == 0 {
            add(prop(a, b, 5.0, 1.0, 6.0));
            add(prop(a, b, 2.0, 1.0, 3.0));
            add(prop(a, b, 1.0, 1.0, 2.0));
            add(prop(a, b, 1.0, 5.0, 6.0));
        } else if i < segs - 1 {
            add(prop(a, b, 5.0, 1.0, 6.0));
            add(prop(a, b, 1.0, 1.0, 2.0));
            add(prop(a, b, 1.0, 5.0, 6.0));
        } else {
            add(prop(a, b, 5.0, 1.0, 6.0));
            add(prop(a, b, 1.0, 1.0, 2.0));
            add(prop(a, b, 1.0, 2.0, 3.0));
            add(prop(a, b, 1.0, 5.0, 6.0));
            add(b);
        }
    }
    d
}

/// dpic tensioned spline: starts at the first control point, ends at the last,
/// passes through the midpoints of the *interior* segments, and bends toward
/// each control vertex by `t`. Control point = endpoint + t·(vertex − endpoint),
/// matching dpic's SVG backend exactly.
fn tensioned_spline(q: &[Point], t: f64) -> String {
    let n = q.len();
    // knots: V0, mid(V1,V2), …, mid(V_{n-3},V_{n-2}), V_{n-1}
    let mut knots = Vec::with_capacity(n);
    knots.push(q[0]);
    for i in 1..n - 2 {
        knots.push((q[i] + q[i + 1]) * 0.5);
    }
    knots.push(q[n - 1]);
    let mut d = format!("M {} {}", num(knots[0].x), num(knots[0].y));
    for j in 0..knots.len() - 1 {
        let a = knots[j];
        let b = knots[j + 1];
        let w = q[j + 1]; // via vertex for this cubic
        let c1 = a + (w - a) * t;
        let c2 = b + (w - b) * t;
        push_cubic(&mut d, c1, c2, b);
    }
    d
}

fn push_cubic(d: &mut String, c1: Point, c2: Point, end: Point) {
    d.push_str(&format!(
        " C {} {} {} {} {} {}",
        num(c1.x),
        num(c1.y),
        num(c2.x),
        num(c2.y),
        num(end.x),
        num(end.y)
    ));
}

/// Format a float compactly (up to 6 decimals, no trailing zeros). Non-finite
/// values (NaN/Inf, e.g. from a zero-length element) become `0` so the SVG stays
/// well-formed instead of emitting a literal `NaN`.
fn num(x: f64) -> String {
    if !x.is_finite() {
        return "0".to_string();
    }
    let r = (x * 1_000_000.0).round() / 1_000_000.0;
    let r = if r == 0.0 { 0.0 } else { r }; // normalise -0
    let mut s = format!("{r:.6}");
    while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
        s.pop();
    }
    s
}

/// Escape text content (`&`, `<`, `>`).
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape a string for embedding inside a double-quoted SVG attribute.
fn attr(s: &str) -> String {
    escape(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{eval::eval, parser::parse};

    fn svg(src: &str) -> String {
        to_svg(&eval(&parse(src).unwrap()).unwrap())
    }

    fn text_y(svg: &str, text: &str) -> f64 {
        let needle = format!(">{text}</text>");
        let line = svg.lines().find(|line| line.contains(&needle)).unwrap();
        let y = line.split(" y=\"").nth(1).unwrap();
        y.split('"').next().unwrap().parse().unwrap()
    }

    fn text_x(svg: &str, text: &str) -> f64 {
        let needle = format!(">{text}</text>");
        let line = svg.lines().find(|line| line.contains(&needle)).unwrap();
        let x = line.split(" x=\"").nth(1).unwrap();
        x.split('"').next().unwrap().parse().unwrap()
    }

    #[test]
    fn pipeline_svg_has_elements() {
        let s = svg(".PS\nellipse \"document\"\narrow\nbox \"PIC\"\n.PE");
        assert!(s.starts_with("<svg"));
        assert!(s.lines().next().unwrap().contains("fill=\"none\""));
        assert!(s.contains("<ellipse"));
        assert!(s.contains("<rect"));
        assert!(s.contains("<line"));
        assert!(s.contains("<polygon")); // arrowhead
        assert!(s.contains(">document<"));
        assert!(s.contains("fill=\"black\">document</text>"));
        assert!(s.contains("</svg>"));
    }

    #[test]
    fn multipoint_path_gets_polyline() {
        let s = svg("line right then up");
        assert!(s.contains("<polyline"));
    }

    #[test]
    fn closed_path_can_be_filled() {
        let s = svg("line fill 0.5 right then up then left then down");
        assert!(s.contains("fill=\"rgb(128,128,128)\" stroke-width=\"0\""));
    }

    #[test]
    fn open_multipoint_path_can_be_filled() {
        let s = svg("line fill 0.5 right then up then left");
        assert!(s.contains("fill=\"rgb(128,128,128)\" stroke-width=\"0\""));
    }

    #[test]
    fn spline_can_be_filled() {
        let s = svg("spline fill 0.5 right then up then left");
        assert!(s.contains("<path"));
        assert!(s.contains("fill=\"rgb(128,128,128)\" stroke-width=\"0\""));
    }

    #[test]
    fn classic_spline_control_points_match_dpic() {
        let d = classic_spline(&[
            Point::new(0.0, 0.0),
            Point::new(6.0, 0.0),
            Point::new(6.0, 6.0),
        ]);
        assert_eq!(d, "M 0 0 C 1 0 2 0 3 0 5 0 6 1 6 3 6 4 6 5 6 6");
    }

    #[test]
    fn filled_spline_arrow_stroke_is_trimmed_like_dpic() {
        let q0 = Point::new(4.0, 52.0);
        let q1 = Point::new(52.0, 4.0);
        let tip = Point::new(100.0, 52.0);
        let (_, _, _, stroke_end) = filled_arrowhead_points(tip, q1, &Style::default()).unwrap();
        assert!((stroke_end.x - 98.445_079).abs() < 1e-6);
        assert!((stroke_end.y - 50.445_079).abs() < 1e-6);

        let d = spline_path_points(&[q0, q1, stroke_end], None);
        assert!(
            d.ends_with("98.445079 50.445079"),
            "spline path should end at the dpic-receded arrow point: {d}"
        );
        assert!(
            !d.ends_with("100 52"),
            "spline path still reaches the arrow tip: {d}"
        );
    }

    #[test]
    fn svg_prelude_bounds_match_dpic_for_lines() {
        let s = svg("line right");
        assert!(
            s.contains("width=\"51.2\" height=\"3.2\" viewBox=\"0 0 51.2 3.2\""),
            "{s}"
        );
        assert!(
            s.contains("<line x1=\"1.066667\" y1=\"0.533333\" x2=\"49.066667\" y2=\"0.533333\""),
            "{s}"
        );

        let s = svg("linethick = 0.4\nline right");
        assert!(
            s.contains("width=\"49.6\" height=\"1.6\" viewBox=\"0 0 49.6 1.6\""),
            "{s}"
        );
        assert!(
            s.contains("<line x1=\"0.533333\" y1=\"0.266667\" x2=\"48.533333\" y2=\"0.266667\""),
            "{s}"
        );
        assert!(s.contains("stroke-width=\"0.533333\""), "{s}");
    }

    #[test]
    fn attached_text_does_not_expand_svg_prelude_bounds() {
        let s = svg("box wid .2 \"longlonglong\"");
        assert!(
            s.contains("width=\"22.4\" height=\"51.2\" viewBox=\"0 0 22.4 51.2\""),
            "{s}"
        );
        assert!(
            s.contains("<rect x=\"1.066667\" y=\"0.533333\" width=\"19.2\" height=\"48\""),
            "{s}"
        );
    }

    #[test]
    fn arc_can_be_filled() {
        let s = svg("arc fill 0.5");
        assert!(s.contains("fill=\"rgb(128,128,128)\" stroke-width=\"0\""));
    }

    #[test]
    fn open_color_changes_stroke_without_area_fill() {
        let s = svg("arc color \"red\"");
        assert!(s.contains("stroke=\"red\""));
        assert!(!s.contains("stroke-width=\"0\" stroke=\"black\""));
    }

    #[test]
    fn root_fill_none_matches_dpic_invalid_color_fallback() {
        let s = svg("line right then up then left shaded \"Dandelion\"");
        assert!(s.lines().next().unwrap().contains("fill=\"none\""));
        assert!(s.contains("fill=\"Dandelion\""), "{s}");
    }

    #[test]
    fn dashed_box_gets_dasharray() {
        let s = svg("box \"x\" dashed");
        assert!(s.contains("stroke-dasharray"));
    }

    #[test]
    fn negative_box_dimensions_emit_positive_svg_rect() {
        let s = svg("box wid -0.5 ht -0.25");
        assert!(s.contains("<rect"));
        assert!(s.contains("width=\"48\""), "{s}");
        assert!(s.contains("height=\"24\""), "{s}");
        assert!(!s.contains("width=\"-"), "{s}");
        assert!(!s.contains("height=\"-"), "{s}");
    }

    #[test]
    fn filled_circle() {
        let s = svg("circle fill 0");
        assert!(s.contains("<circle"));
        assert!(s.contains("rgb(0,0,0)"));
    }

    #[test]
    fn invisible_filled_closed_shape_keeps_fill_only() {
        let s = svg("box invis fill 0.5");
        assert!(s.contains("<rect"));
        assert!(s.contains("fill=\"rgb(128,128,128)\""));
        assert!(s.contains("stroke=\"none\""));
    }

    #[test]
    fn xml_is_escaped() {
        let s = svg("box \"a < b & c\"");
        assert!(s.contains("a &lt; b &amp; c"));
    }

    #[test]
    fn text_justification_is_per_string() {
        let s = svg("\"LLLL\" ljust\n\"RRRR\" rjust");
        assert!(s.contains("text-anchor=\"start\""));
        assert!(s.contains(">LLLL</text>"));
        assert!(s.contains("text-anchor=\"end\""));
        assert!(s.contains(">RRRR</text>"));

        let s = svg("box wid 1 ht .6 \"AAAA\" above \"BBBB\" below");
        let above = text_y(&s, "AAAA");
        let below = text_y(&s, "BBBB");
        assert!(above < below, "above={above} below={below}");
        assert!((below - above) < 40.0, "above/below offset too large: {s}");
    }

    #[test]
    fn horizontal_text_justification_uses_textoffset() {
        let s = svg("textoffset = 0.1\n\"L\" ljust at (0,0)\n\"R\" rjust at (0,0)");
        let l = text_x(&s, "L");
        let r = text_x(&s, "R");
        assert!(
            (l - r - 19.2).abs() < 1e-9,
            "expected opposite 0.1in offsets in SVG px: {s}"
        );
    }

    #[test]
    fn open_arrowhead_geometry_matches_dpic_default() {
        let style = Style {
            arrow_filled: false,
            ..Default::default()
        };
        let (left, point, right) = open_arrowhead_points(
            Point::new(97.066_667, 2.933_333),
            Point::new(1.066_667, 2.933_333),
            &style,
        )
        .unwrap();
        assert!((point.x - 94.867_677).abs() < 1e-6, "point={point:?}");
        assert!((point.y - 2.933_333).abs() < 1e-6, "point={point:?}");
        assert!((left.x - 87.333_333).abs() < 1e-6, "left={left:?}");
        assert!((left.y - 1.049_747).abs() < 1e-6, "left={left:?}");
        assert!((right.x - 87.333_333).abs() < 1e-6, "right={right:?}");
        assert!((right.y - 4.816_919).abs() < 1e-6, "right={right:?}");
    }

    #[test]
    fn filled_arrowhead_geometry_matches_dpic_default() {
        let style = Style::default();
        let (left, point, right, stroke_end) = filled_arrowhead_points(
            Point::new(97.066_667, 2.933_333),
            Point::new(1.066_667, 2.933_333),
            &style,
        )
        .unwrap();
        assert!(
            (stroke_end.x - 94.867_677).abs() < 1e-6,
            "stroke_end={stroke_end:?}"
        );
        assert!(
            (stroke_end.y - 2.933_333).abs() < 1e-6,
            "stroke_end={stroke_end:?}"
        );
        assert!((point.x - 97.066_667).abs() < 1e-6, "point={point:?}");
        assert!((point.y - 2.933_333).abs() < 1e-6, "point={point:?}");
        assert!((left.x - 87.466_667).abs() < 1e-6, "left={left:?}");
        assert!((left.y - 0.533_333).abs() < 1e-6, "left={left:?}");
        assert!((right.x - 87.466_667).abs() < 1e-6, "right={right:?}");
        assert!((right.y - 5.333_333).abs() < 1e-6, "right={right:?}");
    }

    #[test]
    fn open_arc_arrowhead_uses_curved_stroke_outline() {
        let s = svg("arrowhead = 0\nlinethick = 4\narc <-> wid .5 ht .5");
        assert!(s.contains("<path stroke-width=\"0\" stroke=\"black\" fill=\"black\""));
        assert!(s.contains(" A "), "{s}");
        assert!(!s.contains("<polyline"), "{s}");
    }

    #[test]
    fn arc_with_explicit_center_uses_large_clockwise_svg_sweep() {
        let s = svg("arc cw rad 1 from (0,-1) to (1,0) with .c at (0,0)");
        assert!(s.contains(" A 96 96 0 1 1 "), "{s}");
    }

    #[test]
    fn num_guards_non_finite() {
        assert_eq!(num(f64::NAN), "0");
        assert_eq!(num(f64::INFINITY), "0");
        assert_eq!(num(-0.0), "0");
    }

    #[test]
    fn attr_escapes_quotes_and_markup() {
        assert_eq!(attr("a\"<b&"), "a&quot;&lt;b&amp;");
    }
}
