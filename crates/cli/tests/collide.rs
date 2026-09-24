//! Collision corpus: every rpic extension word must behave as an ordinary
//! identifier wherever the dpic grammar admits one, so that a dpic document
//! that happens to use the word as a name keeps its meaning. Three positions
//! are visited: variable (`w = 2; box wid w`), macro name (`define w { box };
//! w`) and optional attribute operand (`w = 0.3; box dashed w`). dpic
//! 2025.08.01 accepts all 120 programs. Each program is rendered next to the
//! same program with the neutral name `zz`; the outputs must be identical.
//!
//! The words that are environment variables by design (the canvas margins,
//! `texlabels`) are the documented exceptions: assigning them has an effect,
//! and they cannot name a macro. That set is frozen here so it can only
//! change deliberately.
//!
//! Written for the rpic method paper (rpic-papers, cola/tools/collide.py),
//! where the same corpus found 60 collisions before rpic-lang #386–#388.

use std::path::Path;
use std::process::Command;

const WORDS: &[&str] = &[
    "margin",
    "topmargin",
    "bottommargin",
    "leftmargin",
    "rightmargin",
    "canvas",
    "behind",
    "dot",
    "bold",
    "italic",
    "mono",
    "font",
    "fontsize",
    "big",
    "small",
    "rotated",
    "aligned",
    "fit",
    "opacity",
    "gradient",
    "hatch",
    "crosshatch",
    "brace",
    "class",
    "link",
    "close",
    "previous",
    "texlabels",
    "animate",
    "draggable",
    "after",
    "delay",
    "repeat",
    "yoyo",
    "ease",
    "along",
    "stagger",
    "out",
    "scroll",
    "into",
];

/// Environment variables of the extension surface: assignment is the
/// feature, so the variable position renders differently from the control
/// and the macro-name position is rejected. Frozen on purpose.
const ENV_VARS: &[&str] = &[
    "margin",
    "topmargin",
    "bottommargin",
    "leftmargin",
    "rightmargin",
    "texlabels",
];

#[derive(Debug, PartialEq)]
enum Verdict {
    Ordinary,
    Rejected,
    Silent,
}

fn render(dir: &Path, tag: &str, src: &str) -> Option<String> {
    let path = dir.join(format!("{tag}.pic"));
    std::fs::write(&path, src).expect("write probe");
    let out = Command::new(env!("CARGO_BIN_EXE_rpic"))
        .args(["--svg"])
        .arg(&path)
        .output()
        .expect("run rpic");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

type Probe = fn(&str) -> String;

fn verdict(dir: &Path, pos: &str, w: &str, mk: Probe) -> Verdict {
    let ctl = render(dir, &format!("{pos}-ctl"), &mk("zz")).expect("control renders");
    match render(dir, &format!("{pos}-{w}"), &mk(w)) {
        None => Verdict::Rejected,
        Some(out) if out == ctl => Verdict::Ordinary,
        Some(_) => Verdict::Silent,
    }
}

#[test]
fn extension_words_are_ordinary_identifiers_in_dpic_positions() {
    let dir = std::env::temp_dir().join(format!("rpic-collide-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let positions: [(&str, Probe); 3] = [
        ("variable", |w| format!(".PS\n{w} = 2\nbox wid {w}\n.PE\n")),
        ("macro", |w| {
            format!(".PS\ndefine {w} {{ box }}\n{w}\n.PE\n")
        }),
        ("operand", |w| {
            format!(".PS\n{w} = 0.3\nbox dashed {w}\n.PE\n")
        }),
    ];
    let mut bad = Vec::new();
    for w in WORDS {
        for (pos, mk) in positions {
            let got = verdict(&dir, pos, w, mk);
            let want = match (ENV_VARS.contains(w), pos) {
                (true, "variable") | (true, "operand") if *w != "texlabels" => Verdict::Silent,
                (true, "macro") => Verdict::Rejected,
                _ => Verdict::Ordinary,
            };
            if got != want {
                bad.push(format!("  {w:<13} as {pos:<8}: {got:?}, expected {want:?}"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        bad.is_empty(),
        "{} collision-corpus cell(s) changed (dpic accepts every one of these programs):\n{}",
        bad.len(),
        bad.join("\n")
    );
}
