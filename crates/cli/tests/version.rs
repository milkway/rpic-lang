//! `rpic --version` / `-V` (#458): one line, `rpic <crate version>`, exit 0,
//! nothing on stderr. The binary had no version flag before 0.18.0.

use std::process::Command;

fn run(flag: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rpic"))
        .arg(flag)
        .output()
        .expect("run rpic")
}

#[test]
fn version_flag_prints_the_crate_version() {
    for flag in ["--version", "-V"] {
        let out = run(flag);
        assert!(out.status.success(), "{flag}: {:?}", out.status);
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert_eq!(
            stdout,
            format!("rpic {}\n", env!("CARGO_PKG_VERSION")),
            "{flag}"
        );
        assert!(
            out.stderr.is_empty(),
            "{flag}: stderr {:?}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn help_lists_the_version_flag() {
    let out = run("--help");
    assert!(out.status.success());
    let help = String::from_utf8(out.stderr).unwrap();
    assert!(help.contains("-V, --version"), "{help}");
}
