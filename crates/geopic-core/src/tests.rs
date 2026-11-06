//! Acceptance tests A1–A4 of the geopic prompt plus conversion checks.
//! Real pic sources go through the engine (`rpic-core`, dev-dependency).

use crate::geodesy::{self, Frame, LonLat};
use crate::register::{Method, Registration};
use crate::{GeoOptions, Geometry, georeference};
use serde_json::Value;

include!("geodtest_cases.inc");

fn scene_of(src: &str) -> String {
    let d = rpic_core::compile(src).expect("pic compiles");
    rpic_core::scene_json(&d, &rpic_core::SceneOptions::default()).json
}

fn reg(frame: Frame, anchor: LonLat, scale: f64, bearing: f64) -> Registration {
    Registration {
        frame,
        anchor,
        origin: [0.0, 0.0],
        scale_m_per_in: scale,
        bearing_deg: bearing,
        method: Method::Anchor,
        control_points: 0,
        rms_m: None,
    }
}

// ---- A1: canvas → lon/lat → canvas round trip -------------------------------

#[test]
fn a1_round_trip_enu_and_mercator_under_1mm() {
    let lats = [-60.0, -30.0, -8.0, 0.0, 45.0, 70.0];
    let dists_m = [10.0, 1_000.0, 10_000.0, 50_000.0];
    let scale = 1_000.0; // m per inch
    for frame in [Frame::Enu, Frame::Mercator] {
        for lat in lats {
            let anchor = LonLat { lon: -34.8811, lat };
            let r = reg(frame, anchor, scale, 17.0);
            for d in dists_m {
                for k in 0..8 {
                    let az = (k as f64) * 45.0_f64.to_radians();
                    let p = [d * az.sin() / scale, d * az.cos() / scale];
                    let ll = r.to_lonlat(p);
                    let back = r.to_canvas(ll);
                    let err_m = ((back[0] - p[0]).hypot(back[1] - p[1])) * scale;
                    // in the Mercator frame the plane metre is a Mercator metre
                    assert!(err_m < 1e-3, "{frame:?} lat={lat} d={d} az={k}: {err_m} m");
                }
            }
        }
    }
}

// ---- A2: the ENU frame reproduces Karney's direct problem -------------------

#[test]
fn a2_enu_matches_karney_direct_problem_under_1mm() {
    for (lat1, lon1, azi1, lat2, lon2, s12) in GEODTEST {
        let anchor = LonLat {
            lon: lon1,
            lat: lat1,
        };
        let az = azi1.to_radians();
        let got = geodesy::enu_to_lonlat(anchor, s12 * az.sin(), s12 * az.cos());
        let want = LonLat {
            lon: geodesy::normalize_lon(lon2),
            lat: lat2,
        };
        let err = geodesy::geodesic_distance(got, want);
        assert!(err < 1e-3, "case ({lat1},{azi1},{s12}): {err} m off");
    }
}

// ---- A3: registration from control points -----------------------------------

/// Synthetic truth: anchor at Recife, 250 m/in, bearing 37°.
fn truth(frame: Frame) -> Registration {
    reg(
        frame,
        LonLat {
            lon: -34.8811,
            lat: -8.0539,
        },
        250.0,
        37.0,
    )
}

fn scene_with_controls(frame: Frame, pts: &[([f64; 2], LonLat)], extra_vars: &str) -> String {
    let mut src = String::new();
    src.push_str(&format!(
        "geo_frame = {}\n",
        if frame == Frame::Mercator { 1 } else { 0 }
    ));
    src.push_str(extra_vars);
    for (i, (p, ll)) in pts.iter().enumerate() {
        src.push_str(&format!(
            "P{i}: ({}, {})\nP{i}_lon = {}\nP{i}_lat = {}\n",
            p[0], p[1], ll.lon, ll.lat
        ));
    }
    // something drawn that spans the control grid, so the scene's canvas
    // (and the 1 % residual threshold) reflect the drawing's real extent
    src.push_str("box wid 3.4 ht 2.6 at (1.6, 0.9)\n");
    scene_of(&src)
}

#[test]
fn a3_two_control_points_recover_the_exact_similarity() {
    for frame in [Frame::Enu, Frame::Mercator] {
        let t = truth(frame);
        let pts: Vec<([f64; 2], LonLat)> = [[0.2, 0.3], [2.5, 1.7]]
            .iter()
            .map(|&p| (p, t.to_lonlat(p)))
            .collect();
        let g = georeference(
            &scene_with_controls(frame, &pts, ""),
            &GeoOptions::default(),
        )
        .unwrap();
        let r = &g.registration;
        assert_eq!(r.method, Method::TwoPoints);
        assert_eq!(r.control_points, 2);
        assert!(
            (r.scale_m_per_in - 250.0).abs() < 1e-6,
            "{frame:?} scale {}",
            r.scale_m_per_in
        );
        // the bearing is measured from the anchor's meridian; the fitted anchor
        // is the control points' centroid, ~400 m east of the truth's, so in
        // the ENU frame the two bearings differ by the meridian convergence
        // (Δλ·sin φ ≈ 5e-4° here). Mercator north is the same everywhere.
        let tol = if frame == Frame::Mercator { 1e-6 } else { 1e-2 };
        assert!(
            (r.bearing_deg - 37.0).abs() < tol,
            "{frame:?} bearing {}",
            r.bearing_deg
        );
        assert!(r.rms_m.unwrap() < 1e-6);
        // the recovered map agrees with the truth away from the control points
        for p in [[0.0, 0.0], [3.0, -1.0], [1.0, 4.0]] {
            let d = geodesy::geodesic_distance(r.to_lonlat(p), t.to_lonlat(p));
            assert!(d < 1e-3, "{frame:?} {p:?}: {d} m");
        }
        assert!(
            g.warnings.iter().all(|w| w.kind != "registration_residual"),
            "{:?}",
            g.warnings
        );
    }
}

/// Deterministic Gaussian-ish noise (sum of 12 LCG uniforms), metres.
struct Noise(u64);
impl Noise {
    fn next(&mut self) -> f64 {
        let mut s = 0.0;
        for _ in 0..12 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            s += (self.0 >> 11) as f64 / (1u64 << 53) as f64;
        }
        s - 6.0 // mean 0, variance 1
    }
}

#[test]
fn a3_helmert_reports_the_residual_and_warns_above_threshold() {
    let t = truth(Frame::Enu);
    let mut noise = Noise(42);
    let sigma = 2.0;
    let mut pts = Vec::new();
    let mut true_sq = 0.0;
    for i in 0..20 {
        let p = [(i % 5) as f64 * 0.8, (i / 5) as f64 * 0.6];
        let (e, n) = t.plane_offset(p);
        let (de, dn) = (sigma * noise.next(), sigma * noise.next());
        true_sq += de * de + dn * dn;
        pts.push((p, geodesy::enu_to_lonlat(t.anchor, e + de, n + dn)));
    }
    let true_rms = (true_sq / 20.0).sqrt();
    // drawing extent is tiny (~1 in ≈ 250 m), so 1% = 2.5 m ≈ the noise: warn
    let g = georeference(
        &scene_with_controls(Frame::Enu, &pts, "geo_scale = 999\n"),
        &GeoOptions::default(),
    )
    .unwrap();
    let r = &g.registration;
    assert_eq!(r.method, Method::Helmert);
    let rms = r.rms_m.unwrap();
    assert!(
        (rms - true_rms).abs() <= 0.2 * true_rms,
        "rms {rms} vs true {true_rms}"
    );
    assert!(
        (r.scale_m_per_in - 250.0).abs() / 250.0 < 0.02,
        "{}",
        r.scale_m_per_in
    );
    assert!((r.bearing_deg - 37.0).abs() < 1.0, "{}", r.bearing_deg);
    assert!(
        g.warnings
            .iter()
            .any(|w| w.kind == "ignored_with_control_points"),
        "{:?}",
        g.warnings
    );
    // 2 m of noise on a ~800 m drawing stays under the 1 % threshold: no warning
    assert!(
        g.warnings.iter().all(|w| w.kind != "registration_residual"),
        "{:?}",
        g.warnings
    );
    // one gross outlier (a control point 300 m off) must trip it
    let mut bad = pts.clone();
    let (e, n) = t.plane_offset(bad[0].0);
    bad[0].1 = geodesy::enu_to_lonlat(t.anchor, e + 300.0, n);
    let g = georeference(
        &scene_with_controls(Frame::Enu, &bad, ""),
        &GeoOptions::default(),
    )
    .unwrap();
    assert!(g.registration.rms_m.unwrap() > 50.0);
    assert!(
        g.warnings.iter().any(|w| w.kind == "registration_residual"),
        "{:?}",
        g.warnings
    );
}

#[test]
fn a3_declared_anchor_without_controls_and_no_registration_error() {
    let g = georeference(
        &scene_of("geo_lon = -34.88\ngeo_lat = -8.05\ngeo_scale = 500\ngeo_bearing = 90\nbox"),
        &GeoOptions::default(),
    )
    .unwrap();
    assert_eq!(g.registration.method, Method::Anchor);
    assert_eq!(g.registration.scale_m_per_in, 500.0);
    assert_eq!(g.registration.bearing_deg, 90.0);
    let e = georeference(&scene_of("box"), &GeoOptions::default()).unwrap_err();
    assert_eq!(e.kind, "no_registration");
    assert!(e.hint.as_deref().unwrap_or("").contains("geo_lon"));
    // overrides win over the source
    let o = GeoOptions {
        anchor: Some(LonLat {
            lon: 10.0,
            lat: 20.0,
        }),
        scale: Some(1.0),
        ..GeoOptions::default()
    };
    let g = georeference(&scene_of("box"), &o).unwrap();
    assert_eq!(
        g.registration.anchor,
        LonLat {
            lon: 10.0,
            lat: 20.0
        }
    );
}

#[test]
fn rescale_is_folded_into_the_scale() {
    // `.PS 3` doubles a 1.5 in box: 100 m per authored inch = ~50 m per emitted inch,
    // and the box's east edge lands 150 m east of the origin either way
    let plain = georeference(
        &scene_of("geo_lon = 0\ngeo_lat = 0\nbox wid 1.5"),
        &GeoOptions::default(),
    )
    .unwrap();
    let scaled = georeference(
        &scene_of(".PS 3\ngeo_lon = 0\ngeo_lat = 0\nbox wid 1.5\n.PE"),
        &GeoOptions::default(),
    )
    .unwrap();
    assert!(scaled.registration.scale_m_per_in < 55.0 && scaled.registration.scale_m_per_in > 50.0);
    let east = |g: &crate::GeoScene| {
        let Geometry::Polygon(rings) = &g.features.features[0].geometry else {
            panic!()
        };
        rings[0].iter().map(|p| p[0]).fold(f64::MIN, f64::max)
    };
    let a = LonLat {
        lon: east(&plain),
        lat: 0.0,
    };
    let b = LonLat {
        lon: east(&scaled),
        lat: 0.0,
    };
    assert!(geodesy::geodesic_distance(a, b) < 0.01, "{a:?} vs {b:?}");
    assert!((geodesy::geodesic_distance(LonLat { lon: 0.0, lat: 0.0 }, a) - 150.0).abs() < 0.01);
}

// ---- A4: densification keeps a long straight line on the geodesic ----------

/// Hausdorff distance between two polylines (point-to-segment, both ways),
/// metres, measured in the tangent plane at `anchor`.
fn hausdorff_m(anchor: LonLat, a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    let to_enu = |p: &[f64; 2]| {
        geodesy::lonlat_to_enu(
            anchor,
            LonLat {
                lon: p[0],
                lat: p[1],
            },
        )
    };
    let ea: Vec<_> = a.iter().map(to_enu).collect();
    let eb: Vec<_> = b.iter().map(to_enu).collect();
    fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
        let (vx, vy) = (b.0 - a.0, b.1 - a.1);
        let len2 = vx * vx + vy * vy;
        let t = if len2 == 0.0 {
            0.0
        } else {
            (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / len2).clamp(0.0, 1.0)
        };
        (p.0 - (a.0 + t * vx)).hypot(p.1 - (a.1 + t * vy))
    }
    let dir = |x: &[(f64, f64)], y: &[(f64, f64)]| {
        x.iter()
            .map(|&p| {
                y.windows(2)
                    .map(|w| seg_dist(p, w[0], w[1]))
                    .fold(f64::MAX, f64::min)
            })
            .fold(0.0, f64::max)
    };
    dir(&ea, &eb).max(dir(&eb, &ea))
}

#[test]
fn a4_densified_line_follows_the_geodesic_and_the_control_does_not() {
    // `scale = 250` makes `line right 80` 0.32 in long; 62 500 m/in → 20 km.
    // Anchor at Porto Alegre, heading NE: far enough from the equator that a
    // straight line in lon/lat space visibly leaves the geodesic.
    let src = "scale = 250\ngeo_lon = -51.2287\ngeo_lat = -30.0346\ngeo_scale = 62500\ngeo_bearing = -45\nline right 80";
    let g = georeference(&scene_of(src), &GeoOptions::default()).unwrap();
    let Geometry::LineString(line) = &g.features.features[0].geometry else {
        panic!("{:?}", g.features.features[0].geometry)
    };
    assert!(line.len() > 100, "densified to {} points", line.len());
    let a = LonLat {
        lon: line[0][0],
        lat: line[0][1],
    };
    let b = LonLat {
        lon: line[line.len() - 1][0],
        lat: line[line.len() - 1][1],
    };
    let len = geodesy::geodesic_distance(a, b);
    assert!((len - 20_000.0).abs() < 1.0, "length {len}");
    // reference geodesic, sampled every 10 m
    let (e, n) = geodesy::lonlat_to_enu(a, b);
    let az = e.atan2(n);
    let reference: Vec<[f64; 2]> = (0..=2000)
        .map(|k| {
            let s = len * k as f64 / 2000.0;
            let p = geodesy::enu_to_lonlat(a, s * az.sin(), s * az.cos());
            [p.lon, p.lat]
        })
        .collect();
    let dense = hausdorff_m(a, line, &reference);
    assert!(
        dense <= 1.0,
        "densified line is {dense} m from the geodesic"
    );
    // control: the two endpoints alone, joined straight in lon/lat space
    let straight: Vec<[f64; 2]> = (0..=2000)
        .map(|k| {
            let t = k as f64 / 2000.0;
            [a.lon + t * (b.lon - a.lon), a.lat + t * (b.lat - a.lat)]
        })
        .collect();
    let coarse = hausdorff_m(a, &straight, &reference);
    assert!(
        coarse > 1.0,
        "the un-densified control only deviates {coarse} m — the test would prove nothing"
    );
}

// ---- conversion coverage -----------------------------------------------------

#[test]
fn every_scene_kind_is_handled_or_warned() {
    // the format's kind list; a new kind must land in convert::KINDS
    let documented = [
        "box", "circle", "ellipse", "arc", "line", "arrow", "spline", "text", "dot", "brace",
        "block",
    ];
    for k in documented {
        assert!(
            crate::convert::KINDS.contains(&k),
            "kind {k} missing from KINDS"
        );
    }
    let src = "geo_lon = 0\ngeo_lat = 0\nB: box \"hub\" fill 0.3\ncircle rad 0.2 at B.e + (0.5,0)\nellipse at (2,0)\n\
               arrow from (0,-1) to (1,-1) dashed\nspline from (0,-2) to (0.5,-1.6) then to (1,-2) ->\n\
               arc from (2,-1) to (2.5,-0.5)\ndot at (3,0)\n\"standalone\" at (0,-3)\nbrace from (3,-1) to (3,-2)\n\
               [ box at (4,0) ]\nmove right 0.2\nbox gradient \"red\" \"blue\" at (5,0) link \"https://rpic.dev\"";
    let g = georeference(&scene_of(src), &GeoOptions::default()).unwrap();
    let kinds: std::collections::BTreeSet<&str> = g
        .features
        .features
        .iter()
        .map(|f| f.properties["geopic:kind"].as_str().unwrap())
        .collect();
    for k in [
        "box",
        "circle",
        "ellipse",
        "arrow",
        "arrowhead",
        "spline",
        "arc",
        "dot",
        "text",
        "brace",
    ] {
        assert!(kinds.contains(k), "no feature of kind {k}: {kinds:?}");
    }
    // never silent: the gradient warned, nothing else is "unsupported_kind"
    assert!(
        g.warnings.iter().any(|w| w.kind == "unsupported_style"),
        "{:?}",
        g.warnings
    );
    assert!(
        g.warnings.iter().all(|w| w.kind != "unsupported_kind"),
        "{:?}",
        g.warnings
    );
    // every feature carries the schema, and the block child names its block
    for f in &g.features.features {
        assert_eq!(f.properties["geopic:schema"], 1);
        assert_eq!(
            f.properties["geopic:placement"]
                .as_str()
                .map(|s| s == "screen"),
            Some(matches!(
                f.properties["geopic:kind"].as_str(),
                Some("text" | "dot")
            ))
        );
    }
    assert!(
        g.features
            .features
            .iter()
            .any(|f| f.properties.get("geopic:block").is_some())
    );
    // simplestyle
    let hub = g
        .features
        .features
        .iter()
        .find(|f| f.properties["geopic:label"] == "B")
        .unwrap();
    assert_eq!(hub.properties["title"], "hub");
    assert_eq!(hub.properties["fill"], "#4d4d4d");
    assert_eq!(hub.properties["stroke"], "#000000");
    let linked = g
        .features
        .features
        .iter()
        .find(|f| f.properties.get("geopic:link").is_some())
        .unwrap();
    assert_eq!(linked.properties["geopic:link"], "https://rpic.dev");
    assert_eq!(linked.properties["fill"], "#ff0000"); // first gradient stop
    // valid GeoJSON text
    let v: Value = serde_json::from_str(&g.features.to_json()).unwrap();
    assert_eq!(v["type"], "FeatureCollection");
    assert_eq!(v["features"][0]["type"], "Feature");
    assert_eq!(v["features"][0]["geometry"]["type"], "Polygon");
}

#[test]
fn scene_parse_errors_and_invalid_anchors_are_structured() {
    assert_eq!(
        crate::parse_scene("nope").unwrap_err().kind,
        "invalid_scene"
    );
    let e = georeference(
        &scene_of("geo_lon = 0\ngeo_lat = 90\nbox"),
        &GeoOptions::default(),
    )
    .unwrap_err();
    assert_eq!(e.kind, "invalid_anchor");
    let e = georeference(
        &scene_of("geo_lon = 190\ngeo_lat = 0\nbox"),
        &GeoOptions::default(),
    )
    .unwrap_err();
    assert_eq!(e.kind, "invalid_anchor");
    let e = georeference(
        &scene_of("geo_lon = 0\ngeo_lat = 0\ngeo_scale = 0\nbox"),
        &GeoOptions::default(),
    )
    .unwrap_err();
    assert_eq!(e.kind, "invalid_scale");
    let e = georeference(
        &scene_of("geo_lon = 0\ngeo_lat = 0\ngeo_scale = -5\nbox"),
        &GeoOptions::default(),
    )
    .unwrap_err();
    assert_eq!(e.kind, "invalid_scale");
    let e = georeference(
        &scene_of("geo_lon = 0\ngeo_lat = 0\ngeo_px = 1\nbox"),
        &GeoOptions::default(),
    )
    .unwrap_err();
    assert_eq!(e.kind, "view_scale_required");
    // antimeridian: a drawing straddling ±180 yields normalized longitudes
    let g = georeference(
        &scene_of("geo_lon = 179.999\ngeo_lat = 0\ngeo_scale = 1000\nline right 1"),
        &GeoOptions::default(),
    )
    .unwrap();
    let Geometry::LineString(l) = &g.features.features[0].geometry else {
        panic!()
    };
    assert!(l.iter().all(|p| p[0].abs() <= 180.0));
    assert!(
        l.last().unwrap()[0] < 0.0,
        "crossed the antimeridian: {:?}",
        l.last()
    );
}

// ---- A6: every feature's properties validate against the published schema --

#[test]
fn a6_feature_properties_validate_against_the_schema() {
    let schema_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/geopic.schema.json");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    let validator = jsonschema::validator_for(&schema).expect("valid schema");
    let src = "geo_lon = -34.88\ngeo_lat = -8.05\nB: box \"hub\" fill 0.3 class \"screen x\"\ncircle rad 0.2 at B.e + (0.5,0) dotted\n\
               ellipse at (2,0) opacity 0.5\narrow from (0,-1) to (1,-1) dashed <->\nspline from (0,-2) to (0.5,-1.6) then to (1,-2) ->\n\
               arc from (2,-1) to (2.5,-0.5) ->\ndot at (3,0)\n\"standalone\" rotated 30 at (0,-3)\nbrace from (3,-1) to (3,-2)\n\
               [ box at (4,0) hatched ]\nbox gradient \"red\" \"blue\" at (5,0) link \"https://rpic.dev\"\nline from (6,0) to (6.5,0) then to (6.5,0.5) close";
    let g = georeference(&scene_of(src), &GeoOptions::default()).unwrap();
    let v: Value = serde_json::from_str(&g.features.to_json()).unwrap();
    let feats = v["features"].as_array().unwrap();
    assert!(feats.len() >= 12, "{}", feats.len());
    for f in feats {
        if let Err(e) = validator.validate(&f["properties"]) {
            panic!(
                "feature {} fails the schema: {e}\n{}",
                f["properties"]["geopic:id"], f["properties"]
            );
        }
        let g = &f["geometry"];
        assert!(
            matches!(g["type"].as_str(), Some("Point" | "LineString" | "Polygon")),
            "{g}"
        );
        if g["type"] == "Polygon" {
            let ring = g["coordinates"][0].as_array().unwrap();
            assert!(
                ring.len() >= 4 && ring[0] == ring[ring.len() - 1],
                "ring not closed: {g}"
            );
        }
    }
    // the `screen` class word marks placement, the closed line is a polygon
    let hub = feats
        .iter()
        .find(|f| f["properties"]["geopic:label"] == "B")
        .unwrap();
    assert_eq!(hub["properties"]["geopic:placement"], "screen");
    let closed = feats
        .iter()
        .rev()
        .find(|f| f["properties"]["geopic:kind"] == "line")
        .unwrap();
    assert_eq!(closed["geometry"]["type"], "Polygon");
}

// ---- hostile input: bounded output ------------------------------------------

#[test]
fn hostile_scales_are_rejected_and_densification_is_budgeted() {
    // absurd scale → structured error, not a 10^12-vertex allocation
    let e = georeference(
        &scene_of("geo_lon = 0\ngeo_lat = 0\ngeo_scale = 1e12\nline right 1"),
        &GeoOptions::default(),
    )
    .unwrap_err();
    assert_eq!(e.kind, "invalid_scale");
    // a drawing whose segments add up to far more than the budget allows
    // (each 1 in = 1e8 m → 1e6 vertices wanted per segment, capped at 1e5)
    let mut src = String::from("geo_lon = 0\ngeo_lat = 0\ngeo_scale = 1e8\n");
    for i in 0..30 {
        src.push_str(&format!("line from ({i}, 0) to ({i}, 1)\n"));
    }
    let g = georeference(&scene_of(&src), &GeoOptions::default()).unwrap();
    let total: usize = g
        .features
        .features
        .iter()
        .map(|f| match &f.geometry {
            Geometry::LineString(p) => p.len(),
            Geometry::Polygon(r) => r.iter().map(Vec::len).sum(),
            Geometry::Point(_) => 1,
        })
        .sum();
    assert!(
        total <= crate::densify::MAX_VERTICES + 2 * 30,
        "{total} vertices"
    );
    assert!(
        g.warnings.iter().any(|w| w.kind == "densification_budget"),
        "{:?}",
        g.warnings
    );
    // every feature still has both endpoints
    assert_eq!(g.features.features.len(), 30);
}

// ---- review follow-ups ----------------------------------------------------------

#[test]
fn text_rotation_is_ccw_from_east_and_open_fills_become_polygons() {
    // bearing 90: canvas +x points south = −90° CCW from east; a label
    // rotated 30 CCW on the canvas is therefore at −60 on the map
    let g = georeference(
        &scene_of("geo_lon = 0\ngeo_lat = 0\ngeo_bearing = 90\n\"t\" rotated 30 at (1, 0)"),
        &GeoOptions::default(),
    )
    .unwrap();
    let txt = g
        .features
        .features
        .iter()
        .find(|f| f.properties["geopic:kind"] == "text")
        .unwrap();
    assert_eq!(txt.properties["geopic:rotation"], -60.0);
    // an open arc with a fill paints its implicitly closed area: a Polygon
    // with role "fill" rides next to the LineString, which keeps the stroke
    let g = georeference(
        &scene_of("geo_lon = 0\ngeo_lat = 0\narc from (0,0) to (0.5,0.5) fill 0.3 ->"),
        &GeoOptions::default(),
    )
    .unwrap();
    let arcs: Vec<_> = g
        .features
        .features
        .iter()
        .filter(|f| f.properties["geopic:kind"] == "arc")
        .collect();
    assert_eq!(
        arcs.len(),
        2,
        "{:?}",
        arcs.iter().map(|f| &f.properties).collect::<Vec<_>>()
    );
    let fill = arcs
        .iter()
        .find(|f| f.properties.get("geopic:role") == Some(&serde_json::json!("fill")))
        .unwrap();
    assert!(matches!(fill.geometry, Geometry::Polygon(_)));
    assert_eq!(fill.properties["fill"], "#4d4d4d");
    assert!(fill.properties.get("stroke").is_none());
    let stroke = arcs
        .iter()
        .find(|f| f.properties.get("geopic:role").is_none())
        .unwrap();
    assert!(matches!(stroke.geometry, Geometry::LineString(_)));
    assert!(stroke.properties.get("fill").is_none());
    assert_eq!(stroke.properties["stroke"], "#000000");
}

#[test]
fn labels_resolve_to_the_latest_top_level_definition() {
    // the redefined A is the live one (pic semantics); the block-local B is
    // not the control point when a top-level B exists
    let src = "A: box at (0,0)\nA: box at (5,5)\nA_lon = 10\nA_lat = 20\n\
               [ B: circle at (9,9) ]\nB: circle at (1,1)\nB_lon = 10.01\nB_lat = 20";
    let g = georeference(&scene_of(src), &GeoOptions::default()).unwrap();
    let r = &g.registration;
    // A at canvas (5,5) and B at (1,1): both control points land where declared
    let a = r.to_lonlat([5.0, 5.0]);
    assert!(
        geodesy::geodesic_distance(
            a,
            LonLat {
                lon: 10.0,
                lat: 20.0
            }
        ) < 1e-3
    );
    let b = r.to_lonlat([1.0, 1.0]);
    assert!(
        geodesy::geodesic_distance(
            b,
            LonLat {
                lon: 10.01,
                lat: 20.0
            }
        ) < 1e-3
    );
}

#[test]
fn host_features_override_source_control_points_and_scale_checks_wait_for_the_fit() {
    let src = "P: (0,0)\nQ: (2,0)\nP_lon = 0\nP_lat = 0\nQ_lon = 0.01\nQ_lat = 0\ngeo_px = 1\ngeo_scale = 1e12\nbox";
    let o = GeoOptions {
        features: vec![crate::NamedFeature {
            name: "Q".into(),
            lon: 0.02,
            lat: 0.0,
            bbox: None,
        }],
        ..GeoOptions::default()
    };
    // geo_px without view_scale and an absurd geo_scale are both irrelevant
    // once two control points fix the scale; the host's Q wins
    let g = georeference(&scene_of(src), &o).unwrap();
    assert_eq!(g.registration.method, Method::TwoPoints);
    let q = g.registration.to_lonlat([2.0, 0.0]);
    assert!(
        geodesy::geodesic_distance(
            q,
            LonLat {
                lon: 0.02,
                lat: 0.0
            }
        ) < 1e-3,
        "{q:?}"
    );
    assert!(
        g.warnings
            .iter()
            .any(|w| w.kind == "control_point_overridden"),
        "{:?}",
        g.warnings
    );
    assert!(
        g.warnings
            .iter()
            .any(|w| w.kind == "ignored_with_control_points")
    );
    // a fitted scale beyond the cap is still rejected
    let far = "P: (0,0)\nQ: (0.000001,0)\nP_lon = 0\nP_lat = 0\nQ_lon = 90\nQ_lat = 0\nbox";
    assert_eq!(
        georeference(&scene_of(far), &GeoOptions::default())
            .unwrap_err()
            .kind,
        "invalid_scale"
    );
    // flatten is validated
    let o = GeoOptions {
        flatten: 0.0,
        ..GeoOptions::default()
    };
    assert_eq!(
        georeference(&scene_of("geo_lon = 0\ngeo_lat = 0\nbox"), &o)
            .unwrap_err()
            .kind,
        "invalid_flatten"
    );
}

#[test]
fn large_circles_stay_within_the_flatten_tolerance() {
    // r = 1 in = 100 km: the fixed 128-vertex ring would be 30 m off; the
    // tolerance-driven one keeps the sagitta under 1e-3 in = 0.1 km... and
    // the 100 m chord rule caps the vertex count at 4096
    let g = georeference(
        &scene_of("geo_lon = 0\ngeo_lat = 0\ngeo_scale = 100000\ncircle rad 1 at (0,0)"),
        &GeoOptions::default(),
    )
    .unwrap();
    let Geometry::Polygon(rings) = &g.features.features[0].geometry else {
        panic!()
    };
    // 4096 chords of ~153 m, each split once by the 100 m rule
    let n = rings[0].len() - 1;
    assert!(
        (crate::densify::MAX_RING_VERTICES..=2 * crate::densify::MAX_RING_VERTICES).contains(&n),
        "{n} vertices"
    );
    // every vertex is 100 km from the centre (the circle is a true circle in
    // the tangent plane); the worst mid-chord sagitta is r(1−cos(π/n)) ≈ 3 cm
    for p in rings[0].iter().step_by(97) {
        let d = geodesy::geodesic_distance(
            LonLat { lon: 0.0, lat: 0.0 },
            LonLat {
                lon: p[0],
                lat: p[1],
            },
        );
        assert!((d - 100_000.0).abs() < 0.5, "{d}");
    }
}
