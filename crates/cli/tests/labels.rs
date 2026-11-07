//! Labels on flat macro calls (#425 item 3, DECISIONS E3) through the real
//! binary: `R1: resistor(A,B)` with the `-c` library names the element's
//! group — the scene gets a labelled block with the call's provenance, the
//! label resolves (`R1.n`) into `--json` places, and `--svg` is
//! byte-identical to the unlabelled call.

use std::process::Command;

fn run(name: &str, src: &str, mode: &str) -> String {
    // per process: two checkouts running `cargo test` must not race
    let dir = std::env::temp_dir().join(format!("rpic-label-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mk temp dir");
    let pic = dir.join(format!("{name}.pic"));
    std::fs::write(&pic, src).expect("write pic");
    let out = Command::new(env!("CARGO_BIN_EXE_rpic"))
        .args(["-c", mode])
        .arg(&pic)
        .output()
        .expect("run rpic");
    assert!(
        out.status.success(),
        "{mode} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8")
}

const LABELLED: &str = ".PS\nA: (0,0); B: (2,0)\nR1: resistor(A,B)\nP: R1.n\nQ: R1.end\n.PE\n";
const PLAIN: &str = ".PS\nA: (0,0); B: (2,0)\nresistor(A,B)\n.PE\n";

#[test]
fn labelled_element_svg_is_unchanged() {
    assert_eq!(
        run("labelled-svg", LABELLED, "--svg"),
        run("plain-svg", PLAIN, "--svg")
    );
}

#[test]
fn labelled_element_resolves_in_json_places() {
    let j: serde_json::Value =
        serde_json::from_str(&run("labelled-json", LABELLED, "--json")).unwrap();
    let place = |n: &str| {
        j["places"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["label"] == n)
            .unwrap_or_else(|| panic!("no place {n}: {j}"))
            .clone()
    };
    // places are in SVG user units; R1.end is B, R1.n sits above the middle
    let (a, b, p) = (place("A"), place("B"), place("P"));
    assert_eq!(place("Q")["x"], b["x"]);
    let mid = (a["x"].as_f64().unwrap() + b["x"].as_f64().unwrap()) / 2.0;
    assert!((p["x"].as_f64().unwrap() - mid).abs() < 1e-3, "{p}");
    assert!(p["y"].as_f64().unwrap() < a["y"].as_f64().unwrap(), "{p}");
    // the bundle has no block vocabulary (S11): one object, unlabelled
    assert_eq!(j["objects"].as_array().unwrap().len(), 1);
}

#[test]
fn labelled_element_is_a_scene_block_with_its_call() {
    let s: serde_json::Value =
        serde_json::from_str(&run("labelled-scene", LABELLED, "--scene")).unwrap();
    let blocks: Vec<_> = s["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "block")
        .collect();
    assert_eq!(blocks.len(), 1, "{s}");
    let b = blocks[0];
    assert_eq!(b["id"], "blk0");
    assert_eq!(b["label"], "R1");
    assert_eq!(b["items"], serde_json::json!(["s0"]));
    assert_eq!(
        b["span"],
        serde_json::json!({"line": 3, "col": 5, "end_col": 18})
    );
    assert_eq!(b["call"]["name"], "resistor");
    assert_eq!(b["call"]["args"], serde_json::json!(["A", "B"]));
    assert_eq!(b["call"]["raw_args"][1]["text"], "B");
}
