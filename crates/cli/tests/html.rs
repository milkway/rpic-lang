//! `--html` output tests (#357): the page embeds the player and pulls only
//! the GSAP plugins the used effects require; a plain drawing gets a fully
//! static page. Also guards the embedded player copy against drifting from
//! the canonical `bindings/js/player.js`.

use std::path::Path;
use std::process::Command;

fn render_html(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join("rpic-html-tests");
    std::fs::create_dir_all(&dir).expect("mk temp dir");
    let pic = dir.join(format!("{name}.pic"));
    std::fs::write(&pic, src).expect("write pic");
    let out = Command::new(env!("CARGO_BIN_EXE_rpic"))
        .arg("--html")
        .arg(&pic)
        .output()
        .expect("run rpic");
    assert!(
        out.status.success(),
        "--html failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8 html")
}

#[test]
fn embedded_player_matches_canonical_bindings_copy() {
    let canonical = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bindings/js/player.js");
    let Ok(canonical) = std::fs::read_to_string(canonical) else {
        return; // published crate: no workspace around us
    };
    let embedded = include_str!("../assets/player.js");
    assert_eq!(
        embedded, canonical,
        "crates/cli/assets/player.js drifted from bindings/js/player.js — \
         copy the canonical file over the crate's copy"
    );
}

#[test]
fn plain_drawing_gets_a_static_page_with_no_scripts() {
    let html = render_html("static", "box \"A\"; arrow; box \"B\"\n");
    assert!(html.starts_with("<!doctype html>"), "{html}");
    assert!(html.contains("<svg"), "{html}");
    assert!(html.contains("<title>static</title>"), "{html}");
    assert!(
        !html.contains("<script"),
        "static page must have no scripts:\n{html}"
    );
    assert!(!html.contains("gsap"), "{html}");
}

#[test]
fn animated_drawing_embeds_player_and_gsap_core_only() {
    let html = render_html("fade", "box \"A\"\nanimate last box with \"fade\"\n");
    assert!(html.contains("gsap@3.13.0/dist/gsap.min.js"), "{html}");
    assert!(html.contains("integrity=\"sha384-"), "{html}");
    assert!(html.contains("crossorigin=\"anonymous\""), "{html}");
    // the embedded player and the inline manifest
    assert!(html.contains("function animate("), "{html}");
    assert!(html.contains("preconvertGeometry"), "{html}");
    assert!(
        html.contains("const animations = [{\"id\":\"s0\",\"effect\":\"fade\""),
        "{html}"
    );
    // fade needs no plugin: no plugin file tag, no generated registerPlugin
    // line (the embedded player's own comments do mention plugins)
    assert!(!html.contains("Plugin.min.js"), "{html}");
    assert!(!html.contains("\ngsap.registerPlugin("), "{html}");
}

#[test]
fn effects_pull_exactly_their_plugins() {
    let html = render_html(
        "mover",
        "L: line right 2\nD: dot at L.start\nanimate D with \"move\" along L\n",
    );
    assert!(html.contains("MotionPathPlugin.min.js"), "{html}");
    assert!(
        html.contains("\ngsap.registerPlugin(MotionPathPlugin);"),
        "{html}"
    );
    // no MorphSVG *tag* (the player's comments mention the plugin by name)
    assert!(!html.contains("MorphSVGPlugin.min.js"), "{html}");

    let html = render_html(
        "wiggler",
        "box \"hey\"\nanimate last box with \"wiggle\" wiggles 4\n",
    );
    assert!(html.contains("CustomEase.min.js"), "{html}");
    assert!(html.contains("CustomWiggle.min.js"), "{html}");
    assert!(
        html.contains("\ngsap.registerPlugin(CustomEase, CustomWiggle);"),
        "{html}"
    );
    assert!(!html.contains("MotionPathPlugin.min.js"), "{html}");
}

#[test]
fn draggable_pulls_draggable_and_inertia() {
    let html = render_html("drag", "A: circle \"A\" rad 0.3\ndraggable A inertia\n");
    assert!(html.contains("Draggable.min.js"), "{html}");
    assert!(html.contains("InertiaPlugin.min.js"), "{html}");
    assert!(
        html.contains("interactive(stage, interactions, Draggable)"),
        "{html}"
    );
    assert!(
        html.contains("const interactions = [{\"id\":\"s0\",\"kind\":\"drag\",\"inertia\":true}]"),
        "{html}"
    );
}

#[test]
fn scroll_hint_wires_scrolltrigger_and_scroll_room() {
    let html = render_html(
        "scrolly",
        "box \"A\"\nanimate last box with \"fade\"\nanimate scroll\n",
    );
    assert!(html.contains("ScrollTrigger.min.js"), "{html}");
    assert!(html.contains("ScrollTrigger.create"), "{html}");
    assert!(html.contains("scrub: true"), "{html}");
    assert!(html.contains("min-height: 300vh"), "{html}");
    assert!(html.contains("position: sticky"), "{html}");

    // without the hint: autoplay, no ScrollTrigger anywhere
    let html = render_html("noscroll", "box \"A\"\nanimate last box with \"fade\"\n");
    assert!(!html.contains("ScrollTrigger"), "{html}");
    assert!(!html.contains("sticky"), "{html}");
}
