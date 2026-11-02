//! Scene export: the resolved draw list as versioned, device-independent JSON.
//!
//! This is a *second reader* of the same [`Drawing`] the SVG backend consumes
//! (see `docs/scene-internals.md`): coordinates are final model inches, y-up;
//! curves are serialized as cubic Béziers using the exact constructions the
//! SVG `d` attributes carry; style is descriptors, text stays text. The format
//! is specified in `docs/scene.md` (version 1) and nothing here touches the
//! render backends — computing a scene is opt-in and the default outputs stay
//! byte-identical.

use crate::diagnostic::{Diagnostic, Span};
use crate::geom::Point;
use crate::ir::*;
use crate::svg;

/// Format version emitted in the `scene` field. Bump only on a breaking
/// change (rename/removal/semantic change of a field) — see docs/scene.md §7.
pub const SCENE_VERSION: u32 = 1;

/// Options for the scene export.
#[derive(Debug, Clone, Default)]
pub struct SceneOptions {
    /// Flatten every cubic path into a polyline within this tolerance
    /// (inches). `None` keeps the cubics.
    pub flatten: Option<f64>,
    /// Embed typeset math (`texlabels`) as self-contained SVG glyph fragments
    /// (`outline` keys). Off, math lines carry only their literal + metrics
    /// and raise a `scene_unsupported` warning.
    pub outline: bool,
    /// The source's display name for the scene header (`source` key); the CLI
    /// passes the input file name.
    pub source: Option<String>,
}

/// A built scene: the JSON document plus the `scene_unsupported` warnings the
/// build raised (empty for the vast majority of drawings).
#[derive(Debug, Clone)]
pub struct SceneOutput {
    pub json: String,
    pub warnings: Vec<Diagnostic>,
}

/// Build the scene JSON for a compiled drawing.
pub fn scene_json(d: &Drawing, opts: &SceneOptions) -> SceneOutput {
    let mut sc = Sc {
        d,
        opts,
        out: String::with_capacity(4096),
        warnings: Vec::new(),
    };
    sc.build();
    SceneOutput {
        json: sc.out,
        warnings: sc.warnings,
    }
}

struct Sc<'a> {
    d: &'a Drawing,
    opts: &'a SceneOptions,
    out: String,
    warnings: Vec<Diagnostic>,
}

// ---- number / string formatting ---------------------------------------------

/// Scene coordinate: 6 decimals, trimmed — the same rounding the SVG uses.
fn snum(x: f64) -> String {
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

/// Full-precision value (the `vars` map): Rust's shortest round-trip decimal.
fn svar(x: f64) -> String {
    if !x.is_finite() {
        return "0".to_string();
    }
    let s = format!("{x}");
    // f64 Display never yields exponents ≥ 1e16 magnitudes as "inf"; it can
    // still print integers without a dot — valid JSON either way.
    s
}

fn spoint(p: Point) -> String {
    format!("[{},{}]", snum(p.x), snum(p.y))
}

fn spoints(pts: &[Point]) -> String {
    let mut s = String::from("[");
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&spoint(*p));
    }
    s.push(']');
    s
}

// ---- cubic paths -------------------------------------------------------------

/// One subpath: either a cubic chain or a polyline (inherently, or flattened).
struct ScenePath {
    closed: bool,
    start: Point,
    cubics: Vec<[Point; 3]>,
    points: Vec<Point>,
}

impl ScenePath {
    fn from_cubics(start: Point, cubics: Vec<[Point; 3]>, closed: bool) -> Self {
        ScenePath {
            closed,
            start,
            cubics,
            points: Vec::new(),
        }
    }

    fn from_points(points: Vec<Point>, closed: bool) -> Self {
        ScenePath {
            closed,
            start: Point::ZERO,
            cubics: Vec::new(),
            points,
        }
    }

    fn flattened(self, tol: f64) -> Self {
        if self.cubics.is_empty() {
            return self;
        }
        let mut pts = vec![self.start];
        let mut cur = self.start;
        for c in &self.cubics {
            flatten_cubic(cur, c[0], c[1], c[2], tol, 0, &mut pts);
            cur = c[2];
        }
        ScenePath::from_points(pts, self.closed)
    }

    fn json(&self) -> String {
        let mut s = String::from("{");
        if self.closed {
            s.push_str("\"closed\":true,");
        }
        if self.points.is_empty() {
            s.push_str(&format!(
                "\"start\":{},\"cubics\":[",
                spoint(self.start)
            ));
            for (i, c) in self.cubics.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&format!(
                    "[{},{},{}]",
                    spoint(c[0]),
                    spoint(c[1]),
                    spoint(c[2])
                ));
            }
            s.push(']');
        } else {
            s.push_str(&format!("\"points\":{}", spoints(&self.points)));
        }
        s.push('}');
        s
    }
}

/// Adaptive De Casteljau flattening: subdivide until both control points sit
/// within `tol` of the chord.
fn flatten_cubic(p0: Point, c1: Point, c2: Point, p3: Point, tol: f64, depth: u32, out: &mut Vec<Point>) {
    const MAX_DEPTH: u32 = 24;
    if depth >= MAX_DEPTH || cubic_flat(p0, c1, c2, p3, tol) {
        out.push(p3);
        return;
    }
    // split at t = 1/2
    let m01 = (p0 + c1) * 0.5;
    let m12 = (c1 + c2) * 0.5;
    let m23 = (c2 + p3) * 0.5;
    let m012 = (m01 + m12) * 0.5;
    let m123 = (m12 + m23) * 0.5;
    let mid = (m012 + m123) * 0.5;
    flatten_cubic(p0, m01, m012, mid, tol, depth + 1, out);
    flatten_cubic(mid, m123, m23, p3, tol, depth + 1, out);
}

fn cubic_flat(p0: Point, c1: Point, c2: Point, p3: Point, tol: f64) -> bool {
    dist_to_segment(c1, p0, p3) <= tol && dist_to_segment(c2, p0, p3) <= tol
}

fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 <= 1e-24 {
        return p.dist(a);
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0);
    p.dist(a + ab * t)
}

/// A line segment as a degenerate cubic (collinear control points).
fn line_cubic(from: Point, to: Point) -> [Point; 3] {
    let d = to - from;
    [from + d * (1.0 / 3.0), from + d * (2.0 / 3.0), to]
}

/// Cubic approximation of a circular arc about `c` from angle `t0` sweeping
/// `da` radians (signed, CCW positive — model space), radius `r`. Slices of
/// ≤ 90° with the standard `k = 4/3·tan(Δ/4)` tangent handles.
fn arc_cubics(c: Point, r: f64, t0: f64, da: f64, out: &mut Vec<[Point; 3]>) {
    if r.abs() < 1e-12 || da.abs() < 1e-12 {
        return;
    }
    let n = ((da.abs() / std::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
    let step = da / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    for i in 0..n {
        let a0 = t0 + step * i as f64;
        let a1 = a0 + step;
        let p0 = c + Point::new(a0.cos(), a0.sin()) * r;
        let p1 = c + Point::new(a1.cos(), a1.sin()) * r;
        let c1 = p0 + Point::new(-a0.sin(), a0.cos()) * (k * r);
        let c2 = p1 - Point::new(-a1.sin(), a1.cos()) * (k * r);
        out.push([c1, c2, p1]);
    }
}

/// Cubics for a minor arc (≤ π) given endpoints, radius and direction — the
/// model-space mirror of the SVG backend's `arc_to` pieces (whose `ccw > 0`
/// maps to sweep 0, i.e. counter-clockwise in y-up model space).
fn minor_arc_cubics(from: Point, to: Point, r: f64, ccw: bool, out: &mut Vec<[Point; 3]>) {
    let chord = to - from;
    let half2 = (chord.x * chord.x + chord.y * chord.y) / 4.0;
    if half2 <= 1e-24 {
        return; // coincident endpoints: the SVG draws nothing either
    }
    if r.abs() < 1e-12 {
        out.push(line_cubic(from, to));
        return;
    }
    let h = (r * r - half2).max(0.0).sqrt();
    let mid = (from + to) * 0.5;
    let len = (half2 * 4.0).sqrt();
    let n = Point::new(-chord.y / len, chord.x / len); // rot90 CCW of unit chord
    let c = if ccw { mid + n * h } else { mid - n * h };
    let t0 = (from - c).y.atan2((from - c).x);
    let t1 = (to - c).y.atan2((to - c).x);
    let mut da = t1 - t0;
    // minor arc in the requested direction
    while da <= -std::f64::consts::PI {
        da += 2.0 * std::f64::consts::PI;
    }
    while da > std::f64::consts::PI {
        da -= 2.0 * std::f64::consts::PI;
    }
    if (ccw && da < 0.0) || (!ccw && da > 0.0) {
        da = -da; // numerical guard; direction is fixed by the center choice
    }
    arc_cubics(c, r.abs(), t0, da, out);
}

// ---- color normalization -----------------------------------------------------

/// Normalize a resolved style color to `#rrggbb`. `Err` = not reducible.
fn hex_color(s: &str) -> Result<String, ()> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix('#') {
        if hex.len() == 3 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let mut out = String::from("#");
            for ch in hex.chars() {
                let c = ch.to_ascii_lowercase();
                out.push(c);
                out.push(c);
            }
            return Ok(out);
        }
        if (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(t.to_ascii_lowercase());
        }
        return Err(());
    }
    if let Some(body) = t
        .strip_prefix("rgb(")
        .and_then(|b| b.strip_suffix(')'))
    {
        let parts: Vec<_> = body.split(',').map(str::trim).collect();
        if parts.len() == 3
            && let (Ok(r), Ok(g), Ok(b)) = (
                parts[0].parse::<u32>(),
                parts[1].parse::<u32>(),
                parts[2].parse::<u32>(),
            )
            && r <= 255
            && g <= 255
            && b <= 255
        {
            return Ok(format!("#{r:02x}{g:02x}{b:02x}"));
        }
        return Err(());
    }
    let lower = t.to_ascii_lowercase();
    CSS_HEX
        .binary_search_by(|(name, _)| name.cmp(&lower.as_str()))
        .map(|i| CSS_HEX[i].1.to_string())
        .map_err(|_| ())
}

/// The CSS Color Module Level 4 extended keywords, sorted, with their sRGB
/// values — the names `color.rs` validates. Used only to *normalize* scene
/// colors; the SVG backend keeps emitting the author's spelling.
static CSS_HEX: &[(&str, &str)] = &[
    ("aliceblue", "#f0f8ff"),
    ("antiquewhite", "#faebd7"),
    ("aqua", "#00ffff"),
    ("aquamarine", "#7fffd4"),
    ("azure", "#f0ffff"),
    ("beige", "#f5f5dc"),
    ("bisque", "#ffe4c4"),
    ("black", "#000000"),
    ("blanchedalmond", "#ffebcd"),
    ("blue", "#0000ff"),
    ("blueviolet", "#8a2be2"),
    ("brown", "#a52a2a"),
    ("burlywood", "#deb887"),
    ("cadetblue", "#5f9ea0"),
    ("chartreuse", "#7fff00"),
    ("chocolate", "#d2691e"),
    ("coral", "#ff7f50"),
    ("cornflowerblue", "#6495ed"),
    ("cornsilk", "#fff8dc"),
    ("crimson", "#dc143c"),
    ("cyan", "#00ffff"),
    ("darkblue", "#00008b"),
    ("darkcyan", "#008b8b"),
    ("darkgoldenrod", "#b8860b"),
    ("darkgray", "#a9a9a9"),
    ("darkgreen", "#006400"),
    ("darkgrey", "#a9a9a9"),
    ("darkkhaki", "#bdb76b"),
    ("darkmagenta", "#8b008b"),
    ("darkolivegreen", "#556b2f"),
    ("darkorange", "#ff8c00"),
    ("darkorchid", "#9932cc"),
    ("darkred", "#8b0000"),
    ("darksalmon", "#e9967a"),
    ("darkseagreen", "#8fbc8f"),
    ("darkslateblue", "#483d8b"),
    ("darkslategray", "#2f4f4f"),
    ("darkslategrey", "#2f4f4f"),
    ("darkturquoise", "#00ced1"),
    ("darkviolet", "#9400d3"),
    ("deeppink", "#ff1493"),
    ("deepskyblue", "#00bfff"),
    ("dimgray", "#696969"),
    ("dimgrey", "#696969"),
    ("dodgerblue", "#1e90ff"),
    ("firebrick", "#b22222"),
    ("floralwhite", "#fffaf0"),
    ("forestgreen", "#228b22"),
    ("fuchsia", "#ff00ff"),
    ("gainsboro", "#dcdcdc"),
    ("ghostwhite", "#f8f8ff"),
    ("gold", "#ffd700"),
    ("goldenrod", "#daa520"),
    ("gray", "#808080"),
    ("green", "#008000"),
    ("greenyellow", "#adff2f"),
    ("grey", "#808080"),
    ("honeydew", "#f0fff0"),
    ("hotpink", "#ff69b4"),
    ("indianred", "#cd5c5c"),
    ("indigo", "#4b0082"),
    ("ivory", "#fffff0"),
    ("khaki", "#f0e68c"),
    ("lavender", "#e6e6fa"),
    ("lavenderblush", "#fff0f5"),
    ("lawngreen", "#7cfc00"),
    ("lemonchiffon", "#fffacd"),
    ("lightblue", "#add8e6"),
    ("lightcoral", "#f08080"),
    ("lightcyan", "#e0ffff"),
    ("lightgoldenrodyellow", "#fafad2"),
    ("lightgray", "#d3d3d3"),
    ("lightgreen", "#90ee90"),
    ("lightgrey", "#d3d3d3"),
    ("lightpink", "#ffb6c1"),
    ("lightsalmon", "#ffa07a"),
    ("lightseagreen", "#20b2aa"),
    ("lightskyblue", "#87cefa"),
    ("lightslategray", "#778899"),
    ("lightslategrey", "#778899"),
    ("lightsteelblue", "#b0c4de"),
    ("lightyellow", "#ffffe0"),
    ("lime", "#00ff00"),
    ("limegreen", "#32cd32"),
    ("linen", "#faf0e6"),
    ("magenta", "#ff00ff"),
    ("maroon", "#800000"),
    ("mediumaquamarine", "#66cdaa"),
    ("mediumblue", "#0000cd"),
    ("mediumorchid", "#ba55d3"),
    ("mediumpurple", "#9370db"),
    ("mediumseagreen", "#3cb371"),
    ("mediumslateblue", "#7b68ee"),
    ("mediumspringgreen", "#00fa9a"),
    ("mediumturquoise", "#48d1cc"),
    ("mediumvioletred", "#c71585"),
    ("midnightblue", "#191970"),
    ("mintcream", "#f5fffa"),
    ("mistyrose", "#ffe4e1"),
    ("moccasin", "#ffe4b5"),
    ("navajowhite", "#ffdead"),
    ("navy", "#000080"),
    ("oldlace", "#fdf5e6"),
    ("olive", "#808000"),
    ("olivedrab", "#6b8e23"),
    ("orange", "#ffa500"),
    ("orangered", "#ff4500"),
    ("orchid", "#da70d6"),
    ("palegoldenrod", "#eee8aa"),
    ("palegreen", "#98fb98"),
    ("paleturquoise", "#afeeee"),
    ("palevioletred", "#db7093"),
    ("papayawhip", "#ffefd5"),
    ("peachpuff", "#ffdab9"),
    ("peru", "#cd853f"),
    ("pink", "#ffc0cb"),
    ("plum", "#dda0dd"),
    ("powderblue", "#b0e0e6"),
    ("purple", "#800080"),
    ("rebeccapurple", "#663399"),
    ("red", "#ff0000"),
    ("rosybrown", "#bc8f8f"),
    ("royalblue", "#4169e1"),
    ("saddlebrown", "#8b4513"),
    ("salmon", "#fa8072"),
    ("sandybrown", "#f4a460"),
    ("seagreen", "#2e8b57"),
    ("seashell", "#fff5ee"),
    ("sienna", "#a0522d"),
    ("silver", "#c0c0c0"),
    ("skyblue", "#87ceeb"),
    ("slateblue", "#6a5acd"),
    ("slategray", "#708090"),
    ("slategrey", "#708090"),
    ("snow", "#fffafa"),
    ("springgreen", "#00ff7f"),
    ("steelblue", "#4682b4"),
    ("tan", "#d2b48c"),
    ("teal", "#008080"),
    ("thistle", "#d8bfd8"),
    ("tomato", "#ff6347"),
    ("turquoise", "#40e0d0"),
    ("violet", "#ee82ee"),
    ("wheat", "#f5deb3"),
    ("white", "#ffffff"),
    ("whitesmoke", "#f5f5f5"),
    ("yellow", "#ffff00"),
    ("yellowgreen", "#9acd32"),
];

// ---- the builder -------------------------------------------------------------

impl<'a> Sc<'a> {
    fn build(&mut self) {
        let d = self.d;
        let raw = d
            .canvas
            .unwrap_or_else(|| svg::drawing_svg_bounds(&d.shapes));
        let pad = d.prelude_thick.max(0.0) / 144.0;
        let m = d.canvas_margin;
        let (west, north, raw_w, raw_h) = if raw.is_empty() {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (raw.min.x, raw.max.y, raw.width(), raw.height())
        };
        let w = svg::positive_extent(raw_w + 6.0 * pad + m.horizontal());
        let h = svg::positive_extent(raw_h + 6.0 * pad + m.vertical());
        // where the pic origin sits, from the canvas lower-left corner — the
        // inputs of the SVG transform, so svg_px = ((x+ox)·96, (h−(y+oy))·96)
        let ox = 2.0 * pad + m.left - west;
        let oy = h - north - pad - m.top;

        self.out.push_str(&format!(
            "{{\"scene\":{SCENE_VERSION},\"units\":\"in\",\"y_up\":true,\"canvas\":{{\"w\":{},\"h\":{},\"margin\":{{\"top\":{},\"right\":{},\"bottom\":{},\"left\":{}}},\"origin\":[{},{}]}}",
            snum(w),
            snum(h),
            snum(m.top),
            snum(m.right),
            snum(m.bottom),
            snum(m.left),
            snum(ox),
            snum(oy),
        ));
        if let Some(src) = &self.opts.source {
            self.out
                .push_str(&format!(",\"source\":\"{}\"", crate::json_str(src)));
        }
        // vars: sorted by name for deterministic output
        let mut vars: Vec<&(String, f64)> = d.vars.iter().collect();
        vars.sort_by(|a, b| a.0.cmp(&b.0));
        self.out.push_str(",\"vars\":{");
        for (i, (k, v)) in vars.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            self.out
                .push_str(&format!("\"{}\":{}", crate::json_str(k), svar(*v)));
        }
        self.out.push_str("},\"items\":[");
        self.items();
        self.out.push_str("]}");
    }

    fn items(&mut self) {
        let d = self.d;
        // draw order: the SVG emitter's stable (layer, index) sort
        let mut order: Vec<usize> = (0..d.shapes.len()).collect();
        order.sort_by_key(|&i| (d.shape_layers.get(i).copied().unwrap_or(0), i));
        // draw position of each shape
        let mut pos_of = vec![0usize; d.shapes.len()];
        for (pos, &i) in order.iter().enumerate() {
            pos_of[i] = pos;
        }
        // block items are interleaved right before their earliest-drawn child,
        // outermost first when several share it
        let mut at_pos: Vec<Vec<usize>> = vec![Vec::new(); order.len() + 1];
        let mut entries: Vec<(usize, usize, usize)> = (0..d.blocks.len())
            .map(|b| {
                let (lo, hi) = d.blocks[b].shapes;
                let first = (lo..hi).map(|s| pos_of[s]).min().unwrap_or(0);
                (first, block_depth(d, b), b)
            })
            .collect();
        entries.sort();
        for (first, _, b) in entries {
            at_pos[first].push(b);
        }

        let mut wrote = false;
        for (pos, &i) in order.iter().enumerate() {
            for &b in &at_pos[pos] {
                if wrote {
                    self.out.push(',');
                }
                wrote = true;
                self.block_item(b);
            }
            if wrote {
                self.out.push(',');
            }
            wrote = true;
            self.shape_item(i, pos);
        }
    }

    fn block_item(&mut self, b: usize) {
        let d = self.d;
        let info = &d.blocks[b];
        self.out.push_str(&format!("{{\"id\":\"blk{b}\",\"kind\":\"block\""));
        if let Some(l) = &info.label {
            self.out
                .push_str(&format!(",\"label\":\"{}\"", crate::json_str(l)));
        }
        if let Some(p) = info.parent {
            self.out
                .push_str(&format!(",\"block\":{}", block_path_json(d, Some(p))));
        }
        self.out.push_str(",\"items\":[");
        let mut first = true;
        for s in info.shapes.0..info.shapes.1 {
            if d.shape_blocks.get(s).copied().flatten() == Some(b) {
                if !first {
                    self.out.push(',');
                }
                first = false;
                self.out.push_str(&format!("\"s{s}\""));
            }
        }
        for (j, inner) in d.blocks.iter().enumerate() {
            if inner.parent == Some(b) {
                if !first {
                    self.out.push(',');
                }
                first = false;
                self.out.push_str(&format!("\"blk{j}\""));
            }
        }
        self.out.push_str("]}");
    }

    fn shape_item(&mut self, i: usize, z: usize) {
        let d = self.d;
        let sh = &d.shapes[i];
        let kind = scene_kind(sh);
        self.out
            .push_str(&format!("{{\"id\":\"s{i}\",\"kind\":\"{kind}\",\"z\":{z}"));
        if let Some(l) = d.shape_labels.get(i).and_then(|l| l.as_deref()) {
            self.out
                .push_str(&format!(",\"label\":\"{}\"", crate::json_str(l)));
        }
        if let Some(b) = d.shape_blocks.get(i).copied().flatten() {
            self.out
                .push_str(&format!(",\"block\":{}", block_path_json(d, Some(b))));
        }
        if let Some(c) = d.shape_classes.get(i).and_then(|c| c.as_deref()) {
            self.out
                .push_str(&format!(",\"class\":\"{}\"", crate::json_str(c)));
        }
        if let Some(l) = d.shape_links.get(i).and_then(|l| l.as_deref()) {
            self.out
                .push_str(&format!(",\"link\":\"{}\"", crate::json_str(l)));
        }
        let span = d.shape_spans.get(i).and_then(|s| s.clone());
        self.shape_body(sh, span);
        self.out.push('}');
    }

    fn shape_body(&mut self, sh: &Shape, span: Option<Span>) {
        match sh {
            Shape::Box {
                c,
                w,
                h,
                rad,
                style,
                text,
            } => {
                self.out.push_str(&format!(
                    ",\"at\":{},\"w\":{},\"h\":{}",
                    spoint(*c),
                    snum(w.abs()),
                    snum(h.abs())
                ));
                if *rad > 0.0 {
                    self.out.push_str(&format!(",\"rad\":{}", snum(*rad)));
                }
                self.closed_style(style, text, *c, span);
            }
            Shape::Circle {
                c, r, style, text, ..
            } => {
                self.out
                    .push_str(&format!(",\"at\":{},\"r\":{}", spoint(*c), snum(r.abs())));
                self.closed_style(style, text, *c, span);
            }
            Shape::Ellipse {
                c,
                w,
                h,
                style,
                text,
            } => {
                self.out.push_str(&format!(
                    ",\"at\":{},\"w\":{},\"h\":{}",
                    spoint(*c),
                    snum(w.abs()),
                    snum(h.abs())
                ));
                self.closed_style(style, text, *c, span);
            }
            Shape::Path {
                pts,
                closed,
                arrows,
                style,
                text,
            } => {
                let stroked = !style.invis && pts.len() >= 2;
                let filled = svg::open_fill_is_visible(style);
                let trimmed = stroked && *arrows != Arrowheads::None;
                let draw_pts: Vec<Point> = if trimmed {
                    svg::path_stroke_points_model(pts, *arrows, style)
                } else {
                    pts.clone()
                };
                self.out
                    .push_str(&format!(",\"points\":{}", spoints(&draw_pts)));
                if *closed {
                    self.out.push_str(",\"closed\":true");
                }
                if filled && trimmed {
                    self.out
                        .push_str(&format!(",\"fillPoints\":{}", spoints(pts)));
                }
                if trimmed {
                    self.heads_straight(pts, *arrows, style);
                }
                if stroked {
                    self.stroke_json(style, span.clone());
                }
                if filled {
                    self.fill_json(style, span.clone());
                    self.opacity_json(style);
                }
                let has_text = self.text_json(
                    svg::path_text_point(pts, *closed).unwrap_or(Point::ZERO),
                    text,
                    None,
                    span,
                );
                if !stroked && !filled && !has_text {
                    self.out.push_str(",\"hidden\":true");
                }
            }
            Shape::Spline {
                pts,
                tension,
                arrows,
                style,
                text,
            } => {
                let stroked = !style.invis && pts.len() >= 2;
                let filled = svg::open_fill_is_visible(style);
                let trimmed = stroked && *arrows != Arrowheads::None;
                let draw_pts: Vec<Point> = if trimmed {
                    svg::path_stroke_points_model(pts, *arrows, style)
                } else {
                    pts.clone()
                };
                self.out.push_str(",\"paths\":[");
                self.out.push_str(&self.spline_scene_path(&draw_pts, *tension).json());
                self.out.push(']');
                if filled && trimmed {
                    self.out.push_str(&format!(
                        ",\"fillPaths\":[{}]",
                        self.spline_scene_path(pts, *tension).json()
                    ));
                }
                if trimmed {
                    self.heads_straight(pts, *arrows, style);
                }
                if stroked {
                    self.stroke_json(style, span.clone());
                }
                if filled {
                    self.fill_json(style, span.clone());
                    self.opacity_json(style);
                }
                let has_text =
                    self.text_json(svg::midpoint(pts).unwrap_or(Point::ZERO), text, None, span);
                if !stroked && !filled && !has_text {
                    self.out.push_str(",\"hidden\":true");
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
                let stroked = !style.invis;
                let filled = svg::open_fill_is_visible(style);
                let start0 = *c + Point::new(a0.cos(), a0.sin()) * *r;
                let end0 = *c + Point::new(a1.cos(), a1.sin()) * *r;
                let angle0 = *a1 - *a0;
                let mut heads: Vec<String> = Vec::new();
                let (start, end) = if stroked && *r > 1e-9 {
                    let mut s = start0;
                    let mut e = end0;
                    if matches!(arrows, Arrowheads::Start | Arrowheads::Both) {
                        let (p, json) = self.arc_head(*c, s, *r, angle0, style, "start");
                        s = p;
                        heads.push(json);
                    }
                    if matches!(arrows, Arrowheads::End | Arrowheads::Both) {
                        let (p, json) = self.arc_head(*c, e, -*r, angle0, style, "end");
                        e = p;
                        heads.push(json);
                    }
                    (s, e)
                } else {
                    (start0, end0)
                };
                // the stroked arc (head-adjusted); fillPaths when it differs
                let path = if stroked {
                    let angle = svg::arc_angle_between(*c, start, end, angle0);
                    self.arc_scene_path(*c, *r, start, angle)
                } else {
                    self.arc_scene_path(*c, *r, start0, angle0)
                };
                self.out.push_str(&format!(",\"paths\":[{}]", path.json()));
                if filled && stroked && !heads.is_empty() {
                    self.out.push_str(&format!(
                        ",\"fillPaths\":[{}]",
                        self.arc_scene_path(*c, *r, start0, angle0).json()
                    ));
                }
                if !heads.is_empty() {
                    self.out
                        .push_str(&format!(",\"heads\":[{}]", heads.join(",")));
                }
                if stroked {
                    self.stroke_json(style, span.clone());
                }
                if filled {
                    self.fill_json(style, span.clone());
                    self.opacity_json(style);
                }
                let has_text = self.text_json(*c, text, None, span);
                if !stroked && !filled && !has_text {
                    self.out.push_str(",\"hidden\":true");
                }
            }
            Shape::Brace {
                cubics,
                label_at,
                style,
                text,
                ..
            } => {
                let stroked = !style.invis && !cubics.is_empty();
                if !cubics.is_empty() {
                    let start = cubics[0][0];
                    let segs: Vec<[Point; 3]> =
                        cubics.iter().map(|c| [c[1], c[2], c[3]]).collect();
                    let mut p = ScenePath::from_cubics(start, segs, false);
                    if let Some(tol) = self.opts.flatten {
                        p = p.flattened(tol);
                    }
                    self.out.push_str(&format!(",\"paths\":[{}]", p.json()));
                }
                if stroked {
                    self.stroke_json(style, span.clone());
                }
                let has_text = self.text_json(*label_at, text, None, span);
                if !stroked && !has_text {
                    self.out.push_str(",\"hidden\":true");
                }
            }
            Shape::Text {
                at,
                text,
                w,
                h,
                standalone,
                ..
            } => {
                self.out.push_str(&format!(",\"at\":{}", spoint(*at)));
                let dims = if *standalone { Some((*w, *h)) } else { None };
                let has_text = self.text_json(*at, text, dims, span);
                if !has_text {
                    self.out.push_str(",\"hidden\":true");
                }
            }
        }
    }

    /// Shared tail for the closed shapes (box/circle/ellipse): stroke/fill/
    /// opacity/text/hidden, mirroring the SVG's `closed_shape_is_visible` +
    /// `paint` rules.
    fn closed_style(&mut self, style: &Style, text: &[TextLine], at: Point, span: Option<Span>) {
        let visible = svg::closed_shape_is_visible(style);
        let stroked = visible && !style.invis;
        let filled =
            style.fill.is_some() || style.hatch.is_some() || style.gradient.is_some();
        if stroked {
            self.stroke_json(style, span.clone());
        }
        if filled {
            self.fill_json(style, span.clone());
            self.opacity_json(style);
        }
        let has_text = self.text_json(at, text, None, span);
        if !stroked && !filled && !has_text {
            self.out.push_str(",\"hidden\":true");
        }
    }

    fn spline_scene_path(&self, pts: &[Point], tension: Option<f64>) -> ScenePath {
        let mut p = if pts.len() < 3 {
            ScenePath::from_points(pts.to_vec(), false)
        } else {
            let (start, cubics) = svg::spline_cubics(pts, tension);
            ScenePath::from_cubics(start, cubics, false)
        };
        if let Some(tol) = self.opts.flatten {
            p = p.flattened(tol);
        }
        p
    }

    fn arc_scene_path(&self, c: Point, r: f64, start: Point, angle: f64) -> ScenePath {
        let t0 = (start - c).y.atan2((start - c).x);
        let mut cubics = Vec::new();
        arc_cubics(c, r.abs(), t0, angle, &mut cubics);
        let mut p = ScenePath::from_cubics(start, cubics, false);
        if let Some(tol) = self.opts.flatten {
            p = p.flattened(tol);
        }
        p
    }

    /// Straight-path arrowheads (line/arrow/spline): the SVG's filled triangle
    /// or open polyline, from the *untrimmed* tip (like `Svg::arrowheads`).
    fn heads_straight(&mut self, pts: &[Point], arrows: Arrowheads, style: &Style) {
        if pts.len() < 2 {
            return;
        }
        let mut heads: Vec<String> = Vec::new();
        let mut head = |at: &str, tip: Point, from: Point| {
            if style.arrow_filled {
                if let Some((l, p, r, _)) = svg::filled_arrowhead_points_model(tip, from, style) {
                    heads.push(format!(
                        "{{\"at\":\"{at}\",\"filled\":true,\"points\":{}}}",
                        spoints(&[l, p, r])
                    ));
                }
            } else if let Some((l, p, r)) = svg::open_arrowhead_points_model(tip, from, style) {
                heads.push(format!(
                    "{{\"at\":\"{at}\",\"filled\":false,\"points\":{}}}",
                    spoints(&[l, p, r])
                ));
            }
        };
        if matches!(arrows, Arrowheads::End | Arrowheads::Both) {
            head("end", pts[pts.len() - 1], pts[pts.len() - 2]);
        }
        if matches!(arrows, Arrowheads::Start | Arrowheads::Both) {
            head("start", pts[0], pts[1]);
        }
        if !heads.is_empty() {
            self.out
                .push_str(&format!(",\"heads\":[{}]", heads.join(",")));
        }
    }

    /// Arc arrowhead: the curved wedge the SVG paints, as a closed cubic
    /// path, plus the adjusted arc endpoint — the model-space mirror of
    /// `Svg::arc_arrowhead_path`.
    fn arc_head(
        &self,
        c: Point,
        point: Point,
        signed_r: f64,
        angle: f64,
        style: &Style,
        at: &str,
    ) -> (Point, String) {
        let atyp = if style.arrow_filled { 2 } else { 0 };
        let geom = svg::arc_head_geometry(
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
        let ccw = geom.ccw > 0.0;
        let mut cubics: Vec<[Point; 3]> = Vec::new();
        let mut cur: Point;
        let start: Point;
        let line_to = |cubics: &mut Vec<[Point; 3]>, from: Point, to: Point| {
            if from.dist(to) > 1e-12 {
                cubics.push(line_cubic(from, to));
            }
        };
        if atyp == 0 && geom.lwi < ((geom.wid - geom.lwi) / 2.0) {
            start = geom.px;
            cur = start;
            let q = svg::prop(geom.ai, geom.ci, r + geom.lwi, -geom.lwi, r);
            minor_arc_cubics(cur, q, r + geom.lwi, ccw, &mut cubics);
            cur = q;
            line_to(&mut cubics, cur, geom.ai);
            cur = geom.ai;
            minor_arc_cubics(cur, point, r, !ccw, &mut cubics);
            cur = point;
            minor_arc_cubics(cur, geom.ao, r, ccw, &mut cubics);
            cur = geom.ao;
            let q2 = svg::prop(geom.ao, geom.co, r - geom.lwi, geom.lwi, r);
            line_to(&mut cubics, cur, q2);
            cur = q2;
            minor_arc_cubics(cur, geom.px, r - geom.lwi, !ccw, &mut cubics);
        } else {
            start = (geom.ao + geom.ai) * 0.5;
            cur = start;
            line_to(&mut cubics, cur, geom.ai);
            cur = geom.ai;
            minor_arc_cubics(cur, point, r, !ccw, &mut cubics);
            cur = point;
            minor_arc_cubics(cur, geom.ao, r, ccw, &mut cubics);
            cur = geom.ao;
            line_to(&mut cubics, cur, start);
        }
        let mut p = ScenePath::from_cubics(start, cubics, true);
        if let Some(tol) = self.opts.flatten {
            p = p.flattened(tol);
        }
        let json = format!(
            "{{\"at\":\"{at}\",\"filled\":{},\"path\":{}}}",
            style.arrow_filled,
            p.json()
        );
        (geom.point, json)
    }

    // ---- style ----------------------------------------------------------

    fn color(&mut self, s: &str, span: &Option<Span>) -> String {
        match hex_color(s) {
            Ok(h) => h,
            Err(()) => {
                let mut d = Diagnostic::new(
                    "scene_unsupported",
                    format!("color `{s}` cannot be normalized to hex; passed through verbatim"),
                );
                if let Some(sp) = span {
                    d = d.at(sp.clone());
                }
                self.warnings.push(d);
                s.to_string()
            }
        }
    }

    fn stroke_json(&mut self, style: &Style, span: Option<Span>) {
        let color = self.color(style.stroke.as_deref().unwrap_or("black"), &span);
        self.out.push_str(&format!(
            ",\"stroke\":{{\"color\":\"{}\",\"width\":{}",
            crate::json_str(&color),
            snum(style.thick.filter(|t| *t > 0.0).unwrap_or(0.8))
        ));
        match style.dash {
            Dash::Solid => {}
            Dash::Dashed(w) => self
                .out
                .push_str(&format!(",\"dash\":{{\"kind\":\"dashed\",\"len\":{}}}", snum(w))),
            Dash::Dotted(gap) => match gap {
                Some(g) => self.out.push_str(&format!(
                    ",\"dash\":{{\"kind\":\"dotted\",\"gap\":{}}}",
                    snum(g)
                )),
                None => self.out.push_str(",\"dash\":{\"kind\":\"dotted\"}"),
            },
        }
        self.out.push('}');
    }

    fn fill_json(&mut self, style: &Style, span: Option<Span>) {
        self.out.push_str(",\"fill\":{");
        let mut first = true;
        if let Some(f) = &style.fill {
            let hex = match f {
                Fill::Gray(g) => {
                    let v = (g.clamp(0.0, 1.0) * 255.0).round() as u32;
                    format!("#{v:02x}{v:02x}{v:02x}")
                }
                Fill::Color(c) => self.color(c, &span),
            };
            self.out
                .push_str(&format!("\"color\":\"{}\"", crate::json_str(&hex)));
            first = false;
        }
        if let Some(g) = &style.gradient {
            if !first {
                self.out.push(',');
            }
            first = false;
            let from = self.color(&g.from, &span);
            let to = self.color(&g.to, &span);
            self.out.push_str(&format!(
                "\"gradient\":{{\"angle\":{},\"stops\":[{{\"offset\":0,\"color\":\"{}\"}},{{\"offset\":1,\"color\":\"{}\"}}]}}",
                snum(g.angle),
                crate::json_str(&from),
                crate::json_str(&to)
            ));
        }
        if let Some(hch) = &style.hatch {
            if !first {
                self.out.push(',');
            }
            let color = self.color(&hch.color, &span);
            self.out.push_str(&format!(
                "\"hatch\":{{\"angle\":{},\"spacing\":{},\"width\":{},\"color\":\"{}\",\"cross\":{}}}",
                snum(hch.angle),
                snum(hch.sep),
                snum(hch.width),
                crate::json_str(&color),
                hch.cross
            ));
        }
        self.out.push('}');
    }

    fn opacity_json(&mut self, style: &Style) {
        if let Some(o) = style.fill_opacity {
            self.out.push_str(&format!(",\"opacity\":{}", snum(o)));
        }
    }

    // ---- text -------------------------------------------------------------

    /// Emit the `text` array for a shape's attached (or standalone) lines,
    /// replicating `Svg::write_text`'s model-space layout: per-line baseline
    /// anchor point + pic compass anchor. Returns whether anything visible
    /// was emitted.
    fn text_json(
        &mut self,
        center: Point,
        lines: &[TextLine],
        standalone_dims: Option<(f64, f64)>,
        span: Option<Span>,
    ) -> bool {
        if lines.is_empty() {
            return false;
        }
        let standalone = standalone_dims.is_some();
        let n = lines.len() as f64;
        let v = n - 1.0 + DP_TEXT_RATIO;
        let lineskip = standalone_dims
            .map(|(_, h)| h)
            .filter(|_| v.abs() > 1e-12)
            .map(|h| h / v)
            .unwrap_or(FONT_PT_CLASSIC / 72.0);
        let xheight = lineskip * DP_TEXT_RATIO;
        let font_pt = lineskip * 72.0;
        let mut y = center.y + (v * lineskip / 2.0) - xheight;
        let mut visible = false;
        let mut parts: Vec<String> = Vec::new();
        for line in lines {
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
            y -= lineskip;
            if line.s.is_empty() && line.math.is_none() {
                continue;
            }
            visible = visible || !line.s.is_empty() || line.math.is_some();
            let anchor = compass(line.halign, line.valign);
            let mut t = format!(
                "{{\"text\":\"{}\",\"at\":[{},{}],\"anchor\":\"{anchor}\"",
                crate::json_str(&line.s),
                snum(x),
                snum(baseline_y)
            );
            t.push_str(&format!(
                ",\"font\":{{\"size\":{}",
                snum(line.size_pt.unwrap_or(font_pt))
            ));
            if let Some(f) = &line.family {
                t.push_str(&format!(",\"family\":\"{}\"", crate::json_str(f)));
            }
            if line.bold {
                t.push_str(",\"bold\":true");
            }
            if line.italic {
                t.push_str(",\"italic\":true");
            }
            t.push('}');
            if let Some(deg) = line.rotate {
                t.push_str(&format!(",\"rotation\":{}", snum(deg)));
            }
            if let Some(m) = &line.math {
                t.push_str(&format!(
                    ",\"math\":{{\"width\":{},\"height\":{},\"depth\":{}}}",
                    snum(m.width),
                    snum(m.height),
                    snum(m.depth)
                ));
                if self.opts.outline {
                    t.push_str(&format!(",\"outline\":\"{}\"", crate::json_str(&m.svg)));
                } else {
                    let mut diag = Diagnostic::new(
                        "scene_unsupported",
                        format!(
                            "typeset math `{}` is represented by its metrics only; \
                             pass the outline option to embed the glyphs",
                            line.s
                        ),
                    );
                    if let Some(sp) = &span {
                        diag = diag.at(sp.clone());
                    }
                    self.warnings.push(diag);
                }
            }
            t.push('}');
            parts.push(t);
        }
        if parts.is_empty() {
            return false;
        }
        self.out
            .push_str(&format!(",\"text\":[{}]", parts.join(",")));
        visible
    }
}

/// pic compass anchor from the halign/valign pair: which point of the ink box
/// the pic reference semantics pin (w = extends right of it, s = sits above …).
fn compass(halign: i8, valign: i8) -> &'static str {
    match (valign, halign) {
        (1, -1) => "sw",
        (1, 1) => "se",
        (1, _) => "s",
        (-1, -1) => "nw",
        (-1, 1) => "ne",
        (-1, _) => "n",
        (_, -1) => "w",
        (_, 1) => "e",
        _ => "c",
    }
}

fn scene_kind(sh: &Shape) -> &'static str {
    match sh {
        Shape::Box { .. } => "box",
        Shape::Circle { dot: true, .. } => "dot",
        Shape::Circle { .. } => "circle",
        Shape::Ellipse { .. } => "ellipse",
        Shape::Path { arrows, .. } if *arrows != Arrowheads::None => "arrow",
        Shape::Path { .. } => "line",
        Shape::Spline { .. } => "spline",
        Shape::Arc { .. } => "arc",
        Shape::Brace { .. } => "brace",
        Shape::Text { .. } => "text",
    }
}

fn block_depth(d: &Drawing, mut b: usize) -> usize {
    let mut depth = 0;
    while let Some(p) = d.blocks[b].parent {
        depth += 1;
        b = p;
        if depth > d.blocks.len() {
            break; // cycle guard; cannot happen with eval-built tables
        }
    }
    depth
}

/// `["blk0","blk2"]` — enclosing block ids, outermost first.
fn block_path_json(d: &Drawing, innermost: Option<usize>) -> String {
    let mut chain = Vec::new();
    let mut cur = innermost;
    while let Some(b) = cur {
        chain.push(b);
        cur = d.blocks[b].parent;
        if chain.len() > d.blocks.len() {
            break;
        }
    }
    chain.reverse();
    let mut s = String::from("[");
    for (i, b) in chain.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("\"blk{b}\""));
    }
    s.push(']');
    s
}
