//! Scene acceptance over the whole corpus (docs/scene-acceptance.md):
//!
//! - A1 coverage: `--scene` succeeds on every example; the drawable item
//!   count equals the SVG's count of non-empty `s<N>` groups.
//! - A3 ids & order: scene array order == SVG document order of the groups,
//!   ids match one-to-one, `z` is dense and increasing.
//! - A4 resolved style: every scene color is `#rrggbb` (or the compile warns
//!   `scene_unsupported` for that figure).
//! - B1 schema: every scene validates against docs/scene.schema.json.
//! - B5: the texlabels gap warns (metrics-only math) instead of silence.
//!
//! Corpus convention matches the drift guard: `-c` on, `-t` off.

use std::path::{Path, PathBuf};
use std::process::Command;

fn collect_pics(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_pics(&path, out);
        } else if path.extension().is_some_and(|e| e == "pic") {
            out.push(path);
        }
    }
}

fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut pics = Vec::new();
    collect_pics(&root.join("examples"), &mut pics);
    collect_pics(&root.join("assets"), &mut pics);
    pics.sort();
    assert!(pics.len() > 100, "corpus went missing? {}", pics.len());
    pics
}

fn run_rpic(args: &[&str], pic: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rpic"))
        .args(args)
        .arg(pic)
        .output()
        .expect("run rpic")
}

/// The ids of `<g id="sN">` groups in SVG document order, with whether the
/// group (or its trailing `<a>` wrapper) contains any graphical content.
fn svg_groups(svg: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find("<g id=\"s") {
        let after = &rest[i + 7..];
        let id_end = after.find('"').unwrap();
        let id = after[..id_end].to_string();
        let body_start = after.find('>').unwrap() + 1;
        let body_end = after.find("</g>").unwrap();
        let body = &after[body_start..body_end];
        out.push((id, paints(body)));
        rest = &after[body_end..];
    }
    out
}

/// Whether a group's body paints anything: it contains an element that is not
/// an empty `<text …></text>` (classic pic emits those for blank label lines;
/// they draw no ink and the scene rightly hides them).
fn paints(body: &str) -> bool {
    let mut rest = body;
    while let Some(i) = rest.find('<') {
        let after = &rest[i..];
        if let Some(r) = after.strip_prefix("</text>") {
            rest = r;
            continue;
        }
        if after.starts_with("<text") {
            let gt = after.find('>').unwrap();
            if after[gt + 1..].starts_with("</text>") {
                rest = &after[gt + 1..];
                continue; // empty text element: no ink
            }
        }
        return true;
    }
    false
}

#[test]
fn corpus_scene_coverage_ids_order_and_schema() {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/scene.schema.json");
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    let validator = jsonschema::validator_for(&schema).expect("valid schema");

    let mut checked = 0;
    for pic in corpus() {
        let scene_out = run_rpic(&["-c", "--scene"], &pic);
        assert!(
            scene_out.status.success(),
            "{}: --scene failed: {}",
            pic.display(),
            String::from_utf8_lossy(&scene_out.stderr)
        );
        let scene: serde_json::Value = serde_json::from_slice(&scene_out.stdout)
            .unwrap_or_else(|e| panic!("{}: scene JSON does not parse: {e}", pic.display()));

        // B1: schema
        if let Err(err) = validator.validate(&scene) {
            panic!("{}: scene fails the schema: {err}", pic.display());
        }

        let svg_out = run_rpic(&["-c", "--svg"], &pic);
        assert!(svg_out.status.success(), "{}: --svg failed", pic.display());
        let svg = String::from_utf8_lossy(&svg_out.stdout).to_string();
        let groups = svg_groups(&svg);

        let items = scene["items"].as_array().unwrap();
        let shape_items: Vec<&serde_json::Value> =
            items.iter().filter(|i| i["kind"] != "block").collect();

        // A3: one item per group, same ids, same (document) order, dense z
        assert_eq!(
            shape_items.len(),
            groups.len(),
            "{}: {} scene items vs {} svg groups",
            pic.display(),
            shape_items.len(),
            groups.len()
        );
        for (k, (item, (gid, has_content))) in shape_items.iter().zip(groups.iter()).enumerate() {
            assert_eq!(
                item["id"].as_str().unwrap(),
                gid,
                "{}: order/id mismatch at draw position {k}",
                pic.display()
            );
            assert_eq!(
                item["z"].as_u64().unwrap() as usize,
                k,
                "{}: z not dense at {k}",
                pic.display()
            );
            // A1: drawability agreement — a non-hidden item paints, a hidden
            // one leaves its group empty
            let hidden = item.get("hidden").is_some();
            assert_eq!(
                !hidden,
                *has_content,
                "{}: item {gid} hidden={hidden} but svg group has_content={has_content}",
                pic.display()
            );
        }

        // A4: every color the scene carries is #rrggbb, unless the figure
        // legitimately warned scene_unsupported (pass-through values)
        let warned = {
            let all = run_rpic(&["-c", "--json=all"], &pic);
            String::from_utf8_lossy(&all.stdout).contains("scene_unsupported")
        };
        if !warned {
            let mut colors = Vec::new();
            collect_colors(&scene, &mut colors);
            for c in colors {
                assert!(
                    c.len() >= 7
                        && c.starts_with('#')
                        && c[1..].chars().all(|ch| ch.is_ascii_hexdigit()),
                    "{}: non-hex color `{c}` without a warning",
                    pic.display()
                );
            }
        }
        checked += 1;
    }
    assert!(checked > 100, "checked only {checked}");
}

fn collect_colors(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, v) in m {
                if k == "color" {
                    if let Some(s) = v.as_str() {
                        out.push(s.to_string());
                    }
                } else if k != "vars" {
                    collect_colors(v, out);
                }
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                collect_colors(v, out);
            }
        }
        _ => {}
    }
}

#[test]
fn corpus_scene_flatten_succeeds_everywhere() {
    for pic in corpus() {
        let out = run_rpic(&["-c", "--scene", "--flatten", "0.001"], &pic);
        assert!(
            out.status.success(),
            "{}: --scene --flatten failed: {}",
            pic.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        let scene: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        // flattened scenes carry no cubics at all
        let mut found = Vec::new();
        collect_keys(&scene, &mut found);
        assert!(
            !found.iter().any(|k| k == "cubics"),
            "{}: cubics survived --flatten",
            pic.display()
        );
    }
}

fn collect_keys(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, v) in m {
                out.push(k.clone());
                if k != "vars" {
                    collect_keys(v, out);
                }
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                collect_keys(v, out);
            }
        }
        _ => {}
    }
}

#[test]
fn texlabels_math_without_outline_warns_scene_unsupported() {
    // B5: the documented scene gap — math glyphs absent unless --outline
    let dir = std::env::temp_dir().join(format!("rpic_scene_tex_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pic = dir.join("m.pic");
    std::fs::write(&pic, "box \"$x^2$\"\n").unwrap();

    let out = run_rpic(&["-t", "--json=all"], &pic);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warnings = v["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w["kind"] == "scene_unsupported" && w["line"] == 1),
        "expected scene_unsupported with span: {warnings:?}"
    );
    // metrics ride the line; glyphs don't
    let line = &v["scene"]["items"][0]["text"][0];
    assert!(line["math"]["width"].as_f64().unwrap() > 0.0);
    assert!(line.get("outline").is_none());

    // with --outline the fragment is embedded and the warning is gone
    let out = run_rpic(&["-t", "--outline", "--json=all"], &pic);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !v["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["kind"] == "scene_unsupported"),
    );
    let line = &v["scene"]["items"][0]["text"][0];
    assert!(
        line["outline"].as_str().unwrap().starts_with("<svg"),
        "outline should embed the typeset fragment"
    );

    // the plain bundle stays scene-free and warning-inert
    let plain = run_rpic(&["-t", "--json"], &pic);
    let s = String::from_utf8_lossy(&plain.stdout).to_string();
    assert!(!s.contains("\"scene\""));
    assert!(!s.contains("scene_unsupported"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scene_source_names_the_input_file() {
    let dir = std::env::temp_dir().join(format!("rpic_scene_src_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pic = dir.join("named.pic");
    std::fs::write(&pic, "box\n").unwrap();
    let out = run_rpic(&["--scene"], &pic);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["source"], "named.pic");
    let _ = std::fs::remove_dir_all(&dir);
}
