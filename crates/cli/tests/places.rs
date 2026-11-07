//! #453: the `--json` bundle's `places` carry the enclosing block path, in
//! the scene's vocabulary, only for places defined inside `[ … ]`.

use std::path::PathBuf;
use std::process::Command;

fn write_pic(name: &str, src: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&p, src).expect("write pic");
    p
}

fn json_of(flag: &str, pic: &PathBuf) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_rpic"))
        .arg(flag)
        .arg(pic)
        .output()
        .expect("run rpic");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json")
}

#[test]
fn bundle_places_carry_the_scene_block_path() {
    let pic = write_pic(
        "places453.pic",
        "A: (0,0)\nB: [ C: (1,1); box at C; [ E: (0,0); circle ] ]\nD: (2,0)\n",
    );
    let bundle = json_of("--json", &pic);
    let scene = json_of("--scene", &pic);
    let bp = bundle["places"].as_array().unwrap();
    let sp = scene["places"].as_array().unwrap();
    assert_eq!(bp.len(), 4);
    assert_eq!(bp.len(), sp.len());
    for (b, s) in bp.iter().zip(sp) {
        assert_eq!(b["label"], s["label"]);
        // same key, same value — or absent in both
        assert_eq!(b.get("block"), s.get("block"), "{b} vs {s}");
    }
    let by = |n: &str| bp.iter().find(|p| p["label"] == n).unwrap();
    assert!(by("A").get("block").is_none());
    assert!(by("D").get("block").is_none());
    assert_eq!(by("C")["block"], serde_json::json!(["blk1"]));
    assert_eq!(by("E")["block"], serde_json::json!(["blk1", "blk0"]));
}
