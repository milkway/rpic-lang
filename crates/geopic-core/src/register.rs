//! Registration: the four-parameter similarity (anchor, scale, bearing) that
//! places the drawing on the Earth, and how it is obtained — declared by the
//! source's variables, or fitted to control points.

use crate::geodesy::{self, Frame, LonLat};
use crate::scene::Scene;
use crate::{GeoError, GeoOptions, NamedFeature, Warning};

/// Largest accepted effective scale, metres per emitted inch.
pub const MAX_SCALE_M_PER_IN: f64 = 1e9;

/// How the registration was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// `geo_lon`/`geo_lat` (+ `geo_scale`, `geo_bearing`), or a single
    /// control point used as the anchor.
    Anchor,
    /// Two control points: scale and bearing in closed form.
    TwoPoints,
    /// Three or more control points: least-squares Helmert (similarity).
    Helmert,
}

/// The similarity canvas (inches) → Earth.
///
/// A canvas point `p` maps to plane offsets `(east, north)` in metres:
/// `d = (p - origin) · scale_m_per_in`, then rotated so the canvas +y axis
/// points to azimuth `bearing_deg`; the offsets are carried to the ellipsoid
/// by the frame ([`Frame`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Registration {
    pub frame: Frame,
    /// Geographic position of `origin`.
    pub anchor: LonLat,
    /// The canvas point (inches) that sits at `anchor`.
    pub origin: [f64; 2],
    /// Metres per inch of the **emitted** geometry (the declared `geo_scale`
    /// already divided by the scene's `rescale`, and multiplied by
    /// `view_scale` under `geo_px = 1`).
    pub scale_m_per_in: f64,
    /// Azimuth of the canvas +y axis, degrees clockwise from north.
    pub bearing_deg: f64,
    pub method: Method,
    /// Control points used by the fit (0 for a declared anchor).
    pub control_points: usize,
    /// Root-mean-square residual of the fit, metres (fits only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rms_m: Option<f64>,
}

impl Registration {
    /// Canvas → plane offsets (east, north) in metres.
    pub fn plane_offset(&self, p: [f64; 2]) -> (f64, f64) {
        let dx = (p[0] - self.origin[0]) * self.scale_m_per_in;
        let dy = (p[1] - self.origin[1]) * self.scale_m_per_in;
        let b = self.bearing_deg.to_radians();
        (dx * b.cos() + dy * b.sin(), -dx * b.sin() + dy * b.cos())
    }

    /// Plane offsets → canvas.
    pub fn offset_to_canvas(&self, east: f64, north: f64) -> [f64; 2] {
        let b = self.bearing_deg.to_radians();
        let dx = east * b.cos() - north * b.sin();
        let dy = east * b.sin() + north * b.cos();
        [
            self.origin[0] + dx / self.scale_m_per_in,
            self.origin[1] + dy / self.scale_m_per_in,
        ]
    }

    /// Canvas point → WGS84.
    pub fn to_lonlat(&self, p: [f64; 2]) -> LonLat {
        let (e, n) = self.plane_offset(p);
        match self.frame {
            Frame::Enu => geodesy::enu_to_lonlat(self.anchor, e, n),
            Frame::Mercator => {
                let a = geodesy::mercator_forward(self.anchor);
                geodesy::mercator_inverse([a[0] + e, a[1] + n])
            }
        }
    }

    /// WGS84 → canvas point (the exact inverse of [`Self::to_lonlat`]).
    pub fn to_canvas(&self, p: LonLat) -> [f64; 2] {
        let (e, n) = match self.frame {
            Frame::Enu => geodesy::lonlat_to_enu(self.anchor, p),
            Frame::Mercator => {
                let a = geodesy::mercator_forward(self.anchor);
                let m = geodesy::mercator_forward(p);
                (m[0] - a[0], m[1] - a[1])
            }
        };
        self.offset_to_canvas(e, n)
    }

    /// Length of one canvas inch in metres along the plane — the scale.
    pub fn metres(&self, inches: f64) -> f64 {
        inches * self.scale_m_per_in
    }
}

/// A control point: a canvas position and where it is on the Earth.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Control {
    canvas: [f64; 2],
    geo: LonLat,
}

fn var(scene: &Scene, name: &str) -> Option<f64> {
    scene.vars.get(name).copied()
}

/// Obtain the registration for `scene` under `opts`.
pub fn register(
    scene: &Scene,
    opts: &GeoOptions,
    warnings: &mut Vec<Warning>,
) -> Result<Registration, GeoError> {
    // frame
    let frame = match opts.frame {
        Some(f) => f,
        None => match var(scene, "geo_frame") {
            None => Frame::Enu,
            Some(v) => Frame::from_var(v).unwrap_or_else(|| {
                warnings.push(Warning::new(
                    "invalid_frame",
                    format!("geo_frame = {v} is neither 0 (enu) nor 1 (mercator); using enu"),
                ));
                Frame::Enu
            }),
        },
    };

    // declared scale and bearing — only *used* when no fit supersedes them,
    // so their validation happens where they are consumed
    let declared_scale = opts.scale.or_else(|| var(scene, "geo_scale"));
    let declared_bearing = opts.bearing.or_else(|| var(scene, "geo_bearing"));
    let px_mode = var(scene, "geo_px").is_some_and(|v| v != 0.0);
    let declared_scale_m_per_in = || -> Result<f64, GeoError> {
        let scale_value = declared_scale.unwrap_or(100.0);
        if !(scale_value.is_finite() && scale_value > 0.0) {
            return Err(GeoError::new(
                "invalid_scale",
                format!(
                    "geo_scale must be a positive number of metres per inch, got {scale_value}"
                ),
            ));
        }
        let per_inch = if px_mode {
            match opts.view_scale {
                Some(v) if v.is_finite() && v > 0.0 => scale_value * v,
                _ => {
                    return Err(GeoError::new(
                        "view_scale_required",
                        "the source sets geo_px = 1 (lengths in pixels) but no positive view_scale was given",
                    )
                    .hint("pass view_scale (pixels per inch of the rendered view, e.g. 96)"));
                }
            }
        } else {
            scale_value
        };
        // `vars` are pre-rescale; the geometry is post-rescale
        Ok(per_inch / scene.rescale)
    };
    let declared_bearing_deg = || -> Result<f64, GeoError> {
        let b = declared_bearing.unwrap_or(0.0);
        if !b.is_finite() {
            return Err(GeoError::new(
                "invalid_scale",
                format!("geo_bearing must be finite, got {b}"),
            ));
        }
        Ok(b)
    };

    // control points: host features first (the caller overrides the source,
    // like -D in troff), then `<L>_lon`/`<L>_lat` variables
    let mut controls: Vec<Control> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for NamedFeature { name, lon, lat, .. } in &opts.features {
        if seen.iter().any(|s| s == name) {
            continue;
        }
        if let Some(canvas) = scene.point_of_label(name) {
            controls.push(Control {
                canvas,
                geo: LonLat::checked(*lon, *lat)?,
            });
            seen.push(name.clone());
        }
    }
    for (name, lon) in scene
        .vars
        .iter()
        .filter_map(|(k, v)| k.strip_suffix("_lon").map(|n| (n, *v)))
    {
        if name == "geo" {
            continue;
        }
        let Some(lat) = var(scene, &format!("{name}_lat")) else {
            warnings.push(Warning::new(
                "control_point_unresolved",
                format!("{name}_lon is set but {name}_lat is not; ignored"),
            ));
            continue;
        };
        if seen.iter().any(|s| s == name) {
            warnings.push(Warning::new(
                "control_point_overridden",
                format!("{name}_lon/{name}_lat in the source are overridden by the host's feature {name}"),
            ));
            continue;
        }
        match scene.point_of_label(name) {
            Some(canvas) => {
                controls.push(Control {
                    canvas,
                    geo: LonLat::checked(lon, lat)?,
                });
                seen.push(name.to_string());
            }
            None => warnings.push(Warning::new(
                "control_point_unresolved",
                format!("{name}_lon/{name}_lat are set but no place or object is labelled {name}; ignored"),
            )),
        }
    }

    // declared anchor
    let anchor = match opts.anchor {
        Some(a) => Some(LonLat::checked(a.lon, a.lat)?),
        None => match (var(scene, "geo_lon"), var(scene, "geo_lat")) {
            (Some(lon), Some(lat)) => Some(LonLat::checked(lon, lat)?),
            (Some(_), None) | (None, Some(_)) => {
                warnings.push(Warning::new(
                    "control_point_unresolved",
                    "only one of geo_lon/geo_lat is set; the anchor needs both",
                ));
                None
            }
            (None, None) => None,
        },
    };

    let reg = match controls.len() {
        0 => {
            let Some(anchor) = anchor else {
                return Err(GeoError::new(
                    "no_registration",
                    "nothing places this drawing on the Earth",
                )
                .hint(
                    "set geo_lon/geo_lat (+ geo_scale in metres per inch, geo_bearing) in the source \
                     or pass anchor=, or give control points <Label>_lon/<Label>_lat for labelled places",
                ));
            };
            Registration {
                frame,
                anchor,
                origin: [0.0, 0.0],
                scale_m_per_in: declared_scale_m_per_in()?,
                bearing_deg: declared_bearing_deg()?,
                method: Method::Anchor,
                control_points: 0,
                rms_m: None,
            }
        }
        1 => {
            if anchor.is_some() {
                warnings.push(Warning::new(
                    "ignored_with_control_points",
                    "geo_lon/geo_lat ignored: the single control point is the anchor",
                ));
            }
            let c = controls[0];
            Registration {
                frame,
                anchor: c.geo,
                origin: c.canvas,
                scale_m_per_in: declared_scale_m_per_in()?,
                bearing_deg: declared_bearing_deg()?,
                method: Method::Anchor,
                control_points: 1,
                rms_m: None,
            }
        }
        n => {
            if declared_scale.is_some() || declared_bearing.is_some() || px_mode {
                warnings.push(Warning::new(
                    "ignored_with_control_points",
                    "geo_scale/geo_bearing/geo_px ignored: scale and bearing come from the control points",
                ));
            }
            if anchor.is_some() {
                warnings.push(Warning::new(
                    "ignored_with_control_points",
                    "geo_lon/geo_lat ignored: the anchor comes from the control points",
                ));
            }
            let mut reg = fit(frame, &controls)?;
            reg.method = if n == 2 {
                Method::TwoPoints
            } else {
                Method::Helmert
            };
            reg.control_points = n;
            // honest residual: geodesic distance between where each control
            // point lands and where it should be
            let sum: f64 = controls
                .iter()
                .map(|c| geodesy::geodesic_distance(reg.to_lonlat(c.canvas), c.geo).powi(2))
                .sum();
            let rms = (sum / n as f64).sqrt();
            reg.rms_m = Some(rms);
            let extent_m = scene.canvas.w.max(scene.canvas.h) * reg.scale_m_per_in;
            if n > 2 && rms > 0.01 * extent_m {
                warnings.push(Warning::new(
                    "registration_residual",
                    format!(
                        "control points disagree with a similarity: RMS residual {rms:.2} m exceeds 1% of the drawing's extent ({extent_m:.1} m)"
                    ),
                ));
            }
            reg
        }
    };
    // one inch longer than 1e9 m (25 Earth circumferences) is not a map —
    // declared or fitted
    if !(reg.scale_m_per_in.is_finite() && reg.scale_m_per_in <= MAX_SCALE_M_PER_IN) {
        return Err(GeoError::new(
            "invalid_scale",
            format!(
                "effective scale {} m per inch exceeds the {MAX_SCALE_M_PER_IN} m limit",
                reg.scale_m_per_in
            ),
        ));
    }
    Ok(reg)
}

/// Least-squares similarity (Helmert, 4 parameters) through the control
/// points; exact for two. Plane coordinates are taken about a provisional
/// anchor and the fit is repeated once about the fitted anchor, so the ENU
/// plane used for fitting is the one the registration will use.
fn fit(frame: Frame, controls: &[Control]) -> Result<Registration, GeoError> {
    let first = controls
        .first()
        .ok_or_else(|| GeoError::new("no_registration", "no control points"))?;
    let mut anchor = first.geo;
    let mut reg = None;
    for _ in 0..2 {
        let plane: Vec<(f64, f64)> = controls
            .iter()
            .map(|c| match frame {
                Frame::Enu => geodesy::lonlat_to_enu(anchor, c.geo),
                Frame::Mercator => {
                    let a = geodesy::mercator_forward(anchor);
                    let m = geodesy::mercator_forward(c.geo);
                    (m[0] - a[0], m[1] - a[1])
                }
            })
            .collect();
        let n = controls.len() as f64;
        let (px, py) = controls
            .iter()
            .fold((0.0, 0.0), |(x, y), c| (x + c.canvas[0], y + c.canvas[1]));
        let (px, py) = (px / n, py / n);
        let (qe, qn) = plane
            .iter()
            .fold((0.0, 0.0), |(e, nn), q| (e + q.0, nn + q.1));
        let (qe, qn) = (qe / n, qn / n);
        let (mut num_a, mut num_b, mut den) = (0.0, 0.0, 0.0);
        for (c, q) in controls.iter().zip(&plane) {
            let dx = c.canvas[0] - px;
            let dy = c.canvas[1] - py;
            let ex = q.0 - qe;
            let ny = q.1 - qn;
            num_a += dx * ex + dy * ny;
            num_b += dx * ny - dy * ex;
            den += dx * dx + dy * dy;
        }
        if den <= 0.0 || !den.is_finite() {
            return Err(GeoError::new(
                "degenerate_control_points",
                "the control points coincide on the canvas; a similarity cannot be fitted",
            ));
        }
        // plane = s·R·(canvas − p̄) + q̄ with R = [[a, −b], [b, a]]/s
        let a = num_a / den;
        let b = num_b / den;
        let scale = a.hypot(b);
        if !(scale.is_finite() && scale > 0.0) {
            return Err(GeoError::new(
                "degenerate_control_points",
                "the control points coincide on the Earth; a similarity cannot be fitted",
            ));
        }
        // east = s(dx cos β + dy sin β), north = s(−dx sin β + dy cos β)
        // ⇒ a = s cos β, b = −s sin β
        let bearing = (-b).atan2(a).to_degrees();
        let new_anchor = match frame {
            Frame::Enu => geodesy::enu_to_lonlat(anchor, qe, qn),
            Frame::Mercator => {
                let a0 = geodesy::mercator_forward(anchor);
                geodesy::mercator_inverse([a0[0] + qe, a0[1] + qn])
            }
        };
        reg = Some(Registration {
            frame,
            anchor: new_anchor,
            origin: [px, py],
            scale_m_per_in: scale,
            bearing_deg: bearing,
            method: Method::Helmert,
            control_points: controls.len(),
            rms_m: None,
        });
        anchor = new_anchor;
    }
    reg.ok_or_else(|| GeoError::new("no_registration", "fit did not converge"))
}
