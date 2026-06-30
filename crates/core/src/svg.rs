//! SVG 1.1 backend. Renders a [`Drawing`] to an SVG string.
//!
//! Reference semantics taken from dpic's `svg.c`: y-axis flipped (SVG is
//! y-down), arrowheads emitted as filled polygons, splines as cubic Béziers,
//! gray fills via pic's `0 = black … 1 = white` convention. Coordinates scale
//! by 96 px/inch (dpic's `dpPPI`).

use crate::geom::Point;
use crate::ir::*;

const PPI: f64 = 96.0;
const MARGIN: f64 = 4.0;
const FONT_PT: f64 = 11.0;

/// Render a drawing to an SVG document string.
pub fn to_svg(d: &Drawing) -> String {
    let mut r = Svg::new(d);
    r.render(d);
    r.finish()
}

struct Svg {
    out: String,
    min: Point,
    maxy: f64,
}

impl Svg {
    fn new(d: &Drawing) -> Self {
        let (min, maxy) = if d.bbox.is_empty() {
            (Point::ZERO, 0.0)
        } else {
            (d.bbox.min, d.bbox.max.y)
        };
        Svg {
            out: String::new(),
            min,
            maxy,
        }
    }

    /// Map a pic point to SVG pixel space (y flipped).
    fn p(&self, p: Point) -> Point {
        Point::new(
            (p.x - self.min.x) * PPI + MARGIN,
            (self.maxy - p.y) * PPI + MARGIN,
        )
    }

    fn render(&mut self, d: &Drawing) {
        let (w, h) = if d.bbox.is_empty() {
            (2.0 * MARGIN, 2.0 * MARGIN)
        } else {
            (
                d.bbox.width() * PPI + 2.0 * MARGIN,
                d.bbox.height() * PPI + 2.0 * MARGIN,
            )
        };
        self.out.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" font-family=\"sans-serif\" font-size=\"{}\">\n",
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
                    let tl = self.p(Point::new(c.x - w / 2.0, c.y + h / 2.0));
                    let mut attrs = format!(
                        "x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                        num(tl.x),
                        num(tl.y),
                        num(w * PPI),
                        num(h * PPI)
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
                    let d = self.spline_path(pts, *tension);
                    if style.fill_open {
                        self.out.push_str(&format!(
                            "<path d=\"{}\" fill=\"{}\" stroke-width=\"0\" stroke=\"black\"/>\n",
                            d,
                            self.fill_value(style)
                        ));
                    }
                    if !style.invis {
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
                cw,
                arrows,
                style,
                text,
            } => {
                if !style.invis || style.fill_open {
                    let dir = if a1 >= a0 { 1.0 } else { -1.0 };
                    let trim = if *arrows == Arrowheads::None || *r <= 1e-9 {
                        0.0
                    } else {
                        (style.arrow_ht / *r * 0.45).min((a1 - a0).abs() / 3.0)
                    };
                    let da0 = if matches!(arrows, Arrowheads::Start | Arrowheads::Both) {
                        a0 + dir * trim
                    } else {
                        *a0
                    };
                    let da1 = if matches!(arrows, Arrowheads::End | Arrowheads::Both) {
                        a1 - dir * trim
                    } else {
                        *a1
                    };
                    let start = self.p(*c + Point::new(da0.cos(), da0.sin()) * *r);
                    let end = self.p(*c + Point::new(da1.cos(), da1.sin()) * *r);
                    let large = if (da1 - da0).abs() > std::f64::consts::PI {
                        1
                    } else {
                        0
                    };
                    let sweep = if *cw { 1 } else { 0 };
                    let d = format!(
                        "M {} {} A {} {} 0 {} {} {} {}",
                        num(start.x),
                        num(start.y),
                        num(r * PPI),
                        num(r * PPI),
                        large,
                        sweep,
                        num(end.x),
                        num(end.y)
                    );
                    if style.fill_open {
                        self.out.push_str(&format!(
                            "<path d=\"{}\" fill=\"{}\" stroke-width=\"0\" stroke=\"black\"/>\n",
                            d,
                            self.fill_value(style)
                        ));
                    }
                    if !style.invis {
                        self.out.push_str(&format!(
                            "<path d=\"{}\" fill=\"none\" {}/>\n",
                            d,
                            self.stroke(style)
                        ));
                        self.arc_arrowheads(*c, *r, *a0, *a1, *arrows, style);
                    }
                }
                self.text(*c, text);
            }
            Shape::Text { at, text } => self.text(*at, text),
        }
    }

    // ---- painting ----------------------------------------------------------

    fn path_stroke_points(&self, pts: &[Point], arrows: Arrowheads, style: &Style) -> Vec<Point> {
        let mut out: Vec<Point> = pts.iter().map(|p| self.p(*p)).collect();
        if style.arrow_filled || out.len() < 2 {
            return out;
        }
        let n = out.len();
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
            let mut u = t - f;
            let len = u.len();
            if len < 1e-9 {
                return;
            }
            u = u / len;
            let perp = Point::new(-u.y, u.x);
            let hl = style.arrow_ht * PPI; // arrowht
            let hw = style.arrow_wid / 2.0 * PPI; // half of arrowwid
            let base = t - u * hl;
            let l = base + perp * hw;
            let r = base - perp * hw;
            if style.arrow_filled {
                out.push_str(&format!(
                    "<polygon points=\"{},{} {},{} {},{}\" fill=\"{}\"/>\n",
                    num(t.x),
                    num(t.y),
                    num(l.x),
                    num(l.y),
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

    fn arc_arrowheads(
        &mut self,
        c: Point,
        r: f64,
        a0: f64,
        a1: f64,
        arrows: Arrowheads,
        style: &Style,
    ) {
        if arrows == Arrowheads::None || r <= 1e-9 {
            return;
        }
        let color = attr(&style.stroke.clone().unwrap_or_else(|| "black".into()));
        let dir = if a1 >= a0 { 1.0 } else { -1.0 };
        let max_step = ((a1 - a0).abs() / 3.0).max(1e-6);
        let step = (style.arrow_ht / r * 0.45).clamp(0.02_f64.min(max_step), max_step);
        let point = |t: f64| c + Point::new(t.cos(), t.sin()) * r;
        let head = |tip: Point, from: Point, out: &mut String| {
            let t = self.p(tip);
            let f = self.p(from);
            let mut u = t - f;
            let len = u.len();
            if len < 1e-9 {
                return;
            }
            u = u / len;
            let perp = Point::new(-u.y, u.x);
            let hl = style.arrow_ht * PPI;
            let hw = style.arrow_wid / 2.0 * PPI;
            let base = t - u * hl;
            let l = base + perp * hw;
            let rr = base - perp * hw;
            if style.arrow_filled {
                out.push_str(&format!(
                    "<path stroke-width=\"0\" fill=\"{}\" d=\"M {},{} L {},{} L {},{} Z\"/>\n",
                    color,
                    num(t.x),
                    num(t.y),
                    num(l.x),
                    num(l.y),
                    num(rr.x),
                    num(rr.y)
                ));
            } else {
                let Some((l, p, rr)) = open_arrowhead_points(t, f, style) else {
                    return;
                };
                out.push_str(&format!(
                    "<path d=\"M {} {} L {} {} L {} {}\" fill=\"none\" {}/>\n",
                    num(l.x),
                    num(l.y),
                    num(p.x),
                    num(p.y),
                    num(rr.x),
                    num(rr.y),
                    self.stroke(style)
                ));
            }
        };
        let mut buf = String::new();
        if matches!(arrows, Arrowheads::End | Arrowheads::Both) {
            head(point(a1), point(a1 - dir * step), &mut buf);
        }
        if matches!(arrows, Arrowheads::Start | Arrowheads::Both) {
            head(point(a0), point(a0 + dir * step), &mut buf);
        }
        self.out.push_str(&buf);
    }

    /// dpic-compatible spline path. The control-point construction matches
    /// `dpic`'s SVG backend (verified against `dpic -v` output); since both the
    /// model→SVG transform and the constructions are affine, we build directly
    /// in SVG space.
    fn spline_path(&self, pts: &[Point], tension: Option<f64>) -> String {
        let q: Vec<Point> = pts.iter().map(|p| self.p(*p)).collect();
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
            None => classic_spline(&q),
            Some(t) => tensioned_spline(&q, t),
        }
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
                "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" dominant-baseline=\"central\">{}</text>\n",
                num(x),
                num(y),
                anchor,
                escape(&line.s)
            ));
        }
    }
}

// ---- helpers ---------------------------------------------------------------

fn thick_px(style: &Style) -> f64 {
    // style.thick is in points; default ~0.8pt.
    let pt = style.thick.filter(|t| *t > 0.0).unwrap_or(0.8);
    pt * PPI / 72.0
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

/// Classic pic spline (no tension): a quadratic B-spline that interpolates the
/// first and last control points, passes through the midpoint of every segment,
/// and has straight first/last half-segments. Each cubic is the segment's
/// quadratic Bézier (control = the shared vertex) raised to cubic degree.
fn classic_spline(q: &[Point]) -> String {
    let n = q.len();
    // knots: V0, mid(V0,V1), …, mid(V_{n-2},V_{n-1}), V_{n-1}
    let mut knots = Vec::with_capacity(n + 1);
    knots.push(q[0]);
    for i in 0..n - 1 {
        knots.push((q[i] + q[i + 1]) * 0.5);
    }
    knots.push(q[n - 1]);
    let segs = knots.len() - 1;
    let mut d = format!("M {} {}", num(knots[0].x), num(knots[0].y));
    for j in 0..segs {
        let a = knots[j];
        let b = knots[j + 1];
        // quad control: V0 (first segment → straight), V_{n-1} (last → straight),
        // otherwise the vertex shared by the two flanking midpoints.
        let w = if j == 0 {
            q[0]
        } else if j == segs - 1 {
            q[n - 1]
        } else {
            q[j]
        };
        let c1 = a + (w - a) * (2.0 / 3.0);
        let c2 = b + (w - b) * (2.0 / 3.0);
        push_cubic(&mut d, c1, c2, b);
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
        assert!(s.contains("<ellipse"));
        assert!(s.contains("<rect"));
        assert!(s.contains("<line"));
        assert!(s.contains("<polygon")); // arrowhead
        assert!(s.contains(">document<"));
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
    fn dashed_box_gets_dasharray() {
        let s = svg("box \"x\" dashed");
        assert!(s.contains("stroke-dasharray"));
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
