//! Corpus-drift guard (#364): every committed `examples/**/*.svg` must be
//! exactly what the current binary renders from its sibling `.pic` with
//! `-c --svg` (the committed-corpus convention — circuit library on, no
//! texlabels). Before this guard, intentional byte-moving changes (#274
//! label-ink, #278 xcolor→hex) regenerated only part of the corpus and 36
//! files drifted silently.
//!
//! On a mismatch: regenerate the sibling (`rpic -c --svg <file>.pic`, stdout
//! into `<file>.svg`), render an old-vs-new contact sheet, and LOOK at every
//! figure before committing.

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

// Windows runners check out text files with CRLF (no .gitattributes pins
// them); the binary always emits LF.
fn lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[test]
fn committed_corpus_svgs_match_current_render() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut pics = Vec::new();
    collect_pics(&root, &mut pics);
    pics.sort();
    assert!(
        pics.len() > 100,
        "corpus went missing? found {}",
        pics.len()
    );

    let mut stale = Vec::new();
    let mut checked = 0;
    for pic in &pics {
        let svg = pic.with_extension("svg");
        let Ok(committed) = std::fs::read_to_string(&svg) else {
            continue; // shims and svg-less demos have no committed render
        };
        let out = Command::new(env!("CARGO_BIN_EXE_rpic"))
            .args(["-c", "--svg"])
            .arg(pic)
            .output()
            .expect("run rpic");
        assert!(
            out.status.success(),
            "{} failed to render: {}",
            pic.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        if lf(&String::from_utf8_lossy(&out.stdout)) != lf(&committed) {
            stale.push(svg);
        }
        checked += 1;
    }
    assert!(checked > 100, "too few sibling svgs? checked {checked}");
    assert!(
        stale.is_empty(),
        "{} committed svg(s) drifted from the current render — regenerate \
         with `rpic -c --svg <file>.pic > <file>.svg` and contact-sheet QA \
         the result (#364):\n{}",
        stale.len(),
        stale
            .iter()
            .map(|p| format!("  {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
