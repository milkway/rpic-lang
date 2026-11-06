//! Densification in the plane, before de-projection: a straight segment of
//! the drawing is not a geodesic (nor a straight Mercator line, in the ENU
//! frame), so its interior must be sampled as plane points and each carried
//! to the ellipsoid separately. Curves are flattened in canvas inches first.

use crate::register::Registration;
use crate::scene::Path;

/// Longest plane step between consecutive samples, metres.
pub const MAX_STEP_M: f64 = 100.0;
/// Shortest step worth taking, metres (short segments stay short).
pub const MIN_STEP_M: f64 = 1.0;
/// Most vertices one segment may receive (a 10 000 km segment at 100 m).
pub const MAX_SEGMENT_VERTICES: usize = 100_000;
/// Total densification budget for one scene: a hostile source (huge
/// coordinates or `geo_scale`) cannot amplify the output unboundedly. When
/// it runs out, remaining segments keep only their endpoints and the
/// converter raises `densification_budget` — never silently.
pub const MAX_VERTICES: usize = 2_000_000;

/// Flatten one cubic Bézier to within `tol` (canvas inches), appending the
/// interior and end points (the start is assumed already present).
pub fn flatten_cubic(
    p0: [f64; 2],
    c1: [f64; 2],
    c2: [f64; 2],
    p3: [f64; 2],
    tol: f64,
    out: &mut Vec<[f64; 2]>,
) {
    fn rec(
        p0: [f64; 2],
        c1: [f64; 2],
        c2: [f64; 2],
        p3: [f64; 2],
        tol: f64,
        depth: u32,
        out: &mut Vec<[f64; 2]>,
    ) {
        // flatness: distance of the control points from the chord
        let d = |p: [f64; 2]| {
            let (vx, vy) = (p3[0] - p0[0], p3[1] - p0[1]);
            let len = vx.hypot(vy);
            if len == 0.0 {
                return (p[0] - p0[0]).hypot(p[1] - p0[1]);
            }
            ((p[0] - p0[0]) * vy - (p[1] - p0[1]) * vx).abs() / len
        };
        if depth >= 16 || d(c1).max(d(c2)) <= tol {
            out.push(p3);
            return;
        }
        // de Casteljau split at t = ½
        let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        let p01 = mid(p0, c1);
        let p12 = mid(c1, c2);
        let p23 = mid(c2, p3);
        let p012 = mid(p01, p12);
        let p123 = mid(p12, p23);
        let m = mid(p012, p123);
        rec(p0, p01, p012, m, tol, depth + 1, out);
        rec(m, p123, p23, p3, tol, depth + 1, out);
    }
    rec(p0, c1, c2, p3, tol.max(1e-9), 0, out);
}

/// The vertices of a scene path as a canvas polyline (closed paths repeat
/// their first vertex at the end).
pub fn path_points(path: &Path, tol: f64) -> Vec<[f64; 2]> {
    let mut pts: Vec<[f64; 2]> = if let Some(p) = &path.points {
        p.clone()
    } else {
        let mut out = Vec::new();
        if let Some(start) = path.start {
            out.push(start);
            let mut cur = start;
            for seg in path.cubics.iter().flatten() {
                flatten_cubic(cur, seg[0], seg[1], seg[2], tol, &mut out);
                cur = seg[2];
            }
        }
        out
    };
    if path.closed {
        close_ring(&mut pts);
    }
    pts
}

/// Make a ring explicit (first == last).
pub fn close_ring(pts: &mut Vec<[f64; 2]>) {
    if let (Some(&f), Some(&l)) = (pts.first(), pts.last())
        && (f != l)
    {
        pts.push(f);
    }
}

/// Subdivide every segment of a canvas polyline so no plane step exceeds
/// [`MAX_STEP_M`] (and none is shorter than [`MIN_STEP_M`]). `budget` is the
/// remaining vertex allowance for the whole scene ([`MAX_VERTICES`]); it is
/// drawn down and, once empty, segments keep only their endpoints. Returns
/// whether the budget limited this polyline.
pub fn densify_polyline(
    pts: &[[f64; 2]],
    reg: &Registration,
    budget: &mut usize,
) -> (Vec<[f64; 2]>, bool) {
    let mut out = Vec::with_capacity(pts.len());
    let mut limited = false;
    if let Some(&first) = pts.first() {
        out.push(first);
    }
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len_in = (b[0] - a[0]).hypot(b[1] - a[1]);
        let len_m = reg.metres(len_in);
        let wanted = if len_m.is_finite() {
            let step = (len_m / 100.0).clamp(MIN_STEP_M, MAX_STEP_M);
            let n = (len_m / step).ceil();
            if n.is_finite() {
                (n as usize).clamp(1, MAX_SEGMENT_VERTICES)
            } else {
                MAX_SEGMENT_VERTICES
            }
        } else {
            1
        };
        let n = wanted.min((*budget).max(1));
        if n < wanted {
            limited = true;
        }
        *budget = budget.saturating_sub(n);
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
        }
    }
    (out, limited)
}

/// Vertex count for a closed curve of plane radius `r_m` metres: a 100 m
/// chord, clamped to 32..=128 (sagitta ≈ r(1 − cos(π/n))).
pub fn arc_vertices(r_m: f64) -> usize {
    let n = (2.0 * std::f64::consts::PI * r_m / MAX_STEP_M).ceil();
    if n.is_finite() {
        (n as usize).clamp(32, 128)
    } else {
        128
    }
}

/// Ellipse (or circle, `rx == ry`) outline as a closed canvas ring.
pub fn ellipse_ring(c: [f64; 2], rx: f64, ry: f64, reg: &Registration) -> Vec<[f64; 2]> {
    let n = arc_vertices(reg.metres(rx.max(ry)));
    let mut pts: Vec<[f64; 2]> = (0..n)
        .map(|k| {
            let t = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
            [c[0] + rx * t.cos(), c[1] + ry * t.sin()]
        })
        .collect();
    close_ring(&mut pts);
    pts
}

/// Axis-aligned box outline (corner radius `rad`, 0 = sharp) as a closed
/// canvas ring, counter-clockwise from the south-west corner.
pub fn box_ring(c: [f64; 2], w: f64, h: f64, rad: f64) -> Vec<[f64; 2]> {
    let (hw, hh) = (w / 2.0, h / 2.0);
    let r = rad.max(0.0).min(hw).min(hh);
    let mut pts = Vec::new();
    if r <= 0.0 {
        pts.extend([
            [c[0] - hw, c[1] - hh],
            [c[0] + hw, c[1] - hh],
            [c[0] + hw, c[1] + hh],
            [c[0] - hw, c[1] + hh],
        ]);
    } else {
        // corner centres and the quarter each covers, CCW from SW
        let corners = [
            ([c[0] - hw + r, c[1] - hh + r], 180.0_f64),
            ([c[0] + hw - r, c[1] - hh + r], 270.0),
            ([c[0] + hw - r, c[1] + hh - r], 0.0),
            ([c[0] - hw + r, c[1] + hh - r], 90.0),
        ];
        for (cc, start) in corners {
            for k in 0..=8 {
                let a = (start + 90.0 * k as f64 / 8.0).to_radians();
                pts.push([cc[0] + r * a.cos(), cc[1] + r * a.sin()]);
            }
        }
    }
    close_ring(&mut pts);
    pts
}
