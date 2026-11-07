//! Minimal GeoJSON (RFC 7946) output types: `Point`, `LineString` and
//! `Polygon` features in a `FeatureCollection`. Hand-rolled on serde so the
//! crate carries no GeoJSON dependency and the output is deterministic.

use serde::Serialize;
use serde::ser::{SerializeStruct, Serializer};

/// A feature geometry, WGS84 `[lon, lat]` positions.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    Point([f64; 2]),
    LineString(Vec<[f64; 2]>),
    /// Rings; the first is the exterior, closed (first == last).
    Polygon(Vec<Vec<[f64; 2]>>),
}

impl Serialize for Geometry {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("Geometry", 2)?;
        match self {
            Geometry::Point(p) => {
                st.serialize_field("type", "Point")?;
                st.serialize_field("coordinates", p)?;
            }
            Geometry::LineString(pts) => {
                st.serialize_field("type", "LineString")?;
                st.serialize_field("coordinates", pts)?;
            }
            Geometry::Polygon(rings) => {
                st.serialize_field("type", "Polygon")?;
                st.serialize_field("coordinates", rings)?;
            }
        }
        st.end()
    }
}

/// A GeoJSON feature. Properties follow the simplestyle-spec keys plus the
/// `geopic:*` namespace (see `docs/geopic.md`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Feature {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub geometry: Geometry,
    pub properties: serde_json::Map<String, serde_json::Value>,
}

impl Feature {
    pub fn new(geometry: Geometry, properties: serde_json::Map<String, serde_json::Value>) -> Self {
        Self {
            type_: "Feature",
            geometry,
            properties,
        }
    }
}

/// A GeoJSON feature collection.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeatureCollection {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub features: Vec<Feature>,
    /// RFC 7946 §6.1 foreign member: `{"schema": 1, "registration": {…}}` —
    /// the similarity that placed the drawing, so a consumer can turn
    /// points into metres (`font_size` pt / 72 × `scale_m_per_in`) and
    /// size labels and strokes true to the ground.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geopic: Option<serde_json::Value>,
}

impl FeatureCollection {
    pub fn new(features: Vec<Feature>) -> Self {
        Self {
            type_: "FeatureCollection",
            features,
            geopic: None,
        }
    }

    /// Compact JSON text.
    pub fn to_json(&self) -> String {
        // serialization of plain structs/arrays/finite numbers cannot fail
        serde_json::to_string(self)
            .unwrap_or_else(|_| "{\"type\":\"FeatureCollection\",\"features\":[]}".to_string())
    }
}

impl Default for FeatureCollection {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}
