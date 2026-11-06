//! WGS84 geodesy: the two frames a registration can use and the primitives
//! the rest of the crate builds on.
//!
//! * `enu` — a local tangent plane at the anchor. Plane offsets (east, north)
//!   in metres go to the ellipsoid through the geodesic **direct** problem
//!   (azimuth = atan2(e, n), distance = hypot(e, n)) and come back through
//!   the **inverse** problem. Karney's algorithms via `geographiclib-rs`:
//!   accurate to nanometres, valid at any distance.
//! * `mercator` — a similarity in EPSG:3857 metres and the Web Mercator
//!   inverse. Exact w.r.t. the projection, so straight lines of the drawing
//!   stay straight on a web map.

use crate::GeoError;
use geographiclib_rs::{DirectGeodesic, Geodesic, InverseGeodesic};
use std::sync::OnceLock;

/// Earth radius of the spherical Web Mercator projection (EPSG:3857), metres.
pub const MERCATOR_RADIUS: f64 = 6_378_137.0;

/// Projection frame of a registration (`geo_frame`: 0 = ENU, 1 = Mercator).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Frame {
    #[default]
    Enu,
    Mercator,
}

impl Frame {
    /// The `geo_frame` variable's encoding; anything else is `None`.
    pub fn from_var(v: f64) -> Option<Frame> {
        if v == 0.0 {
            Some(Frame::Enu)
        } else if v == 1.0 {
            Some(Frame::Mercator)
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Frame::Enu => "enu",
            Frame::Mercator => "mercator",
        }
    }
}

/// A WGS84 position, degrees.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct LonLat {
    pub lon: f64,
    pub lat: f64,
}

impl LonLat {
    /// Validate a position usable as an anchor or control point: finite,
    /// `|lat| < 90` (the tangent plane and the azimuth are undefined at the
    /// poles) and `lon` within `[-180, 180]`.
    pub fn checked(lon: f64, lat: f64) -> Result<LonLat, GeoError> {
        if !(lon.is_finite() && lat.is_finite()) {
            return Err(GeoError::new(
                "invalid_anchor",
                format!("non-finite coordinate ({lon}, {lat})"),
            ));
        }
        if lat.abs() >= 90.0 {
            return Err(GeoError::new(
                "invalid_anchor",
                format!("latitude {lat} is at or beyond a pole; geopic needs |lat| < 90"),
            )
            .hint("the local frame and the bearing are undefined at the poles"));
        }
        if lon.abs() > 180.0 {
            return Err(GeoError::new(
                "invalid_anchor",
                format!("longitude {lon} is outside [-180, 180]"),
            )
            .hint("normalize the longitude (e.g. 190 → -170)"));
        }
        Ok(LonLat { lon, lat })
    }
}

fn wgs84() -> &'static Geodesic {
    static G: OnceLock<Geodesic> = OnceLock::new();
    G.get_or_init(Geodesic::wgs84)
}

/// Wrap a longitude into `(-180, 180]`.
pub fn normalize_lon(lon: f64) -> f64 {
    let mut l = (lon + 180.0).rem_euclid(360.0) - 180.0;
    if l == -180.0 {
        l = 180.0;
    }
    l
}

/// Direct problem: the point `east`/`north` metres from `anchor` in its
/// tangent plane (azimuth `atan2(e, n)`, distance `hypot(e, n)`).
pub fn enu_to_lonlat(anchor: LonLat, east: f64, north: f64) -> LonLat {
    let s = east.hypot(north);
    if s == 0.0 {
        return anchor;
    }
    let az = east.atan2(north).to_degrees();
    let (lat2, lon2): (f64, f64) = wgs84().direct(anchor.lat, anchor.lon, az, s);
    LonLat {
        lon: normalize_lon(lon2),
        lat: lat2,
    }
}

/// Inverse problem: `(east, north)` metres of `p` in `anchor`'s tangent plane.
pub fn lonlat_to_enu(anchor: LonLat, p: LonLat) -> (f64, f64) {
    let (s12, azi1, _azi2, _a12): (f64, f64, f64, f64) =
        wgs84().inverse(anchor.lat, anchor.lon, p.lat, p.lon);
    let az = azi1.to_radians();
    (s12 * az.sin(), s12 * az.cos())
}

/// Geodesic distance between two positions, metres.
pub fn geodesic_distance(a: LonLat, b: LonLat) -> f64 {
    wgs84().inverse(a.lat, a.lon, b.lat, b.lon)
}

/// Web Mercator forward: degrees → EPSG:3857 metres.
pub fn mercator_forward(p: LonLat) -> [f64; 2] {
    let x = MERCATOR_RADIUS * p.lon.to_radians();
    let phi = p.lat.to_radians();
    let y = MERCATOR_RADIUS * (std::f64::consts::FRAC_PI_4 + phi / 2.0).tan().ln();
    [x, y]
}

/// Web Mercator inverse: EPSG:3857 metres → degrees.
pub fn mercator_inverse(m: [f64; 2]) -> LonLat {
    let lon = (m[0] / MERCATOR_RADIUS).to_degrees();
    let lat =
        (2.0 * (m[1] / MERCATOR_RADIUS).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
    LonLat {
        lon: normalize_lon(lon),
        lat,
    }
}
