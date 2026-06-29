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
                if !style.invis {
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
                if !style.invis {
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
                if !style.invis {
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
                if !style.invis && pts.len() >= 2 {
                    let pstr: Vec<String> = pts
                        .iter()
                        .map(|p| {
                            let q = self.p(*p);
                            format!("{},{}", num(q.x), num(q.y))
                        })
                        .collect();
                    self.out.push_str(&format!(
                        "<polyline points=\"{}\" fill=\"none\" {}/>\n",
                        pstr.join(" "),
                        self.stroke(style)
                    ));
                    self.arrowheads(pts, *arrows, style);
                }
                if let Some(c) = midpoint(pts) {
                    self.text(c, text);
                }
            }
            Shape::Spline {
                pts,
                arrows,
                style,
                text,
            } => {
                if !style.invis && pts.len() >= 2 {
                    self.out.push_str(&format!(
                        "<path d=\"{}\" fill=\"none\" {}/>\n",
                        self.spline_path(pts),
                        self.stroke(style)
                    ));
                    self.arrowheads(pts, *arrows, style);
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
                if !style.invis {
                    let start = self.p(*c + Point::new(a0.cos(), a0.sin()) * *r);
                    let end = self.p(*c + Point::new(a1.cos(), a1.sin()) * *r);
                    let large = if (a1 - a0).abs() > std::f64::consts::PI {
                        1
                    } else {
                        0
                    };
                    // y is flipped, so a pic ccw arc sweeps clockwise on screen
                    let sweep = if *cw { 0 } else { 1 };
                    self.out.push_str(&format!(
                        "<path d=\"M {} {} A {} {} 0 {} {} {} {}\" fill=\"none\" {}/>\n",
                        num(start.x),
                        num(start.y),
                        num(r * PPI),
                        num(r * PPI),
                        large,
                        sweep,
                        num(end.x),
                        num(end.y),
                        self.stroke(style)
                    ));
                    // arrowheads oriented along the tangent at each tip: pass
                    // points stepped slightly inward so the head follows the curve
                    let pt = |t: f64| *c + Point::new(t.cos(), t.sin()) * *r;
                    let d = if a1 >= a0 { 0.08 } else { -0.08 };
                    self.arrowheads(&[pt(*a0), pt(a0 + d), pt(a1 - d), pt(*a1)], *arrows, style);
                }
                self.text(*c, text);
            }
            Shape::Text { at, text } => self.text(*at, text),
        }
    }

    // ---- painting ----------------------------------------------------------

    fn stroke(&self, style: &Style) -> String {
        let color = attr(&style.stroke.clone().unwrap_or_else(|| "black".into()));
        let mut s = format!(
            "stroke=\"{}\" stroke-width=\"{}\"",
            color,
            num(thick_px(style))
        );
        match style.dash {
            Dash::Solid => {}
            Dash::Dashed => s.push_str(" stroke-dasharray=\"4,3\""),
            Dash::Dotted => s.push_str(" stroke-dasharray=\"1,3\" stroke-linecap=\"round\""),
        }
        s
    }

    /// stroke + fill for closed shapes.
    fn paint(&self, style: &Style) -> String {
        let fill = match &style.fill {
            None => "none".to_string(),
            Some(Fill::Gray(g)) => {
                let v = (g.clamp(0.0, 1.0) * 255.0).round() as u32;
                format!("rgb({v},{v},{v})")
            }
            Some(Fill::Color(c)) => attr(c),
        };
        format!("fill=\"{}\" {}", fill, self.stroke(style))
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
                // open arrowhead: two strokes meeting at the tip
                out.push_str(&format!(
                    "<polyline points=\"{},{} {},{} {},{}\" fill=\"none\" {}/>\n",
                    num(l.x),
                    num(l.y),
                    num(t.x),
                    num(t.y),
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

    fn spline_path(&self, pts: &[Point]) -> String {
        // Catmull-Rom through the points, converted to cubic Béziers.
        let q: Vec<Point> = pts.iter().map(|p| self.p(*p)).collect();
        let n = q.len();
        let mut d = format!("M {} {}", num(q[0].x), num(q[0].y));
        for i in 0..n - 1 {
            let p0 = q[i.saturating_sub(1)];
            let p1 = q[i];
            let p2 = q[i + 1];
            let p3 = q[(i + 2).min(n - 1)];
            let c1 = p1 + (p2 - p0) * (1.0 / 6.0);
            let c2 = p2 - (p3 - p1) * (1.0 / 6.0);
            d.push_str(&format!(
                " C {} {} {} {} {} {}",
                num(c1.x),
                num(c1.y),
                num(c2.x),
                num(c2.y),
                num(p2.x),
                num(p2.y)
            ));
        }
        d
    }

    fn text(&mut self, center: Point, lines: &[TextLine]) {
        if lines.is_empty() {
            return;
        }
        let c = self.p(center);
        let lh = FONT_PT * PPI / 72.0 * 1.2;
        let n = lines.len() as f64;
        for (i, line) in lines.iter().enumerate() {
            let dy = (i as f64 - (n - 1.0) / 2.0) * lh;
            let anchor = match line.halign {
                -1 => "start",
                1 => "end",
                _ => "middle",
            };
            let y = c.y + dy - (line.valign as f64) * lh;
            self.out.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" dominant-baseline=\"central\">{}</text>\n",
                num(c.x),
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

fn midpoint(pts: &[Point]) -> Option<Point> {
    if pts.is_empty() {
        None
    } else {
        Some((pts[0] + pts[pts.len() - 1]) * 0.5)
    }
}

/// Format a float compactly (up to 3 decimals, no trailing zeros). Non-finite
/// values (NaN/Inf, e.g. from a zero-length element) become `0` so the SVG stays
/// well-formed instead of emitting a literal `NaN`.
fn num(x: f64) -> String {
    if !x.is_finite() {
        return "0".to_string();
    }
    let r = (x * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r }; // normalise -0
    let mut s = format!("{r:.3}");
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

    #[test]
    fn pipeline_svg_has_elements() {
        let s = svg(".PS\nellipse \"document\"\narrow\nbox \"PIC\"\n.PE");
        assert!(s.starts_with("<svg"));
        assert!(s.contains("<ellipse"));
        assert!(s.contains("<rect"));
        assert!(s.contains("<polyline"));
        assert!(s.contains("<polygon")); // arrowhead
        assert!(s.contains(">document<"));
        assert!(s.contains("</svg>"));
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
    fn xml_is_escaped() {
        let s = svg("box \"a < b & c\"");
        assert!(s.contains("a &lt; b &amp; c"));
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
