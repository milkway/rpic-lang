//! geopic in the browser: `compile_geo` (pic source → GeoJSON
//! FeatureCollection, docs/geopic.md) and `geo_to_canvas` (the inverse map
//! the JS host uses for two-pass feature injection). Built only with
//! `--features geo`.

use wasm_bindgen::prelude::*;

use crate::wasm_options;

/// A host feature as the options JSON carries it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GeoFeatureOpt {
    name: String,
    lon: f64,
    lat: f64,
    #[serde(default)]
    bbox: Option<[f64; 4]>,
}

/// `compile_geo`'s options JSON; every key optional.
#[derive(serde::Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct GeoOpts {
    frame: Option<String>,
    anchor: Option<[f64; 2]>,
    scale: Option<f64>,
    bearing: Option<f64>,
    features: Vec<GeoFeatureOpt>,
    flatten: Option<f64>,
    view_scale: Option<f64>,
    screen_classes: Option<Vec<String>>,
}

impl GeoOpts {
    fn into_options(self) -> Result<geopic_core::GeoOptions, geopic_core::GeoError> {
        let defaults = geopic_core::GeoOptions::default();
        let frame = match self.frame {
            None => None,
            Some(f) => Some(f.parse::<geopic_core::Frame>()?),
        };
        Ok(geopic_core::GeoOptions {
            frame,
            anchor: self
                .anchor
                .map(|[lon, lat]| geopic_core::LonLat { lon, lat }),
            scale: self.scale,
            bearing: self.bearing,
            features: self
                .features
                .into_iter()
                .map(|f| geopic_core::NamedFeature {
                    name: f.name,
                    lon: f.lon,
                    lat: f.lat,
                    bbox: f.bbox,
                })
                .collect(),
            flatten: self.flatten.unwrap_or(defaults.flatten),
            view_scale: self.view_scale,
            screen_classes: self.screen_classes.unwrap_or(defaults.screen_classes),
        })
    }
}

/// `{"error", "geo_error": {kind, message, hint?}}`.
fn geo_error_json(e: &geopic_core::GeoError) -> String {
    serde_json::json!({ "error": e.to_string(), "geo_error": e }).to_string()
}

/// An engine diagnostic as the bundle's `warnings`/`error_info` carry it
/// (rpic-core's own serializer, so the keys never drift).
fn diagnostic_value(d: &rpic_core::Diagnostic) -> serde_json::Value {
    serde_json::from_str(&rpic_core::diagnostic_json(d)).unwrap_or(serde_json::Value::Null)
}

/// Georeference pic source: `{"geojson": FeatureCollection, "warnings":
/// [...], "diagnostics": [...], "registration": {...}, "canvas": {"rescale",
/// "scale"}}`, or
/// `{"error", "error_info"}` on a pic error, or `{"error", "geo_error":
/// {kind, message, hint}}` on a georeferencing error (bad options included).
///
/// `opts_json` is a JSON object (or empty string) with the optional keys
/// `frame` (`"enu"` | `"mercator"`), `anchor` (`[lon, lat]`), `scale`
/// (metres per inch), `bearing` (degrees clockwise from north), `features`
/// (`[{name, lon, lat, bbox?}]`, control points by label), `flatten`
/// (inches), `view_scale` (px per inch under `geo_px = 1`) and
/// `screen_classes`. Each overrides the corresponding `geo_*` variable of
/// the source. Animations cannot ride into GeoJSON: a source with `animate`
/// statements adds an `ignored` warning.
#[wasm_bindgen]
pub fn compile_geo(src: &str, circuits: bool, texlabels: bool, opts_json: &str) -> String {
    let parsed: Result<GeoOpts, _> = if opts_json.trim().is_empty() {
        Ok(GeoOpts::default())
    } else {
        serde_json::from_str(opts_json)
    };
    let gopts = match parsed {
        Ok(o) => match o.into_options() {
            Ok(g) => g,
            Err(e) => return geo_error_json(&e),
        },
        Err(e) => {
            return geo_error_json(&geopic_core::GeoError::new(
                "invalid_options",
                format!("geo options do not parse: {e}"),
            ));
        }
    };
    let d = match rpic_core::compile_with_diagnostics(src, &wasm_options(circuits, texlabels)) {
        Ok(d) => d,
        Err(e) => {
            return serde_json::json!({
                "error": e.message,
                "error_info": diagnostic_value(&e.info),
                "warnings": [],
            })
            .to_string();
        }
    };
    let scene = rpic_core::scene_json(&d, &rpic_core::SceneOptions::default());
    let mut g = match geopic_core::georeference(&scene.json, &gopts) {
        Ok(g) => g,
        Err(e) => return geo_error_json(&e),
    };
    if !d.anims.is_empty() {
        g.warnings
            .push(geopic_core::Warning::animations_ignored(d.anims.len()));
    }
    // engine warnings first, then the scene's, then geopic's (the order
    // `compile_json_all` uses)
    let mut warnings: Vec<serde_json::Value> = d
        .warnings
        .iter()
        .chain(scene.warnings.iter())
        .map(diagnostic_value)
        .collect();
    warnings.extend(
        g.warnings
            .iter()
            .map(|w| serde_json::to_value(w).unwrap_or(serde_json::Value::Null)),
    );
    let scale = d
        .vars
        .iter()
        .find(|(k, _)| k == "scale")
        .map_or(1.0, |(_, v)| *v);
    let rest = serde_json::json!({
        "warnings": warnings,
        // pic `print` lines and engine notices (e.g. texlabels without a
        // math renderer), as in `compile`'s bundle
        "diagnostics": d.diagnostics,
        "registration": g.registration,
        "canvas": { "rescale": d.rescale, "scale": scale },
    })
    .to_string();
    // splice the FeatureCollection text in without re-parsing it
    format!("{{\"geojson\":{},{}", g.features.to_json(), &rest[1..])
}

/// Canvas positions (inches) of WGS84 points under a registration (as
/// `compile_geo` returned it): `points_json` is `[[lon, lat], …]`; returns
/// `[[x, y], …]`, or `{"error", "geo_error"}` when either input does not
/// parse. The host uses it to place features before compiling (two-pass
/// feature injection, see `bindings/js/index.js`).
#[wasm_bindgen]
pub fn geo_to_canvas(registration_json: &str, points_json: &str) -> String {
    let reg: geopic_core::Registration = match serde_json::from_str(registration_json) {
        Ok(r) => r,
        Err(e) => {
            return geo_error_json(&geopic_core::GeoError::new(
                "invalid_options",
                format!("bad registration: {e}"),
            ));
        }
    };
    let pts: Vec<[f64; 2]> = match serde_json::from_str(points_json) {
        Ok(p) => p,
        Err(e) => {
            return geo_error_json(&geopic_core::GeoError::new(
                "invalid_options",
                format!("points must be [[lon, lat], …]: {e}"),
            ));
        }
    };
    let out: Vec<[f64; 2]> = pts
        .into_iter()
        .map(|[lon, lat]| reg.to_canvas(geopic_core::LonLat { lon, lat }))
        .collect();
    serde_json::to_string(&out).unwrap_or_else(|_| "[]".to_string())
}
