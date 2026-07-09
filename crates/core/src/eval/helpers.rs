//! Free helper functions for the evaluator (pure; no `State`).

use super::*;

// ---- free helpers ----------------------------------------------------------

pub(super) fn apply_op(op: AssignOp, cur: f64, rhs: f64) -> ER<f64> {
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
pub(super) struct GlibcRand {
    state: [u32; 31],
    f: usize,
    r: usize,
}

impl GlibcRand {
    pub(super) fn new(seed: i64) -> Self {
        let mut rng = GlibcRand {
            state: [0; 31],
            f: 3,
            r: 0,
        };
        rng.seed(seed);
        rng
    }

    pub(super) fn seed(&mut self, seed: i64) {
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

    pub(super) fn next_i31(&mut self) -> u32 {
        let x = self.state[self.f].wrapping_add(self.state[self.r]);
        self.state[self.f] = x;
        let out = (x >> 1) & 0x7fff_ffff;
        self.f = (self.f + 1) % 31;
        self.r = (self.r + 1) % 31;
        out
    }

    pub(super) fn next_f64(&mut self) -> f64 {
        self.next_i31() as f64 / 2_147_483_647.0
    }
}

pub(super) fn bool_f(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

pub(super) fn eval_numeric_bin(op: BinOp, x: f64, y: f64) -> ER<f64> {
    Ok(match op {
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
    })
}

pub(super) fn dpic_round(x: f64) -> i64 {
    if x < 0.0 {
        -((-x + 0.5).floor() as i64)
    } else {
        (x + 0.5).floor() as i64
    }
}

pub(super) fn dpic_mod(x: f64, y: f64, zero_msg: &'static str) -> ER<f64> {
    let i = dpic_round(x);
    let j = dpic_round(y);
    if j == 0 {
        return err(zero_msg);
    }
    Ok((i - (i / j) * j) as f64)
}

pub(super) fn dpic_pow(x: f64, y: f64) -> ER<f64> {
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

pub(super) fn dpic_int_pow(x: f64, y: i64) -> f64 {
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

pub(super) fn apply_text_pos(halign: &mut i8, valign: &mut i8, pos: token::TextPos) {
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

pub(super) fn ensure_hatch(style: &mut Style) -> &mut Hatch {
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
pub(super) fn validate_class(name: &str) -> ER<()> {
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

pub(super) fn ensure_gradient(style: &mut Style) -> &mut Gradient {
    style.gradient.get_or_insert_with(|| Gradient {
        from: "black".into(),
        to: "white".into(),
        angle: 0.0,
    })
}

pub(super) fn has_visible_text(lines: &[TextLine]) -> bool {
    lines.iter().any(|line| !line.s.is_empty())
}

pub(super) fn closed_shape_is_visible(style: &Style) -> bool {
    !style.invis || style.fill.is_some() || style.hatch.is_some() || style.gradient.is_some()
}

pub(super) fn open_fill_is_visible(style: &Style) -> bool {
    style.fill_open && (style.fill.is_some() || style.hatch.is_some() || style.gradient.is_some())
}

pub(super) fn stroke_half_width(style: &Style) -> f64 {
    if style.invis {
        0.0
    } else {
        style.thick.unwrap_or(0.8) / 144.0
    }
}

pub(super) fn painted_bbox(bb: &Bbox, pad: f64) -> Bbox {
    if bb.is_empty() || pad <= 0.0 {
        return *bb;
    }
    let mut out = Bbox::new();
    out.add(bb.min - Point::new(pad, pad));
    out.add(bb.max + Point::new(pad, pad));
    out
}

pub(super) fn dpic_box_layout_bbox(center: Point, w: f64, h: f64) -> Bbox {
    pub(super) fn axis(center: f64, extent: f64) -> (f64, f64) {
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

pub(super) fn drawing_painted_bbox(shapes: &[Shape]) -> Bbox {
    let mut out = Bbox::new();
    for sh in shapes {
        out.union(&shape_painted_bbox(sh));
    }
    out
}

pub(super) fn shape_painted_bbox(sh: &Shape) -> Bbox {
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

pub(super) fn brace_side(unit: Point, side_dir: Option<Dir>) -> Point {
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

pub(super) fn brace_cubics(a: Point, b: Point, depth: Point, pos: f64) -> Vec<[Point; 4]> {
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

pub(super) fn brace_cusp(cubics: &[[Point; 4]]) -> Option<Point> {
    cubics.get(2).map(|c| c[3])
}

pub(super) fn cubic_at(c: &[Point; 4], t: f64) -> Point {
    let mt = 1.0 - t;
    c[0] * (mt * mt * mt)
        + c[1] * (3.0 * mt * mt * t)
        + c[2] * (3.0 * mt * t * t)
        + c[3] * (t * t * t)
}

pub(super) fn sample_cubics(cubics: &[[Point; 4]], steps: usize) -> Vec<Point> {
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

pub(super) fn cubics_bbox(cubics: &[[Point; 4]]) -> Bbox {
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
pub(super) struct PendingStyle {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) family: Option<String>,
    pub(super) size_pt: Option<f64>,
    pub(super) rotate: Option<f64>,
    pub(super) aligned: bool,
}

/// Line advance width: exact metrics for typeset math, the classic
/// 0.6 em/char estimate otherwise (scaled by the line's font style).
pub(super) fn text_line_width(line: &TextLine) -> f64 {
    line.ink_width_in()
}

pub(super) fn text_bbox(center: Point, lines: &[TextLine]) -> Bbox {
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
                    // the SVG backend rotates about the text anchor `(x, y)`
                    // (the halign edge), not the rect centre — match it (#audit)
                    Some(deg) => bb.add_rect_rotated_about(min, max, Point::new(x, y), deg),
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

pub(super) fn fitted_text_size(lines: &[TextLine]) -> Option<(f64, f64)> {
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

pub(super) fn text_object_bbox(center: Point, lines: &[TextLine], w: f64, h: f64) -> Bbox {
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

pub(super) fn text_object_vertical_bbox(center: Point, lines: &[TextLine], h: f64) -> Bbox {
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
pub(super) fn fmt_num(v: f64) -> String {
    format!("{v}")
}

pub(super) fn install_dpic_compat_vars(vars: &mut HashMap<String, f64>) {
    // dpic backend option constants are ONE-based (oracle: `dpic -v` prints
    // optMFpic=1 … optSVG=9 … optxfig=12), matching the branch order of the
    // dpic suite's own `case(dpicopt, ...)` dispatch. rpic renders SVG.
    let opts = [
        ("optMFpic", 1.0),
        ("optMpost", 2.0),
        ("optPDF", 3.0),
        ("optPGF", 4.0),
        ("optPict2e", 5.0),
        ("optPS", 6.0),
        ("optPSfrag", 7.0),
        ("optPSTricks", 8.0),
        ("optSVG", 9.0),
        ("optTeX", 10.0),
        ("opttTeX", 11.0),
        ("optxfig", 12.0),
    ];
    for (name, val) in opts {
        vars.insert(name.to_string(), val);
    }
    vars.insert("dpicopt".to_string(), 9.0);
}

/// Map a linear-style `.start`/`.end` anchor to the box/ellipse compass
/// corner it means for an object flowing in `dir` (entry vs exit edge), so
/// `with .start at …` / `with .end at …` edge-align closed objects the way
/// pikchr does — and consistently with the read path (`box_corner`, which
/// returns the stored `self.start`/`self.end`). Other corners pass through.
pub(super) fn dir_start_end_corner(c: Corner, dir: Dir) -> Corner {
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
pub(super) fn readable_angle(mut deg: f64) -> f64 {
    while deg > 90.0 {
        deg -= 180.0;
    }
    while deg <= -90.0 {
        deg += 180.0;
    }
    deg
}

pub(super) fn corner_offset(c: Corner, w: f64, h: f64) -> Point {
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

pub(super) fn closed_corner_offset(p: Prim, c: Corner, w: f64, h: f64, rad: f64) -> Point {
    match p {
        Prim::Circle | Prim::Ellipse => ellipse_corner_offset(c, w, h),
        Prim::Box => box_corner_offset(c, w, h, rad),
        _ => corner_offset(c, w, h),
    }
}

pub(super) fn arc_angles(center: Point, start: Point, end: Point, cw: bool) -> (f64, f64) {
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

pub(super) fn ellipse_corner_offset(c: Corner, w: f64, h: f64) -> Point {
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

pub(super) fn box_corner_offset(c: Corner, w: f64, h: f64, rad: f64) -> Point {
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
pub(super) fn primobj_kind(o: &PrimObj) -> Option<PKind> {
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

pub(super) fn want_name(k: PKind) -> &'static str {
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

pub(super) fn nearest_dir(v: Point) -> Dir {
    if v.x.abs() >= v.y.abs() {
        if v.x >= 0.0 { Dir::Right } else { Dir::Left }
    } else if v.y >= 0.0 {
        Dir::Up
    } else {
        Dir::Down
    }
}

pub(super) enum PrintfArg {
    Num(f64),
    Str(String),
}

impl PrintfArg {
    pub(super) fn num(&self) -> f64 {
        match self {
            PrintfArg::Num(v) => *v,
            PrintfArg::Str(s) => s.parse::<f64>().unwrap_or(0.0),
        }
    }

    pub(super) fn string(&self) -> String {
        match self {
            PrintfArg::Num(v) => fmt_num(*v),
            PrintfArg::Str(s) => s.clone(),
        }
    }
}

/// Minimal printf-style formatter supporting `%d %i %f %e %g %s %%` with
/// optional `.precision`. Width/flags are accepted but ignored.
pub(super) fn sprintf_fmt(fmt: &str, vals: &[PrintfArg]) -> String {
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
        // Clamp precision: it flows straight into `format!("{:.*}", …)`, so an
        // enormous value (`"%.999999999f"`) would allocate gigabytes and abort
        // (#284). 512 fractional digits is far past f64's ~17 significant
        // figures — no real format needs more.
        const MAX_PREC: usize = 512;
        let prec = spec.split('.').nth(1).and_then(|p| {
            p.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<usize>()
                .ok()
                .map(|n| n.min(MAX_PREC))
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

pub(super) fn stringexpr_lit(se: &StringExpr) -> String {
    match se {
        StringExpr::Lit(s) => s.clone(),
        StringExpr::Concat(a, b) => format!("{}{}", stringexpr_lit(a), stringexpr_lit(b)),
        StringExpr::Arg(n) => format!("${n}"),
        StringExpr::Sprintf(fmt, _) => stringexpr_lit(fmt),
        StringExpr::SvgFont(_) => String::new(),
        StringExpr::Rgb(_) | StringExpr::ColorNum(_) => String::new(),
    }
}

/// A numeric colour (`0xRRGGBB`, pikchr-style) formatted as `#rrggbb`.
pub(super) fn num_to_color(v: f64) -> ER<String> {
    if !v.is_finite() || v.round() < 0.0 || v.round() > 0xFFFFFF as f64 {
        return err("numeric color out of range 0-0xFFFFFF");
    }
    Ok(format!("#{:06x}", v.round() as u32))
}

/// Normalise a colour *string* so an easy-to-mistype hex form doesn't sail
/// through to invalid SVG: `0xRRGGBB` / `0xRGB` (the bare literal works, but the
/// quoted string didn't) becomes `#rrggbb`, and a dvips/xcolor name browsers
/// can't render (`Dandelion`) becomes its RGB. Everything else — CSS names,
/// `#rrggbb`, `rgb(...)` output — is passed through unchanged.
pub(super) fn normalize_color_string(s: String) -> String {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
        && (hex.len() == 6 || hex.len() == 3)
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return format!("#{}", hex.to_ascii_lowercase());
    }
    if let Some(hex) = crate::color::xcolor_hex(&s) {
        return hex.to_string();
    }
    s
}

pub(super) fn unsafe_svg_colour_reason(color: &str) -> Option<&'static str> {
    let lower = color.to_ascii_lowercase();
    if has_css_function_named(&lower, "url") {
        return Some("colour values must not use url(...) paint servers");
    }
    if has_css_function_named(&lower, "var") {
        return Some("colour values must not use CSS variables");
    }
    if crate::color::is_valid_color(color) {
        return None;
    }
    leading_css_function_name(color)
        .is_some()
        .then_some("unsupported CSS colour function in colour value")
}

pub(super) fn has_css_function_named(s: &str, name: &str) -> bool {
    let bytes = s.as_bytes();
    let name = name.as_bytes();
    let mut i = 0;
    while i + name.len() <= bytes.len() {
        if bytes[i..].starts_with(name) && (i == 0 || !is_css_ident_byte(bytes[i - 1])) && {
            let mut j = i + name.len();
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            j < bytes.len() && bytes[j] == b'('
        } {
            return true;
        }
        i += 1;
    }
    false
}

pub(super) fn leading_css_function_name(s: &str) -> Option<&str> {
    let s = s.trim_start();
    let mut end = 0;
    let mut has_alpha = false;
    for (idx, ch) in s.char_indices() {
        if idx == 0 && !(ch.is_ascii_alphabetic() || ch == '-') {
            return None;
        }
        if ch.is_ascii_alphabetic() || ch == '-' {
            has_alpha |= ch.is_ascii_alphabetic();
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 || !has_alpha {
        return None;
    }
    s[end..].trim_start().starts_with('(').then_some(&s[..end])
}

pub(super) fn is_css_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')
}

pub(super) fn single_token_macro_string(toks: &[crate::lexer::Spanned]) -> Option<String> {
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

pub(super) fn unescape_exec_source(src: &str) -> String {
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
pub(super) fn rebase_placed(pl: &mut Placed, d: Point, shape_off: usize) {
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

pub(super) fn scale_placed(pl: &mut Placed, f: f64) {
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

pub(super) fn translate_shape(sh: &mut Shape, d: Point) {
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

pub(super) fn scale_bbox_in_place(bb: &mut Bbox, f: f64) {
    if bb.is_empty() {
        return;
    }
    let mut b = Bbox::new();
    b.add(bb.min * f);
    b.add(bb.max * f);
    *bb = b;
}

pub(super) fn scale_style(style: &mut Style, f: f64) {
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

pub(super) fn multiply_shape_fill_opacity(sh: &mut Shape, opacity: f64) {
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
pub(super) fn scale_shape(sh: &mut Shape, f: f64) {
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

pub(super) fn object_uses_bare_distance(kind: &ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::Primitive(Prim::Line | Prim::Arrow | Prim::Move | Prim::Spline)
            | ObjectKind::Brace
            | ObjectKind::Continue
    )
}

pub(super) fn expr_bare_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Var(name, None) => Some(name.as_str()),
        _ => None,
    }
}

pub(super) fn suggest_attribute(word: &str) -> Option<&'static str> {
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
