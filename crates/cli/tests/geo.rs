//! `rpic --geojson` drift guard: every `examples/geo/*.pic` must produce
//! exactly its committed sibling `.geojson` (compact, as the CLI prints it;
//! CRLF-normalized), and every feature's properties must validate against
//! `docs/geopic.schema.json` (acceptance A6 of the geo prompt).
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
        assert_eq!(
            got.trim_end(),
            committed.trim_end(),
            "{}: --geojson output drifted from the committed .geojson — regenerate and review",
            pic.display()
        );
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
    let tmp = std::env::temp_dir().join(format!("rpic_geo_{}.pic", std::process::id()));
    std::fs::write(&tmp, "P: (0,0)\nQ: (1,0)\nline from P to Q\n").unwrap();
    let none = rpic(&["--geojson"], &tmp);
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
        &tmp,
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
    // bad option values are rejected up front
    let bad = rpic(&["--geojson", "--geo-anchor", "x"], &pic);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("--geo-anchor"));
    let _ = std::fs::remove_file(&tmp);
}
