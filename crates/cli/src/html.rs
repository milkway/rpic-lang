//! `--html` output (#357): a single self-contained page that plays the
//! drawing's animations on open. Inline SVG + inline manifests + the GSAP
//! player embedded, plus pinned CDN `<script>` tags for GSAP — and **only**
//! the plugins the used effects actually require. A drawing with no
//! animations and no interactions becomes a plain static page with no
//! scripts at all.
//!
//! `assets/player.js` is a synced copy of the canonical
//! `bindings/js/player.js` (the npm package's zero-import player); the copy
//! exists because `include_str!` cannot reach outside the crate once
//! published to crates.io. A test guards the two files byte-identical.

use rpic_core::Drawing;

/// The embedded GSAP player (synced copy of `bindings/js/player.js`).
pub const PLAYER_JS: &str = include_str!("../assets/player.js");

/// Pinned GSAP version for the CDN tags.
const GSAP_VERSION: &str = "3.13.0";

/// (file, sha384 SRI hash) for every GSAP dist file the page may need.
/// Hashes are of the exact pinned bytes on jsdelivr — a mismatch blocks the
/// script, so bumping GSAP_VERSION requires recomputing all of them.
const GSAP_CORE: (&str, &str) = (
    "gsap.min.js",
    "sha384-HOvlOYPIs/zjoIkWUGXkVmXsjr8GuZLV+Q+rcPwmJOVZVpvTSXQChiN4t9Euv9Vc",
);
const PLUGIN_MOTIONPATH: (&str, &str) = (
    "MotionPathPlugin.min.js",
    "sha384-ZdcNM4JVCcTz+LG7hy3tkzchm/ljdYJcwthjW8G4vqAXApMm4ve+3ByX+RfrAlFt",
);
const PLUGIN_MORPHSVG: (&str, &str) = (
    "MorphSVGPlugin.min.js",
    "sha384-8oj95/2bNit2rlOOsJCaGBxSngfBWjoyQ8MtNbxmuk0v0B2nAK3Bwb1OJ5fWMWlk",
);
const PLUGIN_SCRAMBLETEXT: (&str, &str) = (
    "ScrambleTextPlugin.min.js",
    "sha384-QsPhgdP78a1erQNT34dj/oyh7uPCXUnT/FkptUa/56LnSGsbTBmdiAOfB7j+8K4A",
);
const PLUGIN_CUSTOMEASE: (&str, &str) = (
    "CustomEase.min.js",
    "sha384-JCMGAgtMgo/19ttIm8BSnUFxOA5KAxaKT7jZdTDtaL0df8+CHKRo9XFCmhsvUlXG",
);
const PLUGIN_CUSTOMWIGGLE: (&str, &str) = (
    "CustomWiggle.min.js",
    "sha384-f//wUEyhrA7TxeRQgq5WVHz7oaPVMZXGy0Q15YPvAPL1x9+DQkPGOgUrT2HJWrRs",
);
const PLUGIN_DRAGGABLE: (&str, &str) = (
    "Draggable.min.js",
    "sha384-IJ0us7IOIdtwqJI237hVYv2U3Qt2IL5AkFeroJOnvSYXbNLBxkQ9xQ2sdidcbzTr",
);
const PLUGIN_INERTIA: (&str, &str) = (
    "InertiaPlugin.min.js",
    "sha384-hyQEhO+HeN2EhyXzCsnqdqptUcD/Mw8xhmmTdECpYOUUNLvSfImrkF05wdz+ncnh",
);
const PLUGIN_SCROLLTRIGGER: (&str, &str) = (
    "ScrollTrigger.min.js",
    "sha384-P8VzCVnT9NBUkMrpcIZrJbA7EBjJvh/fJS6PmP+4nLIM284DtsImIv8D0fFjIkeh",
);

fn escape_html_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The plugin tags (beyond the GSAP core) this drawing needs, with the
/// matching global names to `registerPlugin`. Order is stable: effects
/// first, interaction plugins, scroll last.
fn needed_plugins(d: &Drawing) -> (Vec<(&'static str, &'static str)>, Vec<&'static str>) {
    let mut files = Vec::new();
    let mut globals = Vec::new();
    let has = |effect: &str| d.anims.iter().any(|a| a.effect == effect);
    if has("move") {
        files.push(PLUGIN_MOTIONPATH);
        globals.push("MotionPathPlugin");
    }
    if has("morph") {
        files.push(PLUGIN_MORPHSVG);
        globals.push("MorphSVGPlugin");
    }
    if has("scramble") {
        files.push(PLUGIN_SCRAMBLETEXT);
        globals.push("ScrambleTextPlugin");
    }
    if has("wiggle") {
        files.push(PLUGIN_CUSTOMEASE);
        globals.push("CustomEase");
        files.push(PLUGIN_CUSTOMWIGGLE);
        globals.push("CustomWiggle");
    }
    if !d.interactions.is_empty() {
        files.push(PLUGIN_DRAGGABLE);
        globals.push("Draggable");
        if d.interactions.iter().any(|it| it.inertia) {
            files.push(PLUGIN_INERTIA);
            globals.push("InertiaPlugin");
        }
    }
    if d.anim_scroll {
        files.push(PLUGIN_SCROLLTRIGGER);
        globals.push("ScrollTrigger");
    }
    (files, globals)
}

fn script_tag(file: &str, sri: &str) -> String {
    format!(
        "  <script src=\"https://cdn.jsdelivr.net/npm/gsap@{GSAP_VERSION}/dist/{file}\"\n          \
         integrity=\"{sri}\" crossorigin=\"anonymous\"></script>\n"
    )
}

/// Render the drawing as a self-contained HTML page. `title` is typically
/// the input file stem.
pub fn to_html(d: &Drawing, title: &str) -> String {
    let svg = rpic_core::to_svg(d);
    let animated = !d.anims.is_empty() || !d.interactions.is_empty();
    let title = escape_html_text(title);

    let mut head_scripts = String::new();
    let mut body_script = String::new();
    // `animate scroll` scrubs the timeline with scroll position, so the page
    // needs scroll room: a tall body with the stage stuck near the top.
    let scroll_css = if animated && d.anim_scroll {
        "    body { min-height: 300vh; align-items: flex-start; }\n    \
         main { position: sticky; top: 15vh; }\n"
    } else {
        ""
    };

    if animated {
        let (files, globals) = needed_plugins(d);
        head_scripts.push_str(&script_tag(GSAP_CORE.0, GSAP_CORE.1));
        for (file, sri) in files {
            head_scripts.push_str(&script_tag(file, sri));
        }

        let mut js = String::new();
        js.push_str("<script type=\"module\">\n");
        js.push_str(PLAYER_JS);
        js.push('\n');
        if !globals.is_empty() {
            js.push_str(&format!("gsap.registerPlugin({});\n", globals.join(", ")));
        }
        js.push_str(&format!(
            "const animations = {};\n",
            rpic_core::animations_json(d)
        ));
        js.push_str(&format!(
            "const interactions = {};\n",
            rpic_core::interactions_json(d)
        ));
        js.push_str("const stage = document.getElementById('rpic-stage');\n");
        js.push_str("const tl = animate(stage, animations, gsap);\n");
        if d.anim_scroll {
            js.push_str(
                "tl.pause();\n\
                 ScrollTrigger.create({ animation: tl, trigger: document.body, \
                 start: 'top top', end: 'bottom bottom', scrub: true });\n",
            );
        }
        js.push_str("if (interactions.length) interactive(stage, interactions, Draggable);\n");
        js.push_str("</script>\n");
        body_script = js;
    }

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  \
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  \
         <title>{title}</title>\n  <style>\n    \
         body {{ margin: 0; min-height: 100vh; display: flex; align-items: center; \
         justify-content: center; background: #fff; }}\n    \
         main {{ padding: 2rem; }}\n    \
         main svg {{ max-width: 100%; height: auto; }}\n{scroll_css}  </style>\n\
         {head_scripts}</head>\n<body>\n<main id=\"rpic-stage\">\n{svg}</main>\n\
         {body_script}</body>\n</html>\n"
    )
}
