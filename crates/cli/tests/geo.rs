//! `rpic --geojson` drift guard: every `examples/geo/*.pic` must produce its
//! committed sibling `.geojson` — the same features, properties and vertex
//! counts, every coordinate within 1e-8° (≈ 1 mm) — and every feature's
//! properties must validate against `docs/geopic.schema.json` (acceptance A6
//! of the geo prompt).
//!
//! The comparison is numeric, not byte-exact, on purpose: the platform libm
//! (sin/cos/atan2) differs in its last bits between macOS, Linux and Windows,
//! and the Helmert fit amplifies that into the 9th decimal of a few
//! coordinates. Millimetres are the reproducibility this guard promises.
//!
//! On a mismatch: regenerate the sibling (`rpic -c --geojson <file>.pic >
//! <file>.geojson`), look at the figure (`rpic -c --png`) and at the
//! coordinates before committing — a drift here is a change in how drawings
//! land on the Earth.

use std::path::{Path, PathBuf};
use std::process::Command;

fn examples() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/geo");
    let mut pics: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("examples/geo exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "pic"))
        .collect();
    pics.sort();
    assert!(
        pics.len() >= 4,
        "expected the four geo examples, found {}",
        pics.len()
    );
    pics
}

fn rpic(args: &[&str], pic: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rpic"))
        .args(args)
        .arg(pic.file_name().unwrap())
        .current_dir(pic.parent().unwrap())
        .output()
        .expect("run rpic")
}

fn lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Coordinates within this many degrees count as equal (≈ 1 mm).
const COORD_TOL: f64 = 1e-8;
/// Numeric properties (`geopic:rotation` carries the fitted bearing) within this.
const PROP_TOL: f64 = 1e-6;

fn close(a: &serde_json::Value, b: &serde_json::Value, tol: f64, what: &str) {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert!((x - y).abs() <= tol, "{what}: {x} vs {y} (tolerance {tol})");
        }
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{what}: array length");
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                close(p, q, tol, &format!("{what}[{i}]"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            let kx: Vec<&String> = x.keys().collect();
            let ky: Vec<&String> = y.keys().collect();
            assert_eq!(kx, ky, "{what}: keys");
            for (k, p) in x {
                close(p, &y[k], tol, &format!("{what}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{what}"),
    }
}

/// Same features in the same order, equal properties (numbers to `PROP_TOL`),
/// same geometry types and vertex counts, coordinates to `COORD_TOL`.
fn assert_geojson_close(got: &str, want: &str, pic: &Path) {
    let g: serde_json::Value = serde_json::from_str(got).unwrap();
    let w: serde_json::Value = serde_json::from_str(want).unwrap();
    let (gf, wf) = (
        g["features"].as_array().unwrap(),
        w["features"].as_array().unwrap(),
    );
    assert_eq!(
        gf.len(),
        wf.len(),
        "{}: --geojson output drifted from the committed .geojson (feature count) — regenerate and review",
        pic.display()
    );
    for (i, (a, b)) in gf.iter().zip(wf).enumerate() {
        let what = format!(
            "{}: feature {i} ({})",
            pic.display(),
            a["properties"]["geopic:id"]
        );
        close(
            &a["properties"],
            &b["properties"],
            PROP_TOL,
            &format!("{what} properties"),
        );
        assert_eq!(
            a["geometry"]["type"], b["geometry"]["type"],
            "{what} geometry type"
        );
        close(
            &a["geometry"]["coordinates"],
            &b["geometry"]["coordinates"],
            COORD_TOL,
            &format!("{what} coordinates"),
        );
    }
}

#[test]
fn committed_geojson_matches_current_output_and_validates() {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/geopic.schema.json");
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    let validator = jsonschema::validator_for(&schema).expect("valid schema");

    for pic in examples() {
        let out = rpic(&["-c", "--geojson"], &pic);
        assert!(
            out.status.success(),
            "{}: --geojson failed: {}",
            pic.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        let got = lf(&String::from_utf8_lossy(&out.stdout));
        let committed = lf(&std::fs::read_to_string(pic.with_extension("geojson"))
            .unwrap_or_else(|_| panic!("{}: no committed .geojson sibling", pic.display())));
        assert_geojson_close(&got, &committed, &pic);
        // no warnings on the curated examples
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("warning:"),
            "{}: unexpected warning: {stderr}",
            pic.display()
        );
        // valid GeoJSON, valid properties
        let v: serde_json::Value = serde_json::from_str(&got).unwrap();
        assert_eq!(v["type"], "FeatureCollection");
        let feats = v["features"].as_array().unwrap();
        assert!(!feats.is_empty());
        for f in feats {
            assert_eq!(f["type"], "Feature");
            assert_eq!(f["properties"]["geopic:schema"], 1);
            if let Err(e) = validator.validate(&f["properties"]) {
                panic!(
                    "{}: {} fails the schema: {e}",
                    pic.display(),
                    f["properties"]["geopic:id"]
                );
            }
            let coords = &f["geometry"]["coordinates"];
            let check = |p: &serde_json::Value| {
                let (lon, lat) = (p[0].as_f64().unwrap(), p[1].as_f64().unwrap());
                assert!(
                    (-180.0..=180.0).contains(&lon) && (-90.0..=90.0).contains(&lat),
                    "{p}"
                );
            };
            match f["geometry"]["type"].as_str().unwrap() {
                "Point" => check(coords),
                "LineString" => coords.as_array().unwrap().iter().for_each(check),
                "Polygon" => {
                    let ring = coords[0].as_array().unwrap();
                    assert!(ring.len() >= 4 && ring[0] == ring[ring.len() - 1]);
                    ring.iter().for_each(check);
                }
                other => panic!("unexpected geometry {other}"),
            }
        }
        // #435: every shape with a title is followed by exactly one title
        // Point naming it, and only shapes with a title have one
        for (i, f) in feats.iter().enumerate() {
            let p = &f["properties"];
            if p["geopic:kind"] == "title" {
                assert_eq!(f["geometry"]["type"], "Point", "{}", pic.display());
                let owner = feats
                    .iter()
                    .map(|f| &f["properties"])
                    .find(|o| o["geopic:id"] == p["geopic:for"] && o["geopic:kind"] != "arrowhead")
                    .unwrap_or_else(|| panic!("{}: title without its shape", pic.display()));
                assert_eq!(p["geopic:text"], owner["title"], "{}", pic.display());
                // right after the owner's own features, arrowheads included
                let own: Vec<usize> = feats
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o["properties"]["geopic:id"] == p["geopic:for"])
                    .map(|(j, _)| j)
                    .collect();
                assert_eq!(own.last().map(|j| j + 1), Some(i), "{}", pic.display());
                assert!(
                    own.windows(2).all(|w| w[1] == w[0] + 1),
                    "{}",
                    pic.display()
                );
            }
            if p["geopic:kind"] == "arrowhead" {
                for k in ["title", "geopic:text", "geopic:font_size"] {
                    assert!(
                        p.get(k).is_none(),
                        "{}: arrowhead carries {k}",
                        pic.display()
                    );
                }
            }
        }
        let shapes_with_titles: std::collections::BTreeSet<&str> = feats
            .iter()
            .map(|f| &f["properties"])
            .filter(|p| {
                !matches!(
                    p["geopic:kind"].as_str(),
                    Some("text" | "title" | "arrowhead")
                )
            })
            .filter(|p| p.get("title").is_some())
            .map(|p| p["geopic:id"].as_str().unwrap())
            .collect();
        let titled: std::collections::BTreeSet<&str> = feats
            .iter()
            .map(|f| &f["properties"])
            .filter(|p| p["geopic:kind"] == "title")
            .map(|p| p["geopic:for"].as_str().unwrap())
            .collect();
        assert_eq!(titled, shapes_with_titles, "{}", pic.display());
        assert_eq!(
            titled.len(),
            feats
                .iter()
                .filter(|f| f["properties"]["geopic:kind"] == "title")
                .count(),
            "{}: one title Point per shape",
            pic.display()
        );
    }
}

#[test]
fn geo_options_override_the_source_and_features_act_as_control_points() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/geo");
    // the callout declares its anchor; --geo-anchor moves the whole drawing
    let pic = dir.join("callout.pic");
    let a = rpic(&["--geojson"], &pic);
    let b = rpic(
        &[
            "--geojson",
            "--geo-anchor",
            "-46.6333,-23.5505",
            "--geo-frame",
            "mercator",
        ],
        &pic,
    );
    assert!(
        a.status.success() && b.status.success(),
        "{}",
        String::from_utf8_lossy(&b.stderr)
    );
    let va: serde_json::Value = serde_json::from_slice(&a.stdout).unwrap();
    let vb: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    let dot = |v: &serde_json::Value| v["features"][0]["geometry"]["coordinates"].clone();
    assert_eq!(dot(&va)[0], -34.8711);
    assert_eq!(dot(&vb)[0], -46.6333);
    assert_eq!(dot(&vb)[1], -23.5505);
    // a source without any registration fails with the structured kind …
    let tmp = TempPic::new("P: (0,0)\nQ: (1,0)\nline from P to Q\n");
    let tmp = &tmp.path;
    let none = rpic(&["--geojson"], tmp);
    assert!(!none.status.success());
    assert!(String::from_utf8_lossy(&none.stderr).contains("no_registration"));
    // … and host features name its places as control points
    let feat = rpic(
        &[
            "--geojson",
            "--geo-feature",
            "P=0,0",
            "--geo-feature",
            "Q=0.01,0",
        ],
        tmp,
    );
    assert!(
        feat.status.success(),
        "{}",
        String::from_utf8_lossy(&feat.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&feat.stdout).unwrap();
    let line = v["features"][0]["geometry"]["coordinates"]
        .as_array()
        .unwrap();
    assert_eq!(line[0][0], 0.0);
    assert!((line[line.len() - 1][0].as_f64().unwrap() - 0.01).abs() < 1e-9);
    // bad or out-of-range option values are rejected up front, naming the flag
    for args in [
        ["--geojson", "--geo-anchor", "x"],
        ["--geojson", "--geo-anchor", "200,0"],
        ["--geojson", "--geo-anchor", "nan,0"],
        ["--geojson", "--geo-feature", "=1,2"],
        ["--geojson", "--geo-feature", "A=1,95"],
    ] {
        let bad = rpic(&args, &pic);
        assert!(!bad.status.success(), "{args:?}");
        let err = String::from_utf8_lossy(&bad.stderr);
        assert!(err.contains(args[1]), "{args:?}: {err}");
        assert!(
            !err.contains("callout.pic"),
            "{args:?}: error blamed the input file: {err}"
        );
    }
    // an unresolved feature and an unused view scale are warned, not swallowed
    let w = rpic(
        &[
            "--geojson",
            "--geo-feature",
            "Typo=1,2",
            "--geo-view-scale",
            "96",
        ],
        &pic,
    );
    assert!(w.status.success());
    let err = String::from_utf8_lossy(&w.stderr);
    assert!(
        err.contains("control_point_unresolved") && err.contains("Typo"),
        "{err}"
    );
    assert!(
        err.contains("(ignored)") && err.contains("view_scale"),
        "{err}"
    );
}

/// A pic source in the temp dir, removed on drop (also on a failed assertion).
struct TempPic {
    path: PathBuf,
}

impl TempPic {
    fn new(src: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "rpic_geo_{}_{}.pic",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&path, src).unwrap();
        Self { path }
    }
}

impl Drop for TempPic {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn animations_are_warned_on_the_cli_too() {
    let tmp = TempPic::new("geo_lon = 0\ngeo_lat = 0\nbox\nanimate last box with \"fade\"\n");
    let out = rpic(&["--geojson"], &tmp.path);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("animation") && err.contains("(ignored)"),
        "{err}"
    );
}

#[test]
fn flatten_reaches_the_georeferencer() {
    // `--flatten` also sets the geo tolerance: a tiny circle (1 m, so the
    // 100 m chord rule is moot) gets its ring vertex count from it — the
    // minimum 32 when coarse, a few hundred when fine. (Splines cannot show
    // this: the scene already polylines them under `--flatten`.)
    let tmp = TempPic::new("geo_lon = 0\ngeo_lat = 0\ngeo_scale = 1\ncircle rad 1\n");
    let count = |tol: &str| {
        let out = rpic(&["--geojson", "--flatten", tol], &tmp.path);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["features"][0]["geometry"]["coordinates"][0]
            .as_array()
            .unwrap()
            .len()
    };
    let (coarse, fine) = (count("0.5"), count("0.0001"));
    assert_eq!(coarse, 33, "coarse ring = 32 vertices + closing point");
    assert!(fine > 200, "fine ring has {fine} vertices");
}

/// `--geojson --maplibre`: the same FeatureCollection plus the MapLibre
/// layers, which match the snapshots in `examples/geo/maplibre/` — the
/// layer specs the rpic.dev Maps page drew before the styling moved into
/// geopic-core (DECISIONS G9). Plain `--geojson` stays a bare collection.
#[test]
fn maplibre_bundle_carries_the_collection_and_the_snapshot_layers() {
    let snapshots = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/geo/maplibre");
    for pic in examples() {
        let plain = rpic(&["-c", "--geojson"], &pic);
        let out = rpic(&["-c", "--geojson", "--maplibre"], &pic);
        assert!(
            out.status.success(),
            "{}: {}",
            pic.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["geojson", "layers"], "{}", pic.display());
        // the collection is the plain --geojson output, byte for byte
        assert_eq!(
            v["geojson"],
            serde_json::from_slice::<serde_json::Value>(&plain.stdout).unwrap(),
            "{}",
            pic.display()
        );
        let stem = pic.file_stem().unwrap().to_string_lossy();
        let want: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(snapshots.join(format!("{stem}.layers.json"))).unwrap(),
        )
        .unwrap();
        // px-per-metre comes from cos(lat): last-bit libm noise only
        close(&v["layers"], &want, 1e-9, &format!("{stem} layers"));
    }
    // `--maplibre` alone is an error, not a silent no-op
    let pic = examples().remove(0);
    let bad = rpic(&["--maplibre"], &pic);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("only applies to `--geojson`"));
}
