//! **geopic-core** — georeference an [rpic](https://rpic.dev) *scene*: turn a
//! pic drawing into a GeoJSON `FeatureCollection` in WGS84.
//!
//! Input is the scene document rpic emits (`rpic --scene`,
//! `compile(src, scene=True)`; format described in the engine's
//! `docs/scene.md`): the resolved draw list in inches, y up, with the user's
//! numeric variables under `vars`. The drawing is placed on the Earth by a
//! four-parameter similarity (anchor, scale in metres per inch, bearing) —
//! declared through pic variables (`geo_lon`, `geo_lat`, `geo_scale`,
//! `geo_bearing`, `geo_frame`) or recovered from control points
//! (`<Label>_lon` / `<Label>_lat` paired with labels of the scene) — and
//! every geometry is densified in the plane before being carried to
//! longitude/latitude through the chosen frame: a local tangent plane
//! (`enu`, geodesic direct problem) or Web Mercator (`mercator`).
//!
//! The crate is pure Rust, does no I/O and knows nothing about the engine
//! beyond the scene's JSON schema; it builds for `wasm32-unknown-unknown`.
//! Everything that cannot be represented raises a structured [`Warning`];
//! nothing is dropped silently.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod convert;
mod densify;
mod geodesy;
mod geojson;
mod register;
mod scene;
#[cfg(test)]
mod tests;

use std::fmt;

pub use convert::KINDS;
pub use geodesy::{Frame, LonLat};
pub use geojson::{Feature, FeatureCollection, Geometry};
pub use register::{Method, Registration};
pub use scene::{Scene, Span};

/// Version of the `geopic:*` property vocabulary every feature carries as
/// `geopic:schema`. Bumped only on a breaking change of a property's meaning.
pub const GEOPIC_SCHEMA: u32 = 1;

/// A named geographic feature the host knows about (a municipality, a
/// station…). Its `name` is matched against the scene's labels — a place
/// `Name: (x, y)` the host injected before compiling, or a labelled object —
/// and then acts as a control point of the registration.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedFeature {
    pub name: String,
    pub lon: f64,
    pub lat: f64,
    /// `[west, south, east, north]` in degrees, when the feature has extent.
    pub bbox: Option<[f64; 4]>,
}

/// Options for [`georeference`]. Every `Option` overrides the corresponding
/// pic variable (the `-D` semantics of troff: the caller wins over the
/// source); `None` leaves the source's value in force.
#[derive(Debug, Clone, PartialEq)]
pub struct GeoOptions {
    /// Projection frame (`geo_frame`: 0 = ENU, 1 = Mercator).
    pub frame: Option<Frame>,
    /// Geographic position of the pic origin `(0, 0)` (`geo_lon`/`geo_lat`).
    pub anchor: Option<LonLat>,
    /// Metres per inch of the drawing as authored (`geo_scale`, default 100).
    pub scale: Option<f64>,
    /// Azimuth of the drawing's +y axis, degrees clockwise from north
    /// (`geo_bearing`, default 0).
    pub bearing: Option<f64>,
    /// Host-provided named features (control points by label).
    pub features: Vec<NamedFeature>,
    /// Flattening tolerance for cubic curves, inches (default `1e-3`).
    pub flatten: f64,
    /// Pixels per inch when the source sets `geo_px = 1` (lengths are then
    /// read in pixels at this view scale: `geo_scale` is metres per pixel).
    pub view_scale: Option<f64>,
    /// `class` words that mark an item as screen-anchored
    /// (`geopic:placement = "screen"`); default `["screen"]`.
    pub screen_classes: Vec<String>,
}

impl Default for GeoOptions {
    fn default() -> Self {
        Self {
            frame: None,
            anchor: None,
            scale: None,
            bearing: None,
            features: Vec::new(),
            flatten: 1e-3,
            view_scale: None,
            screen_classes: vec!["screen".to_string()],
        }
    }
}

/// A non-fatal loss or substitution, never silent.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Warning {
    /// Stable machine-readable kind (`unsupported_style`, `ignored`,
    /// `registration_residual`, `ignored_with_control_points`,
    /// `control_point_unresolved`, `control_point_overridden`,
    /// `unsupported_kind`, `densification_budget`, …).
    pub kind: String,
    pub message: String,
    /// The scene item concerned, when there is one (`s<N>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Its source span, when the scene carried one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl Warning {
    fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            message: message.into(),
            id: None,
            span: None,
        }
    }

    fn on(mut self, item: &scene::Item) -> Self {
        self.id = Some(item.id.clone());
        self.span = item.span.clone();
        self
    }
}

/// A fatal, structured error.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GeoError {
    /// `no_registration`, `invalid_scale`, `invalid_anchor`, `invalid_scene`,
    /// `invalid_flatten`, `view_scale_required`, `degenerate_control_points`.
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl GeoError {
    fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            message: message.into(),
            hint: None,
        }
    }

    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for GeoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.kind)?;
        if let Some(h) = &self.hint {
            write!(f, " — {h}")?;
        }
        Ok(())
    }
}

impl std::error::Error for GeoError {}

/// The result of [`georeference`].
#[derive(Debug, Clone, PartialEq)]
pub struct GeoScene {
    pub features: FeatureCollection,
    pub warnings: Vec<Warning>,
    pub registration: Registration,
}

/// Parse a scene document (format 1). Unknown fields are ignored, so newer
/// additive fields never break this crate.
pub fn parse_scene(json: &str) -> Result<Scene, GeoError> {
    let scene: Scene = serde_json::from_str(json)
        .map_err(|e| GeoError::new("invalid_scene", format!("scene JSON does not parse: {e}")))?;
    if scene.scene != 1 {
        return Err(GeoError::new(
            "invalid_scene",
            format!(
                "unsupported scene format {} (this crate reads format 1)",
                scene.scene
            ),
        ));
    }
    if !(scene.rescale.is_finite() && scene.rescale > 0.0) {
        return Err(GeoError::new(
            "invalid_scene",
            format!(
                "scene.rescale must be a positive number, got {}",
                scene.rescale
            ),
        ));
    }
    Ok(scene)
}

/// Georeference a scene document given as JSON.
pub fn georeference(scene_json: &str, opts: &GeoOptions) -> Result<GeoScene, GeoError> {
    georeference_scene(&parse_scene(scene_json)?, opts)
}

/// Georeference an already-parsed scene.
pub fn georeference_scene(scene: &Scene, opts: &GeoOptions) -> Result<GeoScene, GeoError> {
    if !(opts.flatten.is_finite() && opts.flatten > 0.0) {
        return Err(GeoError::new(
            "invalid_flatten",
            format!(
                "flatten must be a positive tolerance in inches, got {}",
                opts.flatten
            ),
        )
        .hint("the default is 1e-3"));
    }
    let mut warnings = Vec::new();
    let registration = register::register(scene, opts, &mut warnings)?;
    let features = convert::convert(scene, &registration, opts, &mut warnings);
    Ok(GeoScene {
        features,
        warnings,
        registration,
    })
}
