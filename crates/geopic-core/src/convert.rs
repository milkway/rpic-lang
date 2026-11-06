//! Scene items → GeoJSON features.

use crate::densify::{
    MAX_VERTICES, box_ring, close_ring, densify_polyline, ellipse_ring, path_points,
};
use crate::geojson::{Feature, FeatureCollection, Geometry};
use crate::register::Registration;
use crate::scene::{Head, Item, Scene, TextLine};
use crate::{GEOPIC_SCHEMA, GeoOptions, Warning};
use serde_json::{Map, Value, json};

/// Every scene `kind` this converter knows. A kind missing here raises
/// `unsupported_kind` (the exhaustiveness test enumerates the format's list).
pub const KINDS: [&str; 11] = [
    "box", "circle", "ellipse", "arc", "line", "arrow", "spline", "text", "dot", "brace", "block",
];

type Props = Map<String, Value>;

/// Round a coordinate to 9 decimals (≈ 0.1 mm) for stable output.
fn round9(v: f64) -> f64 {
    (v * 1e9).round() / 1e9
}

struct Ctx<'a> {
    scene: &'a Scene,
    reg: &'a Registration,
    opts: &'a GeoOptions,
    warnings: &'a mut Vec<Warning>,
    out: Vec<Feature>,
    /// Remaining densification vertices for the whole scene.
    budget: usize,
    budget_hit: bool,
}

impl Ctx<'_> {
    fn geo(&self, p: [f64; 2]) -> [f64; 2] {
        let ll = self.reg.to_lonlat(p);
        [round9(ll.lon), round9(ll.lat)]
    }

    fn geo_line(&mut self, pts: &[[f64; 2]]) -> Vec<[f64; 2]> {
        let (dense, limited) = densify_polyline(pts, self.reg, &mut self.budget);
        if limited {
            self.budget_hit = true;
        }
        dense.iter().map(|&p| self.geo(p)).collect()
    }

    fn geo_ring(&mut self, ring: &[[f64; 2]]) -> Vec<[f64; 2]> {
        let mut r = self.geo_line(ring);
        // densification keeps first == last; make sure after rounding
        if let Some(&f) = r.first()
            && let Some(l) = r.last_mut()
        {
            *l = f;
        }
        r
    }

    fn placement(&self, item: &Item) -> &'static str {
        if item.kind == "text" || item.kind == "dot" {
            return "screen";
        }
        if let Some(c) = &item.class
            && c.split_whitespace()
                .any(|w| self.opts.screen_classes.iter().any(|s| s == w))
        {
            return "screen";
        }
        "geo"
    }

    fn base_props(&mut self, item: &Item) -> Props {
        let mut p = Props::new();
        p.insert("geopic:schema".into(), json!(GEOPIC_SCHEMA));
        p.insert("geopic:id".into(), json!(item.id));
        p.insert("geopic:kind".into(), json!(item.kind));
        if let Some(z) = item.z {
            p.insert("geopic:z".into(), json!(z));
        }
        if let Some(l) = &item.label {
            p.insert("geopic:label".into(), json!(l));
        }
        if let Some(b) = &item.block {
            p.insert("geopic:block".into(), json!(b));
        }
        if let Some(c) = &item.class {
            p.insert("geopic:class".into(), json!(c));
        }
        if let Some(l) = &item.link {
            p.insert("geopic:link".into(), json!(l));
        }
        p.insert("geopic:placement".into(), json!(self.placement(item)));
        // text attached to the item
        if let Some(lines) = &item.text
            && !lines.is_empty()
        {
            let text = lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            p.insert("title".into(), json!(text));
            p.insert("geopic:text".into(), json!(text));
            for l in lines {
                if l.math.is_some() {
                    self.warnings.push(
                        Warning::new(
                            "ignored",
                            format!("math label {:?} kept as its TeX literal; typeset glyphs are not representable", l.text),
                        )
                        .on(item),
                    );
                }
            }
        }
        // style → simplestyle + geopic descriptors
        if let Some(s) = &item.stroke {
            p.insert("stroke".into(), json!(s.color));
            p.insert("stroke-width".into(), json!(s.width));
            if let Some(d) = &s.dash {
                p.insert("geopic:dash".into(), d.clone());
            }
        }
        if let Some(f) = &item.fill {
            let mut color = f.color.clone();
            if let Some(g) = &f.gradient {
                self.warnings.push(
                    Warning::new(
                        "unsupported_style",
                        "gradient fill is not representable in GeoJSON; the first stop's color is used",
                    )
                    .on(item),
                );
                if color.is_none() {
                    color = g.stops.first().map(|s| s.color.clone());
                }
            }
            if let Some(c) = color {
                p.insert("fill".into(), json!(c));
            }
            if let Some(h) = &f.hatch {
                p.insert("geopic:hatch".into(), h.clone());
            }
        }
        if let Some(o) = item.opacity {
            p.insert("fill-opacity".into(), json!(o));
            p.insert("stroke-opacity".into(), json!(o));
        }
        p
    }

    fn push(&mut self, geometry: Geometry, props: Props) {
        self.out.push(Feature::new(geometry, props));
    }

    /// An open or closed stroke. Closed → Polygon. Open → LineString; when
    /// the item is also filled (the SVG paints the implicitly closed area),
    /// a second Polygon feature with `geopic:role = "fill"` carries the fill
    /// — from `fill_outline` (the scene's untrimmed `fillPaths`/`fillPoints`)
    /// when given, else from the stroke's own vertices — and the LineString
    /// keeps only the stroke properties.
    fn polyline_or_polygon(
        &mut self,
        pts: &[[f64; 2]],
        closed: bool,
        fill_outline: Option<&[[f64; 2]]>,
        mut props: Props,
    ) {
        if pts.len() < 2 {
            return;
        }
        if closed {
            let mut ring = pts.to_vec();
            close_ring(&mut ring);
            if ring.len() >= 4 {
                let r = self.geo_ring(&ring);
                self.push(Geometry::Polygon(vec![r]), props);
                return;
            }
        }
        if props.contains_key("fill") {
            let mut fill_props = props.clone();
            fill_props.remove("stroke");
            fill_props.remove("stroke-width");
            fill_props.remove("stroke-opacity");
            fill_props.remove("geopic:dash");
            fill_props.insert("geopic:role".into(), json!("fill"));
            props.remove("fill");
            props.remove("fill-opacity");
            props.remove("geopic:hatch");
            let mut ring = fill_outline.unwrap_or(pts).to_vec();
            close_ring(&mut ring);
            if ring.len() >= 4 {
                let r = self.geo_ring(&ring);
                self.push(Geometry::Polygon(vec![r]), fill_props);
            }
        }
        let line = self.geo_line(pts);
        self.push(Geometry::LineString(line), props);
    }

    fn heads(&mut self, item: &Item, heads: &[Head], base: &Props) {
        for h in heads {
            let mut props = base.clone();
            props.insert("geopic:kind".into(), json!("arrowhead"));
            props.insert("geopic:head".into(), json!(h.at));
            props.insert("geopic:filled".into(), json!(h.filled));
            if let Some(pts) = &h.points {
                if h.filled {
                    let mut ring = pts.clone();
                    close_ring(&mut ring);
                    if ring.len() >= 4 {
                        // a filled head is painted in the stroke color
                        if let Some(c) = base.get("stroke").cloned() {
                            props.insert("fill".into(), c);
                        }
                        let r = self.geo_ring(&ring);
                        self.push(Geometry::Polygon(vec![r]), props);
                    }
                } else if pts.len() >= 2 {
                    let line = self.geo_line(pts);
                    self.push(Geometry::LineString(line), props);
                }
            } else if let Some(path) = &h.path {
                let mut ring = path_points(path, self.opts.flatten);
                close_ring(&mut ring);
                if ring.len() >= 4 {
                    if let Some(c) = base.get("stroke").cloned() {
                        props.insert("fill".into(), c);
                    }
                    let r = self.geo_ring(&ring);
                    self.push(Geometry::Polygon(vec![r]), props);
                }
            } else {
                self.warnings
                    .push(Warning::new("ignored", "arrowhead without geometry").on(item));
            }
        }
    }

    fn text_point(&mut self, lines: &[TextLine], at: [f64; 2], mut props: Props) {
        if let Some(first) = lines.first() {
            let rot = first.rotation.unwrap_or(0.0);
            // scene rotation is CCW on the canvas; the canvas +x axis sits at
            // −bearing CCW from the map's east, so the map rotation (CCW from
            // east) is rot − bearing
            props.insert("geopic:rotation".into(), json!(rot - self.reg.bearing_deg));
            props.insert("geopic:anchor".into(), json!(first.anchor));
        }
        let g = self.geo(at);
        self.push(Geometry::Point(g), props);
    }

    fn item(&mut self, item: &Item) {
        if item.kind == "block" || item.is_hidden() {
            return;
        }
        let props = self.base_props(item);
        match item.kind.as_str() {
            "box" => {
                let (Some(c), Some(w), Some(h)) = (item.at, item.w, item.h) else {
                    return self.missing(item);
                };
                let ring = box_ring(
                    c,
                    w,
                    h,
                    item.rad.unwrap_or(0.0),
                    self.opts.flatten,
                    self.reg,
                );
                let r = self.geo_ring(&ring);
                self.push(Geometry::Polygon(vec![r]), props);
            }
            "circle" => {
                let (Some(c), Some(r)) = (item.at, item.r) else {
                    return self.missing(item);
                };
                let ring = ellipse_ring(c, r, r, self.opts.flatten, self.reg);
                let g = self.geo_ring(&ring);
                self.push(Geometry::Polygon(vec![g]), props);
            }
            "ellipse" => {
                let (Some(c), Some(w), Some(h)) = (item.at, item.w, item.h) else {
                    return self.missing(item);
                };
                let ring = ellipse_ring(c, w / 2.0, h / 2.0, self.opts.flatten, self.reg);
                let g = self.geo_ring(&ring);
                self.push(Geometry::Polygon(vec![g]), props);
            }
            "dot" => {
                let Some(c) = item.at else {
                    return self.missing(item);
                };
                let mut props = props;
                if let Some(r) = item.r {
                    props.insert("geopic:radius_in".into(), json!(r));
                }
                let g = self.geo(c);
                self.push(Geometry::Point(g), props);
            }
            "line" | "arrow" => {
                let Some(pts) = &item.points else {
                    return self.missing(item);
                };
                let closed = item.closed == Some(true);
                self.polyline_or_polygon(pts, closed, item.fill_points.as_deref(), props.clone());
                if let Some(heads) = &item.heads {
                    self.heads(item, heads, &props);
                }
            }
            "spline" | "arc" | "brace" => {
                let Some(paths) = &item.paths else {
                    return self.missing(item);
                };
                for (i, path) in paths.iter().enumerate() {
                    let pts = path_points(path, self.opts.flatten);
                    let fill_outline = item
                        .fill_paths
                        .as_ref()
                        .and_then(|fp| fp.get(i))
                        .map(|fp| path_points(fp, self.opts.flatten));
                    let mut p = props.clone();
                    if paths.len() > 1 {
                        p.insert("geopic:part".into(), json!(i));
                    }
                    self.polyline_or_polygon(&pts, path.closed, fill_outline.as_deref(), p);
                }
                if let Some(heads) = &item.heads {
                    self.heads(item, heads, &props);
                }
            }
            "text" => {
                let Some(at) = item.at else {
                    return self.missing(item);
                };
                let lines = item.text.clone().unwrap_or_default();
                self.text_point(&lines, at, props);
            }
            other => {
                self.warnings.push(
                    Warning::new(
                        "unsupported_kind",
                        format!("scene kind {other:?} has no GeoJSON mapping; skipped"),
                    )
                    .on(item),
                );
            }
        }
    }

    fn missing(&mut self, item: &Item) {
        self.warnings.push(
            Warning::new(
                "ignored",
                format!(
                    "{} item without the geometry its kind requires; skipped",
                    item.kind
                ),
            )
            .on(item),
        );
    }
}

/// Convert every item of `scene` under `reg`.
pub fn convert(
    scene: &Scene,
    reg: &Registration,
    opts: &GeoOptions,
    warnings: &mut Vec<Warning>,
) -> FeatureCollection {
    let mut ctx = Ctx {
        scene,
        reg,
        opts,
        warnings,
        out: Vec::new(),
        budget: MAX_VERTICES,
        budget_hit: false,
    };
    for item in &ctx.scene.items {
        ctx.item(item);
    }
    if ctx.budget_hit {
        ctx.warnings.push(Warning::new(
            "densification_budget",
            format!(
                "densification budget ({MAX_VERTICES} vertices) exhausted; later segments keep only their endpoints"
            ),
        ));
    }
    FeatureCollection::new(ctx.out)
}
