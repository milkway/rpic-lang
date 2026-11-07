//! `--prelude NAME=FILE` (#450, DECISIONS E4) through the real binary: the
//! file is a named unit spliced before the input, so user positions never
//! shift, positions inside the prelude report `file: NAME`, and a prelude
//! macro called from the input keeps the input's call-site provenance.

use std::path::PathBuf;
use std::process::{Command, Output};

const LIB: &str = "define vsrc { [ line from $1 to $2; circle rad 0.1 at last line.c ] }\n";

fn dir() -> PathBuf {
    // per process: two checkouts running `cargo test` must not race
    let d = std::env::temp_dir().join(format!("rpic-prelude-tests-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("mk temp dir");
    d
}

fn rpic(args: &[&str], name: &str, src: &str) -> Output {
    let d = dir();
    std::fs::write(d.join("lib.pic"), LIB).expect("write lib");
    std::fs::write(d.join("bad.pic"), "box\nbxo\n").expect("write bad");
    let pic = d.join(format!("{name}.pic"));
    std::fs::write(&pic, src).expect("write pic");
    Command::new(env!("CARGO_BIN_EXE_rpic"))
        .current_dir(&d)
        .args(args)
        .arg(&pic)
        .output()
        .expect("run rpic")
}

fn stdout(o: Output) -> String {
    assert!(
        o.status.success(),
        "rpic failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).expect("utf8")
}

#[test]
fn prelude_macro_keeps_the_user_call_site_in_the_scene() {
    let out = stdout(rpic(
        &["--prelude", "dune=lib.pic", "--scene"],
        "call",
        "A: (0,0); B: (1,0)\nV1: vsrc(A,B)\n",
    ));
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let items = v["items"].as_array().unwrap();
    let blk = items.iter().find(|i| i["kind"] == "block").unwrap();
    assert_eq!(blk["label"], "V1");
    assert_eq!(blk["span"]["line"], 2);
    assert!(blk["span"].get("file").is_none(), "{blk}");
    assert_eq!(blk["call"]["raw_args"][0]["line"], 2);
    assert!(
        items
            .iter()
            .filter(|i| i["kind"] != "block")
            .all(|i| i["span"]["file"] == "dune"),
        "{items:?}"
    );
}

#[test]
fn prelude_errors_and_user_errors_keep_their_own_positions() {
    let o = rpic(&["--prelude", "house=bad.pic", "--json"], "ok", "circle\n");
    let v: serde_json::Value = serde_json::from_str(&stdout(o)).unwrap();
    assert_eq!(v["error_info"]["file"], "house");
    assert_eq!(v["error_info"]["line"], 2);
    let o = rpic(
        &["--prelude", "dune=lib.pic", "--json"],
        "bad-user",
        "box\nbxo\n",
    );
    let v: serde_json::Value = serde_json::from_str(&stdout(o)).unwrap();
    assert!(v["error_info"]["file"].is_null(), "{v}");
    assert_eq!(v["error_info"]["line"], 2);
}

#[test]
fn prelude_flag_is_validated() {
    for (arg, want) in [
        ("noequals", "NAME=FILE"),
        ("x=missing-file.pic", "cannot read prelude"),
        ("circuits=lib.pic", "reserved"),
    ] {
        let o = rpic(&["--prelude", arg, "--svg"], "v", "box\n");
        assert!(!o.status.success(), "{arg}");
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(err.contains(want), "{arg}: {err}");
    }
    // repeatable, in order: the second unit sees the first
    let d = dir();
    std::fs::write(d.join("w.pic"), "define w { 2 }\n").unwrap();
    std::fs::write(d.join("uses.pic"), "wid2 = w\n").unwrap();
    let svg = stdout(rpic(
        &["--prelude", "a=w.pic", "--prelude", "b=uses.pic", "--svg"],
        "order",
        "box wid wid2\n",
    ));
    assert_eq!(svg, stdout(rpic(&["--svg"], "order-plain", "box wid 2\n")));
}
