//! MapLibre style-spec layers for a geopic FeatureCollection (DECISIONS G9,
//! `docs/geopic.md` § "Rendering on MapLibre"): styling as data, emitted once
//! here so every consumer — the rpic.dev Maps page, the Studio, other sites,
//! R/mapgl — draws the same map.
//!
//! The layers read one GeoJSON source, always named [`SOURCE`] (`"pic"`), and
//! style every feature from its simplestyle + `geopic:*` properties. Every
//! size (stroke width, dot radius, font size) is **true to the ground**: the
//! collection's `geopic.registration` gives metres per inch, a pic size
//! becomes metres, and metres become pixels at zoom `z` as
//! `m · 2^z / (78271.517 · cos φ)` — MapLibre's 512 px tiles put the equator
//! on 512 px at zoom 0. Only a top-level `interpolate` may read `zoom`, so
//! each size is an exponential (base 2) interpolation between two zoom stops,
//! which reproduces the 2^z law exactly. Under the `mercator` frame the scale
//! is already in EPSG:3857 metres, so `cos φ` drops out.
//!
//! The JS port in `bindings/js/maplibre.js` mirrors this module line for
//! line; a test there pins it against this output.

use serde_json::{Value, json};

use crate::{FeatureCollection, GeoError};

/// The GeoJSON source id every layer reads.
pub const SOURCE: &str = "pic";

/// Metres per pixel at zoom 0 on the equator (512 px tiles, WGS84 equator).
pub const METRES_PER_PX_Z0: f64 = 78_271.517;

/// MapLibre packs symbol sizes into 16-bit vertex attributes: ≈ 255 px max.
pub const MAX_TEXT_PX: f64 = 255.0;

/// Defaults when a feature or the collection lacks the property.
const DEFAULT_M_PER_IN: f64 = 100.0;
const DEFAULT_STROKE_PT: f64 = 0.8;
const DEFAULT_RADIUS_IN: f64 = 0.04;
const DEFAULT_FONT_PT: f64 = 11.0;

/// pic compass anchor → MapLibre `text-anchor` (the reference point sits at
/// that side of the text: `w` = text extends right of it, `n` = below).
pub const ANCHORS: [(&str, &str); 9] = [
    ("c", "center"),
    ("n", "top"),
    ("s", "bottom"),
    ("e", "right"),
    ("w", "left"),
    ("ne", "top-right"),
    ("nw", "top-left"),
    ("se", "bottom-right"),
    ("sw", "bottom-left"),
];

/// The per-collection numbers the size expressions are built from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sizing {
    /// `registration.scale_m_per_in` (100 when absent).
    pub m_per_in: f64,
    /// The latitude whose `cos` scales pixels: the anchor's under `enu`, 0
    /// under `mercator` (and when absent).
    pub lat: f64,
    /// Pixels per ground metre at zoom 0: `1 / (78271.517 · cos lat)`.
    pub px_per_metre_z0: f64,
    /// Upper zoom stop of `text-size`: the last zoom at which the largest
    /// font of the collection still fits in 255 px, clamped to `1..=24`.
    pub text_z_hi: i32,
}

/// A JSON value read as a number the way JavaScript's `Number(v) || 0` does
/// for the shapes a property can take (numbers, numeric strings).
fn num_or_zero(v: Option<&Value>) -> f64 {
    let x = match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) if s.trim().is_empty() => 0.0,
        Some(Value::String(s)) => s.trim().parse::<f64>().unwrap_or(0.0),
        Some(Value::Bool(true)) => 1.0,
        _ => 0.0,
    };
    if x.is_nan() { 0.0 } else { x }
}

/// A registration field as a number: a JSON number, or a numeric string (a
/// collection re-serialised by a loosely typed host) — the same coercion the
/// JS port's arithmetic applies, so the two emit the same sizes.
fn num_opt(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|x| x.is_finite())
}

fn sizing_from<'a>(geopic: Option<&Value>, font_sizes: impl Iterator<Item = &'a Value>) -> Sizing {
    let reg = geopic.and_then(|g| g.get("registration"));
    let m_per_in = num_opt(reg.and_then(|r| r.get("scale_m_per_in"))).unwrap_or(DEFAULT_M_PER_IN);
    let mercator = reg.and_then(|r| r.get("frame")).and_then(Value::as_str) == Some("mercator");
    let lat = if mercator {
        0.0
    } else {
        num_opt(reg.and_then(|r| r.get("anchor")).and_then(|a| a.get("lat"))).unwrap_or(0.0)
    };
    let d = METRES_PER_PX_Z0 * (lat * std::f64::consts::PI / 180.0).cos();
    let px_per_metre_z0 = 1.0 / if d == 0.0 || d.is_nan() { 1.0 } else { d };
    let max_font_m = font_sizes
        .map(|v| num_or_zero(Some(v)) * (m_per_in / 72.0))
        .fold(1e-6, f64::max);
    let z = (MAX_TEXT_PX / (max_font_m * px_per_metre_z0))
        .log2()
        .floor();
    // NaN only from a degenerate registration; the lowest stop keeps it valid
    let text_z_hi = if z.is_nan() {
        1
    } else {
        z.clamp(1.0, 24.0) as i32
    };
    Sizing {
        m_per_in,
        lat,
        px_per_metre_z0,
        text_z_hi,
    }
}

/// The sizing numbers of a collection (its `geopic.registration` and the
/// largest `geopic:font_size` among its features).
pub fn sizing(fc: &FeatureCollection) -> Sizing {
    sizing_from(
        fc.geopic.as_ref(),
        fc.features
            .iter()
            .filter_map(|f| f.properties.get("geopic:font_size")),
    )
}

/// `2^z` as a float.
fn pow2(z: i32) -> f64 {
    2f64.powi(z)
}

impl Sizing {
    /// Ground-true pixels between zoom 0 and 24, never below `floor_px`.
    fn ground_px(&self, metres: Value, floor_px: i64) -> Value {
        let p = self.px_per_metre_z0;
        json!([
            "interpolate",
            ["exponential", 2],
            ["zoom"],
            0,
            ["max", floor_px, ["*", metres.clone(), p]],
            24,
            ["max", floor_px, ["*", metres, p * pow2(24)]]
        ])
    }

    /// Ground-true text pixels up to `text_z_hi`, frozen (≤ 255 px) beyond.
    fn ground_text_px(&self, metres: Value) -> Value {
        let p = self.px_per_metre_z0;
        let z = self.text_z_hi;
        json!([
            "interpolate",
            ["exponential", 2],
            ["zoom"],
            0,
            ["max", 1, ["*", metres.clone(), p]],
            z,
            ["min", 255, ["max", 1, ["*", metres, p * pow2(z)]]]
        ])
    }

    fn pt_to_m(&self, expr: Value) -> Value {
        json!(["*", expr, self.m_per_in / 72.0])
    }

    fn in_to_m(&self, expr: Value) -> Value {
        json!(["*", expr, self.m_per_in])
    }

    /// `line-width`: stroke points → metres → pixels, at least 1 px.
    pub fn line_width(&self) -> Value {
        self.ground_px(
            self.pt_to_m(json!([
                "coalesce",
                ["get", "stroke-width"],
                DEFAULT_STROKE_PT
            ])),
            1,
        )
    }

    /// `circle-radius`: dot radius inches → metres → pixels, at least 2 px.
    pub fn dot_radius(&self) -> Value {
        self.ground_px(
            self.in_to_m(json!([
                "coalesce",
                ["get", "geopic:radius_in"],
                DEFAULT_RADIUS_IN
            ])),
            2,
        )
    }

    /// `text-size`: font points → metres → pixels (1..=255 px).
    pub fn text_size(&self) -> Value {
        self.ground_text_px(self.pt_to_m(json!([
            "coalesce",
            ["get", "geopic:font_size"],
            DEFAULT_FONT_PT as i64
        ])))
    }

    /// The six layers for these sizes, in draw order.
    pub fn layers(&self) -> Vec<Value> {
        let is_polygon = json!(["==", ["geometry-type"], "Polygon"]);
        let is_line = json!(["==", ["geometry-type"], "LineString"]);
        let kind = json!(["get", "geopic:kind"]);
        let line_width = self.line_width();
        let text_size = self.text_size();
        let text_paint = json!({
            "text-color": "#0f172a",
            "text-halo-color": "#ffffff",
            "text-halo-width": 1.5,
        });
        let mut anchor = vec![json!("match"), json!(["get", "geopic:anchor"])];
        for (pic, ml) in ANCHORS {
            anchor.push(json!(pic));
            anchor.push(json!(ml));
        }
        anchor.push(json!("center"));

        let stroke = |id: &str, dashed: bool| {
            let dash = if dashed {
                json!(["has", "geopic:dash"])
            } else {
                json!(["!", ["has", "geopic:dash"]])
            };
            let mut paint = json!({
                "line-color": ["get", "stroke"],
                "line-width": line_width.clone(),
                "line-opacity": ["coalesce", ["get", "stroke-opacity"], 1],
            });
            if dashed {
                paint["line-dasharray"] = json!([3, 2]);
            }
            json!({
                "id": id, "type": "line", "source": SOURCE,
                "filter": ["all", ["any", is_polygon.clone(), is_line.clone()], ["has", "stroke"], dash],
                "layout": { "line-cap": "round", "line-join": "round" },
                "paint": paint,
            })
        };

        // geopic:text keeps the author's blank rows (G10 addendum), but
        // MapLibre's shaper only breaks *before* a following character, so a
        // final "\n" is dropped and `box "above" ""` would render centred:
        // a trailing newline is padded with a no-break space (trimmed by the
        // shaper, so the row stays empty but is kept)
        let text = json!(["get", "geopic:text"]);
        let text_field = json!([
            "case",
            [
                "==",
                ["slice", text, ["max", 0, ["-", ["length", text], 1]]],
                "\n"
            ],
            ["concat", text, "\u{a0}"],
            text
        ]);
        let symbol = |id: &str, k: &str| {
            json!({
                "id": id, "type": "symbol", "source": SOURCE,
                "filter": ["==", kind.clone(), k],
                "layout": {
                    "text-field": text_field.clone(),
                    "text-font": ["Noto Sans Regular"],
                    "text-size": text_size.clone(),
                    // pic's line skip is the font size; MapLibre's default
                    // 1.2 em would push every row 20 % off `text_center`
                    "text-line-height": 1,
                    "text-anchor": anchor.clone(),
                    // geopic:rotation is CCW from east; MapLibre rotates clockwise
                    "text-rotate": ["*", -1, ["coalesce", ["get", "geopic:rotation"], 0]],
                    "text-rotation-alignment": "map",
                    "text-pitch-alignment": "map",
                    "text-allow-overlap": true,
                    "text-ignore-placement": true,
                },
                "paint": text_paint.clone(),
            })
        };

        vec![
            json!({
                "id": "pic-fill", "type": "fill", "source": SOURCE,
                "filter": is_polygon.clone(),
                "paint": {
                    "fill-color": ["coalesce", ["get", "fill"], "#000000"],
                    "fill-opacity": ["case", ["has", "fill"], ["coalesce", ["get", "fill-opacity"], 0.55], 0],
                },
            }),
            stroke("pic-stroke", false),
            stroke("pic-stroke-dashed", true),
            json!({
                "id": "pic-dot", "type": "circle", "source": SOURCE,
                "filter": ["==", kind.clone(), "dot"],
                "paint": {
                    "circle-radius": self.dot_radius(),
                    "circle-color": ["coalesce", ["get", "fill"], "#000000"],
                },
            }),
            symbol("pic-text", "text"),
            // text attached to a shape (a box's label): its own Point at the
            // shape's centre (DECISIONS G10), styled like free text
            symbol("pic-title", "title"),
        ]
    }
}

/// The MapLibre layers (style-spec objects, source [`SOURCE`]) that draw
/// `fc` with sizes true to the ground, in draw order: `pic-fill`,
/// `pic-stroke`, `pic-stroke-dashed`, `pic-dot`, `pic-text`, `pic-title`.
pub fn layers(fc: &FeatureCollection) -> Vec<Value> {
    sizing(fc).layers()
}

/// [`layers`] for a FeatureCollection given as JSON text (bindings): returns
/// the layers array as JSON text. Only the `geopic.registration` foreign
/// member and the features' `geopic:font_size` are read.
pub fn layers_json(fc_json: &str) -> Result<String, GeoError> {
    let v: Value = serde_json::from_str(fc_json).map_err(|e| {
        GeoError::new(
            "invalid_geojson",
            format!("FeatureCollection JSON does not parse: {e}"),
        )
    })?;
    let features = v
        .get("features")
        .and_then(Value::as_array)
        .filter(|_| v.get("type").and_then(Value::as_str) == Some("FeatureCollection"))
        .ok_or_else(|| {
            GeoError::new(
                "invalid_geojson",
                "expected a GeoJSON FeatureCollection (`type` and `features`)",
            )
        })?;
    let s = sizing_from(
        v.get("geopic"),
        features
            .iter()
            .filter_map(|f| f.get("properties").and_then(|p| p.get("geopic:font_size"))),
    );
    Ok(Value::Array(s.layers()).to_string())
}

/// `{"geojson": <fc>, "layers": [...]}` as JSON text — the shape
/// `rpic --geojson --maplibre` prints. The collection's own text is spliced
/// in (no re-parse).
pub fn bundle_json(fc: &FeatureCollection) -> String {
    format!(
        "{{\"geojson\":{},\"layers\":{}}}",
        fc.to_json(),
        Value::Array(layers(fc))
    )
}
