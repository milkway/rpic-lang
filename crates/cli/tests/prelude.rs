//! `--prelude NAME=FILE` (#450, DECISIONS E4) through the real binary: the
//! file is a named unit spliced before the input, so user positions never
//! shift, positions inside the prelude report `file: NAME`, and a prelude
//! macro called from the input keeps the input's call-site provenance.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LIB: &str = "define vsrc { [ line from $1 to $2; circle rad 0.1 at last line.c ] }\n";

/// One scratch directory per test (the tests run in parallel), removed when
/// the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let d =
            std::env::temp_dir().join(format!("rpic-prelude-tests-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&d).expect("mk temp dir");
        std::fs::write(d.join("lib.pic"), LIB).expect("write lib");
        std::fs::write(d.join("bad.pic"), "box\nbxo\n").expect("write bad");
        Scratch(d)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, text).expect("write");
        p
    }

    /// Run rpic on `src` (written as `<name>.pic` here) from `cwd`.
    fn rpic_in(&self, cwd: &Path, args: &[&str], name: &str, src: &str) -> Output {
        let pic = self.write(&format!("{name}.pic"), src);
        Command::new(env!("CARGO_BIN_EXE_rpic"))
            .current_dir(cwd)
            .args(args)
            .arg(&pic)
            .output()
            .expect("run rpic")
    }

    fn rpic(&self, args: &[&str], name: &str, src: &str) -> Output {
        self.rpic_in(&self.0, args, name, src)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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
    let t = Scratch::new("call-site");
    let out = stdout(t.rpic(
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
    let t = Scratch::new("errors");
    let o = t.rpic(&["--prelude", "house=bad.pic", "--json"], "ok", "circle\n");
    let v: serde_json::Value = serde_json::from_str(&stdout(o)).unwrap();
    assert_eq!(v["error_info"]["file"], "house");
    assert_eq!(v["error_info"]["line"], 2);
    let o = t.rpic(
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
    let t = Scratch::new("validate");
    for (arg, want) in [
        ("noequals", "NAME=FILE"),
        ("x=missing-file.pic", "cannot read prelude"),
        ("circuits=lib.pic", "reserved"),
    ] {
        let o = t.rpic(&["--prelude", arg, "--svg"], "v", "box\n");
        assert!(!o.status.success(), "{arg}");
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(err.contains(want), "{arg}: {err}");
    }
    // repeatable, in order: the second unit sees the first
    t.write("w.pic", "define w { 2 }\n");
    t.write("uses.pic", "wid2 = w\n");
    let svg = stdout(t.rpic(
        &["--prelude", "a=w.pic", "--prelude", "b=uses.pic", "--svg"],
        "order",
        "box wid wid2\n",
    ));
    assert_eq!(
        svg,
        stdout(t.rpic(&["--svg"], "order-plain", "box wid 2\n"))
    );
}
