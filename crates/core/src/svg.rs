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
const DP_TEXT_RATIO: f64 = 0.66;

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
    margin: CanvasMargin,
    next_pattern: usize,
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
            margin: d.canvas_margin,
            next_pattern: 0,
        }
    }

    /// Map a pic point to SVG pixel space (y flipped).
    fn p(&self, p: Point) -> Point {
        Point::new(
            (p.x - self.west + 2.0 * self.pad + self.margin.left) * PPI,
            (self.north - p.y + self.pad + self.margin.top) * PPI,
        )
    }

    fn render(&mut self, d: &Drawing) {
        let raw = drawing_svg_bounds(&d.shapes);
        let raw_w = if raw.is_empty() { 0.0 } else { raw.width() };
        let raw_h = if raw.is_empty() { 0.0 } else { raw.height() };
        let (w, h) = if raw.is_empty() {
            (
                positive_extent(6.0 * self.pad + self.margin.horizontal()) * PPI,
                positive_extent(6.0 * self.pad + self.margin.vertical()) * PPI,
            )
        } else {
            (
                positive_extent(raw_w + 6.0 * self.pad + self.margin.horizontal()) * PPI,
                positive_extent(raw_h + 6.0 * self.pad + self.margin.vertical()) * PPI,
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
        let mut order: Vec<usize> = (0..d.shapes.len()).collect();
        order.sort_by_key(|&i| (d.shape_layers.get(i).copied().unwrap_or(0), i));
        for i in order {
            let s = &d.shapes[i];
            // The stable `s<N>` id is the GSAP/animation target; a `class`
            // extension hook rides alongside it without changing the id.
            match d.shape_classes.get(i).and_then(|c| c.as_deref()) {
                Some(class) => self
                    .out
                    .push_str(&format!("<g id=\"s{i}\" class=\"{}\">\n", attr(class))),
                None => self.out.push_str(&format!("<g id=\"s{i}\">\n")),
            }
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
                    let paint = self.paint(style);
                    self.out.push_str(&format!("<rect {} {}/>\n", attrs, paint));
                }
                self.text(*c, text);
            }
            Shape::Circle {
                c, r, style, text, ..
            } => {
                if closed_shape_is_visible(style) {
                    let cc = self.p(*c);
                    let paint = self.paint(style);
                    self.out.push_str(&format!(
                        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" {}/>\n",
                        num(cc.x),
                        num(cc.y),
                        num(r * PPI),
                        paint
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
                    let paint = self.paint(style);
                    self.out.push_str(&format!(
                        "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" {}/>\n",
                        num(cc.x),
                        num(cc.y),
                        num(w / 2.0 * PPI),
                        num(h / 2.0 * PPI),
                        paint
                    ));
                }
                self.text(*c, text);
            }
            Shape::Path {
                pts,
                closed,
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
                            let fill = self.fill_attr(style);
                            if *closed {
                                self.out.push_str(&format!(
                                    "<polygon points=\"{}\" fill=\"{}\"{} stroke-width=\"0\" stroke=\"black\"/>\n",
                                    pstr.join(" "),
                                    fill,
                                    fill_opacity_attr(style)
                                ));
                            } else {
                                self.out.push_str(&format!(
                                    "<polyline points=\"{}\" fill=\"{}\"{} stroke-width=\"0\" stroke=\"black\"/>\n",
                                    pstr.join(" "),
                                    fill,
                                    fill_opacity_attr(style)
                                ));
                            }
                        }
                        if !style.invis {
                            let stroke_pstr: Vec<String> = stroke_pts
                                .iter()
                                .map(|p| format!("{},{}", num(p.x), num(p.y)))
                                .collect();
                            if *closed {
                                self.out.push_str(&format!(
                                    "<polygon points=\"{}\" fill=\"none\" {}/>\n",
                                    stroke_pstr.join(" "),
                                    self.stroke(style)
                                ));
                            } else {
                                self.out.push_str(&format!(
                                    "<polyline points=\"{}\" fill=\"none\" {}/>\n",
                                    stroke_pstr.join(" "),
                                    self.stroke(style)
                                ));
                            }
                        }
                    }
                    if !style.invis {
                        self.arrowheads(pts, *arrows, style);
                    }
                }
                if let Some(c) = path_text_point(pts, *closed) {
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
                        let fill = self.fill_attr(style);
                        self.out.push_str(&format!(
                            "<path d=\"{}\" fill=\"{}\"{} stroke-width=\"0\" stroke=\"black\"/>\n",
                            d,
                            fill,
                            fill_opacity_attr(style)
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
                    let fill = self.fill_attr(style);
                    self.out.push_str(&format!(
                        "<path d=\"{}\" fill=\"{}\"{} stroke-width=\"0\" stroke=\"black\"/>\n",
                        d,
                        fill,
                        fill_opacity_attr(style)
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
            Shape::Brace {
                cubics,
                label_at,
                style,
                text,
                ..
            } => {
                if !style.invis && !cubics.is_empty() {
                    let mut d = String::new();
                    let p0 = self.p(cubics[0][0]);
                    d.push_str(&format!("M {} {}", num(p0.x), num(p0.y)));
                    for cubic in cubics {
                        let c1 = self.p(cubic[1]);
                        let c2 = self.p(cubic[2]);
                        let p = self.p(cubic[3]);
                        d.push_str(&format!(
                            " C {} {}, {} {}, {} {}",
                            num(c1.x),
                            num(c1.y),
                            num(c2.x),
                            num(c2.y),
                            num(p.x),
                            num(p.y)
                        ));
                    }
                    self.out.push_str(&format!(
                        "<path d=\"{}\" fill=\"none\" {}/>\n",
                        d,
                        self.stroke(style)
                    ));
                }
                self.text(*label_at, text);
            }
            Shape::Text {
                at,
                text,
                w,
                h,
                standalone,
                ..
            } => {
                if *standalone {
                    self.standalone_text(*at, text, *w, *h);
                } else {
                    self.text(*at, text);
                }
            }
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
    fn paint(&mut self, style: &Style) -> String {
        let fill = self.fill_attr(style);
        let fill_opacity = fill_opacity_attr(style);
        if style.invis {
            return format!("fill=\"{}\"{} stroke=\"none\"", fill, fill_opacity);
        }
        format!("fill=\"{}\"{} {}", fill, fill_opacity, self.stroke(style))
    }

    fn fill_attr(&mut self, style: &Style) -> String {
        match &style.hatch {
            Some(hatch) => format!("url(#{})", self.define_hatch_pattern(style, hatch)),
            None => match &style.gradient {
                Some(g) => format!("url(#{})", self.define_linear_gradient(g)),
                None => self.fill_value(style),
            },
        }
    }

    fn define_linear_gradient(&mut self, g: &Gradient) -> String {
        let id = format!("grad{}", self.next_pattern);
        self.next_pattern += 1;
        // The angle is measured in pic coordinates (y-up): 0 = left to right,
        // 90 = bottom to top. SVG bounding-box coordinates are y-down, so the
        // direction vector flips its y component; center it in the unit box.
        let a = g.angle.to_radians();
        let (dx, dy) = (a.cos(), -a.sin());
        let (x1, y1) = (0.5 - dx / 2.0, 0.5 - dy / 2.0);
        let (x2, y2) = (0.5 + dx / 2.0, 0.5 + dy / 2.0);
        self.out.push_str(&format!(
            "<defs><linearGradient id=\"{}\" gradientUnits=\"objectBoundingBox\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">\n<stop offset=\"0\" stop-color=\"{}\"/>\n<stop offset=\"1\" stop-color=\"{}\"/>\n</linearGradient></defs>\n",
            id,
            num(x1),
            num(y1),
            num(x2),
            num(y2),
            attr(&g.from),
            attr(&g.to)
        ));
        id
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

    fn define_hatch_pattern(&mut self, style: &Style, hatch: &Hatch) -> String {
        let id = format!("hatch{}", self.next_pattern);
        self.next_pattern += 1;
        let sep = positive_extent(hatch.sep * PPI).max(1.0);
        let width = (hatch.width.max(0.0) * PPI / 72.0).max(0.0);
        let color = attr(&hatch.color);
        let bg = match &style.gradient {
            Some(g) => format!("url(#{})", self.define_linear_gradient(g)),
            None => self.fill_value(style),
        };
        self.out.push_str(&format!(
            "<defs><pattern id=\"{}\" patternUnits=\"userSpaceOnUse\" width=\"{}\" height=\"{}\" patternTransform=\"rotate({})\">\n",
            id,
            num(sep),
            num(sep),
            num(-hatch.angle)
        ));
        if bg != "none" {
            self.out.push_str(&format!(
                "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
                num(sep),
                num(sep),
                bg
            ));
        }
        self.out.push_str(&format!(
            "<line x1=\"{}\" y1=\"0\" x2=\"{}\" y2=\"0\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
            num(-sep),
            num(2.0 * sep),
            color,
            num(width)
        ));
        if hatch.cross {
            self.out.push_str(&format!(
                "<line x1=\"0\" y1=\"{}\" x2=\"0\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
                num(-sep),
                num(2.0 * sep),
                color,
                num(width)
            ));
        }
        self.out.push_str("</pattern></defs>\n");
        id
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
        self.write_text(center, lines, None);
    }

    fn standalone_text(&mut self, center: Point, lines: &[TextLine], w: f64, h: f64) {
        self.write_text(center, lines, Some((w, h)));
    }

    fn write_text(
        &mut self,
        center: Point,
        lines: &[TextLine],
        standalone_dims: Option<(f64, f64)>,
    ) {
        if lines.is_empty() {
            return;
        }
        let standalone = standalone_dims.is_some();
        let n = lines.len() as f64;
        let v = n - 1.0 + DP_TEXT_RATIO;
        let lineskip = standalone_dims
            .map(|(_, h)| h)
            .filter(|_| v.abs() > 1e-12)
            .map(|h| h / v)
            .unwrap_or(FONT_PT / 72.0);
        let xheight = lineskip * DP_TEXT_RATIO;
        let font_pt = lineskip * 72.0;
        let mut y = center.y + (v * lineskip / 2.0) - xheight;
        for line in lines {
            let anchor = match line.halign {
                -1 => "start",
                1 => "end",
                _ => "middle",
            };
            let just_offset = xheight / 2.0 + line.text_offset;
            let standalone_half_width = standalone_dims
                .map(|(w, _)| {
                    if w.abs() > 1e-12 {
                        w / 2.0
                    } else {
                        line.text_offset
                    }
                })
                .unwrap_or(0.0);
            let x = center.x
                + match line.halign {
                    -1 if standalone => standalone_half_width,
                    1 if standalone => -standalone_half_width,
                    -1 => line.text_offset,
                    1 => -line.text_offset,
                    _ => 0.0,
                };
            let baseline_y = y + (line.valign as f64) * just_offset;
            let p = self.p(Point::new(x, baseline_y));
            let text_stroke = if standalone {
                format!("stroke-width=\"{}\"", num(0.2 * PPI / 72.0))
            } else {
                "stroke-width=\"0.2pt\"".to_string()
            };
            self.out.push_str(&format!(
                "<text font-size=\"{}pt\" {} fill=\"black\" x=\"{}\" y=\"{}\" text-anchor=\"{}\">{}</text>\n",
                num(font_pt),
                text_stroke,
                num(p.x),
                num(p.y),
                anchor,
                escape(&line.s)
            ));
            y -= lineskip;
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

fn positive_extent(v: f64) -> f64 {
    if v.is_finite() && v > 0.0 { v } else { 0.0 }
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
            if !style.invis || style.invis_bounds || open_fill_is_visible(style) {
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
            if !style.invis || open_fill_is_visible(style) {
                for k in 0..=12 {
                    let t = *a0 + (*a1 - *a0) * (k as f64 / 12.0);
                    out.add(*c + Point::new(t.cos(), t.sin()) * *r);
                }
            }
        }
        Shape::Brace {
            cubics,
            label_at,
            style,
            text,
            ..
        } => {
            if !style.invis || style.invis_bounds {
                for cubic in cubics {
                    for p in cubic {
                        out.add(*p);
                    }
                }
            }
            out.union(&attached_text_bounds(*label_at, text));
        }
        Shape::Text {
            at,
            text,
            bbox,
            w,
            h,
            standalone,
            ..
        } => {
            if *standalone {
                out.union(&standalone_text_bounds(*at, text, *w, *h));
            } else {
                out.union(bbox);
            }
        }
    }
    out
}

fn standalone_text_bounds(at: Point, text: &[TextLine], w: f64, h: f64) -> Bbox {
    let mut bb = Bbox::new();
    if text.is_empty() {
        return bb;
    }
    let mut half_w = w.abs() / 2.0;
    if half_w <= 1e-12 {
        half_w = text
            .iter()
            .filter(|line| line.halign != 0 && !line.s.is_empty())
            .map(|line| line.text_offset.abs())
            .fold(0.0, f64::max);
    }
    let n = text.len() as f64;
    let v = n - 1.0 + DP_TEXT_RATIO;
    let lineskip = if h.abs() > 1e-12 && v.abs() > 1e-12 {
        h / v
    } else {
        FONT_PT / 72.0
    };
    let xheight = lineskip * DP_TEXT_RATIO;
    let mut baseline_y = at.y + (v * lineskip / 2.0) - xheight;
    for line in text {
        if line.s.is_empty() {
            baseline_y -= lineskip;
            continue;
        }
        let just_offset = xheight / 2.0 + line.text_offset;
        let y = baseline_y + (line.valign as f64) * just_offset;
        bb.add(Point::new(at.x - half_w, y));
        bb.add(Point::new(at.x + half_w, y + xheight));
        baseline_y -= lineskip;
    }
    bb
}

fn attached_text_bounds(center: Point, text: &[TextLine]) -> Bbox {
    let mut bb = Bbox::new();
    if text.iter().all(|line| line.s.is_empty()) {
        return bb;
    }
    let em = FONT_PT / 72.0;
    let char_w = 0.6 * em;
    let line_h = 1.2 * em;
    let xheight = DP_TEXT_RATIO * em;
    let n = text.len() as f64;
    for (i, line) in text.iter().enumerate() {
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

fn path_text_point(pts: &[Point], closed: bool) -> Option<Point> {
    if !closed {
        return midpoint(pts);
    }
    let mut bb = Bbox::new();
    for p in pts {
        bb.add(*p);
    }
    Some((bb.min + bb.max) * 0.5)
}

fn closed_shape_is_visible(style: &Style) -> bool {
    !style.invis || style.fill.is_some() || style.hatch.is_some() || style.gradient.is_some()
}

fn fill_opacity_attr(style: &Style) -> String {
    if style.fill.is_none() && style.hatch.is_none() {
        return String::new();
    }
    match style.fill_opacity {
        Some(opacity) => format!(" fill-opacity=\"{}\"", num(opacity)),
        None => String::new(),
    }
}

fn open_fill_is_visible(style: &Style) -> bool {
    style.fill_open && (style.fill.is_some() || style.hatch.is_some() || style.gradient.is_some())
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
        assert!(s.contains(">document</text>"));
        assert!(s.contains("</svg>"));
    }

    #[test]
    fn brace_svg_uses_cubic_path_and_label() {
        let s = svg("brace from (0,0) to (2,0) up \"n\" wid .25");
        assert!(s.contains("<path d=\"M "));
        assert!(s.contains(" C "));
        assert!(s.contains(">n</text>"));
        assert!(text_y(&s, "n") > 0.0);
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
    fn close_line_renders_polygon_and_centers_text_on_bbox() {
        let s = svg("line right 1 then up 1 close shaded \"yellow\" outlined \"black\" \"closed\"");
        assert!(s.contains("<polygon"));
        assert!(s.contains("fill=\"yellow\""));

        let x = text_x(&s, "closed");
        let y = text_y(&s, "closed");
        assert!(x > 40.0 && x < 70.0, "x = {x}\n{s}");
        assert!(y > 40.0 && y < 70.0, "y = {y}\n{s}");
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
    fn behind_renders_lower_layer_first_with_stable_ids() {
        let s = svg("A: box fill 0 at (0,0)\nB: box fill 1 behind A at (0,0)");
        let b = s.find("<g id=\"s1\">").unwrap();
        let a = s.find("<g id=\"s0\">").unwrap();
        assert!(b < a, "{s}");
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
    fn ps_sizing_scales_svg_prelude_padding_like_dpic() {
        let s = svg(".PS 3.5\nlinethick = 0.375*72\ncircle rad 3\n.PE");
        assert!(
            s.contains(
                "width=\"375.529412\" height=\"375.529412\" viewBox=\"0 0 375.529412 375.529412\""
            ),
            "{s}"
        );
        assert!(
            s.contains("<circle cx=\"177.882353\" cy=\"168\" r=\"158.117647\""),
            "{s}"
        );
        assert!(s.contains("stroke-width=\"36\""), "{s}");
    }

    #[test]
    fn canvas_margin_expands_svg_canvas_only_when_used() {
        let base = svg("line right");
        assert_eq!(svg("margin = 0; topmargin = 0; line right"), base);

        let s = svg("margin = 0.25\nline right");
        assert!(
            s.contains("width=\"99.2\" height=\"51.2\" viewBox=\"0 0 99.2 51.2\""),
            "{s}"
        );
        assert!(
            s.contains("<line x1=\"25.066667\" y1=\"24.533333\" x2=\"73.066667\" y2=\"24.533333\""),
            "{s}"
        );

        let s = svg("margin = 0.25\nleftmargin = -0.25\nline right");
        assert!(
            s.contains("width=\"75.2\" height=\"51.2\" viewBox=\"0 0 75.2 51.2\""),
            "{s}"
        );
        assert!(
            s.contains("<line x1=\"1.066667\" y1=\"24.533333\" x2=\"49.066667\" y2=\"24.533333\""),
            "{s}"
        );
    }

    #[test]
    fn move_expands_svg_bounds_like_dpic() {
        let s =
            svg(".PS\nscale=0.25\nline from (0,0) to (1,0)\nmove left 0.4*scale from (0,0)\n.PE");
        assert!(
            s.contains("width=\"425.6\" height=\"3.2\" viewBox=\"0 0 425.6 3.2\""),
            "{s}"
        );
        assert!(
            s.contains("<line x1=\"39.466667\" y1=\"0.533333\" x2=\"423.466667\" y2=\"0.533333\""),
            "{s}"
        );
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
    fn standalone_text_height_sets_svg_font_size_like_dpic() {
        let s = svg("\"\\includegraphics{diagA.eps}\" wid 172/72 ht 54/72");
        assert!(s.contains("font-size=\"81.818182pt\""), "{s}");
        assert!(
            s.contains("stroke-width=\"0.266667\" fill=\"black\" x=\"115.733333\" y=\"72.533333\""),
            "{s}"
        );
        assert!(!s.contains("dominant-baseline"), "{s}");
    }

    #[test]
    fn standalone_text_below_offsets_svg_baseline_like_dpic() {
        let s = svg(".PS\nscale=0.25\nline up 0.05 from (0,0)\n\"0\" below at (0,0)\n.PE");
        assert!(
            s.contains("width=\"3.2\" height=\"34.746667\" viewBox=\"0 0 3.2 34.746667\""),
            "{s}"
        );
        assert!(
            s.contains("stroke-width=\"0.266667\" fill=\"black\" x=\"1.066667\" y=\"32.08\""),
            "{s}"
        );
    }

    #[test]
    fn attached_text_uses_dpic_svg_baseline() {
        let s = svg("box wid .2 \"longlonglong\"");
        assert!(s.contains("font-size=\"11pt\""), "{s}");
        assert!(
            s.contains("stroke-width=\"0.2pt\" fill=\"black\" x=\"10.666667\" y=\"29.373333\""),
            "{s}"
        );
        assert!(!s.contains("dominant-baseline"), "{s}");
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
    fn gradient_fill_emits_linear_gradient_defs() {
        // angle 0: left to right in bounding-box coordinates
        let s = svg("box gradient \"steelblue\" \"white\"");
        assert!(s.contains("<defs><linearGradient id=\"grad0\""), "{s}");
        assert!(s.contains("x1=\"0\" y1=\"0.5\" x2=\"1\" y2=\"0.5\""), "{s}");
        assert!(
            s.contains("<stop offset=\"0\" stop-color=\"steelblue\"/>"),
            "{s}"
        );
        assert!(
            s.contains("<stop offset=\"1\" stop-color=\"white\"/>"),
            "{s}"
        );
        assert!(s.contains("fill=\"url(#grad0)\""), "{s}");

        // angle 90 in pic coordinates = bottom to top = SVG y from 1 to 0
        let s = svg("box gradient \"gold\" \"red\" gradientangle 90");
        assert!(s.contains("x1=\"0.5\" y1=\"1\" x2=\"0.5\" y2=\"0\""), "{s}");

        // classic output carries no gradient defs
        let plain = svg("box\ncircle fill 0.5");
        assert!(!plain.contains("linearGradient"), "{plain}");
    }

    #[test]
    fn gradient_composes_with_hatch_and_opacity() {
        // gradient becomes the hatch pattern background, and fill-opacity
        // still applies to the composed fill
        let s = svg("box gradient \"gold\" \"white\" hatch opacity 0.5");
        assert!(s.contains("<linearGradient id=\"grad1\""), "{s}");
        assert!(s.contains("<pattern id=\"hatch0\""), "{s}");
        assert!(s.contains("fill=\"url(#grad1)\"/>"), "{s}"); // pattern bg rect
        assert!(s.contains("fill=\"url(#hatch0)\""), "{s}"); // shape fill
        assert!(s.contains("fill-opacity=\"0.5\""), "{s}");
    }

    #[test]
    fn class_hook_lands_on_shape_group_and_keeps_ids() {
        let s = svg("box class \"critical hot\"\ncircle");
        assert!(s.contains("<g id=\"s0\" class=\"critical hot\">"), "{s}");
        assert!(s.contains("<g id=\"s1\">\n<circle"), "{s}");

        // classic output stays byte-identical: no class attribute anywhere
        let plain = svg("box\ncircle");
        assert!(!plain.contains("class="), "{plain}");
    }

    #[test]
    fn class_follows_its_shape_through_behind_reordering() {
        // `behind` reorders group emission; the class must stay attached to
        // its own shape id, which is also the animation target contract.
        let s = svg("A: box class \"front\"\nbox class \"back\" behind A at A");
        let back = s.find("<g id=\"s1\" class=\"back\">").unwrap();
        let front = s.find("<g id=\"s0\" class=\"front\">").unwrap();
        assert!(back < front, "{s}");
    }

    #[test]
    fn hatch_fill_emits_svg_pattern() {
        let s = svg("box fill 0.9 crosshatch hatchangle 30 hatchsep .05 hatchwid 1 hatchcolor red");
        assert!(s.contains("<defs><pattern id=\"hatch0\""), "{s}");
        assert!(s.contains("patternUnits=\"userSpaceOnUse\""), "{s}");
        assert!(s.contains("patternTransform=\"rotate(-30)\""), "{s}");
        assert!(s.contains("<rect x=\"0\" y=\"0\""), "{s}");
        assert!(s.contains("fill=\"rgb(230,230,230)\""), "{s}");
        assert!(s.contains("stroke=\"red\""), "{s}");
        assert!(s.contains("fill=\"url(#hatch0)\""), "{s}");
    }

    #[test]
    fn opacity_emits_as_fill_opacity_only() {
        let s = svg("box \"label\" fill 0.8 opacity .4");
        assert!(s.contains("<g id=\"s0\">"), "{s}");
        assert!(s.contains("fill-opacity=\"0.4\""), "{s}");
        assert!(!s.contains(" opacity=\"0.4\""), "{s}");
        assert!(s.contains(">label</text>"), "{s}");
    }

    #[test]
    fn opacity_applies_to_open_path_fill_not_stroke_or_text() {
        let s = svg("line right then up then left then down fill 0.8 opacity .5 \"area\"");
        assert!(s.contains("fill-opacity=\"0.5\""), "{s}");
        assert!(s.contains("fill=\"none\" stroke=\"black\""), "{s}");
        assert!(!s.contains("stroke-opacity"), "{s}");
        assert!(s.contains(">area</text>"), "{s}");
    }

    #[test]
    fn invisible_hatched_open_path_still_sets_svg_bounds() {
        let s = svg("line hatch invis right then up then left then down");
        assert!(
            s.contains("width=\"51.2\" height=\"51.2\" viewBox=\"0 0 51.2 51.2\""),
            "{s}"
        );
        assert!(s.contains("fill=\"url(#hatch0)\""), "{s}");
        assert!(!s.contains("fill=\"none\" stroke=\"black\""), "{s}");
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
