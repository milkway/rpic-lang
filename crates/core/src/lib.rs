//! rpic-core — engine for the `pic` picture-drawing language.
//!
//! Pipeline: source → [`lexer`] → [`parser`] → [`ast`] → [`eval`] → placed
//! primitives ([`ir`]) → render backends ([`svg`], later PNG/PDF). This crate
//! is pure (no file I/O); the CLI and WASM wrappers drive it.
//!
//! Note: [`eval`] is the pic-language *interpreter* — a safe walker over a typed
//! expression/geometry tree. It executes no arbitrary code.

pub mod ast;
pub mod color;
pub mod diagnostic;
pub mod eval;
pub mod geom;
pub mod ir;
pub mod lexer;
mod math;
pub mod parser;
pub mod svg;
pub mod token;

pub use ast::{IncludeCtx, IncludePolicy};
pub use diagnostic::{CompileError, Diagnostic, Span};
pub use eval::{EvalError, EvalLimits, eval, eval_with_limits};
pub use ir::Drawing;
pub use lexer::{LexError, lex};
pub use math::{MathSpan, set_math_renderer};
pub use parser::{ParseError, parse, parse_in_dir, parse_with_prelude};
pub use svg::to_svg;
pub use token::Token;

/// Bundled native circuit-element library (the `define` dialect). Prepend it to
/// source to use `resistor`, `capacitor`, … See `std/circuits.pic`.
pub const CIRCUITS: &str = include_str!("std/circuits.pic");

/// Compile pic source into a placed-primitive [`Drawing`].
pub fn compile(src: &str) -> Result<Drawing, String> {
    compile_in_dir(src, None)
}

/// Compile pic source, resolving `copy "file"` includes relative to `base`.
pub fn compile_in_dir(src: &str, base: Option<&std::path::Path>) -> Result<Drawing, String> {
    let picture = parser::parse_in_dir(src, base).map_err(|e| e.to_string())?;
    eval(&picture).map_err(|e| e.to_string())
}

/// Compile pic source directly to an SVG string.
pub fn render_svg(src: &str) -> Result<String, String> {
    Ok(to_svg(&compile(src)?))
}

/// Render pic source to SVG, resolving `copy "file"` includes relative to `base`.
pub fn render_svg_in_dir(src: &str, base: Option<&std::path::Path>) -> Result<String, String> {
    Ok(to_svg(&compile_in_dir(src, base)?))
}

/// Build the JSON animation manifest array (`[{id,effect,start,duration},…]`)
/// for an already-compiled drawing.
pub fn animations_json(d: &Drawing) -> String {
    let mut s = String::from("[");
    for (i, a) in d.anims.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        // Only the always-present keys are emitted for a plain animation; the
        // optional repeat/yoyo/ease/path/color keys ride along only when set,
        // so manifests that don't use them stay byte-identical.
        s.push_str(&format!(
            "{{\"id\":\"s{}\",\"effect\":\"{}\",\"start\":{},\"duration\":{}",
            a.shape,
            json_str(&a.effect),
            a.start,
            a.duration
        ));
        if a.repeat != 0 {
            s.push_str(&format!(",\"repeat\":{}", a.repeat));
        }
        if a.yoyo {
            s.push_str(",\"yoyo\":true");
        }
        if let Some(ease) = &a.ease {
            s.push_str(&format!(",\"ease\":\"{}\"", json_str(ease)));
        }
        if let Some(path) = a.path {
            s.push_str(&format!(",\"path\":\"s{path}\""));
        }
        if let Some(color) = &a.color {
            s.push_str(&format!(",\"color\":\"{}\"", json_str(color)));
        }
        if a.out {
            s.push_str(",\"out\":true");
        }
        if let Some(from) = &a.from {
            s.push_str(&format!(",\"from\":\"{}\"", json_str(from)));
        }
        if let Some(morph) = a.morph {
            s.push_str(&format!(",\"morph\":\"s{morph}\""));
        }
        if a.type_word {
            s.push_str(",\"unit\":\"word\"");
        }
        if let Some(chars) = &a.scramble_chars {
            s.push_str(&format!(",\"chars\":\"{}\"", json_str(chars)));
        }
        if let Some(wiggles) = a.wiggles {
            s.push_str(&format!(",\"wiggles\":{wiggles}"));
        }
        if let Some(from) = a.draw_from {
            s.push_str(&format!(",\"drawFrom\":{}", json_num(from)));
        }
        if let Some(to) = a.draw_to {
            s.push_str(&format!(",\"drawTo\":{}", json_num(to)));
        }
        s.push('}');
    }
    s.push(']');
    s
}

/// Build the JSON per-object geometry array: one entry per emitted
/// `<g id="sN">` group, with the shape kind, its bbox in SVG user units
/// (`null` for invisible shapes), and — when known — the source span of the
/// statement that produced it (`file` follows the same convention as
/// diagnostics: absent = user input, `"circuits"` = the `-c` library, else
/// the `copy` include name).
pub fn objects_json(d: &Drawing) -> String {
    let geoms = svg::object_geometries(d);
    let mut s = String::from("[");
    for (i, g) in geoms.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{{\"id\":\"s{}\",\"kind\":\"{}\"", i, g.kind));
        match g.bbox {
            Some((x, y, w, h)) => s.push_str(&format!(
                ",\"bbox\":{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
                json_num(x),
                json_num(y),
                json_num(w),
                json_num(h)
            )),
            None => s.push_str(",\"bbox\":null"),
        }
        if let Some(span) = d.shape_spans.get(i).and_then(|s| s.as_ref()) {
            s.push_str(&format!(
                ",\"line\":{},\"col\":{},\"end_col\":{}",
                span.line, span.col, span.end_col
            ));
            if let Some(f) = &span.file {
                s.push_str(&format!(",\"file\":\"{}\"", json_str(f)));
            }
        }
        s.push('}');
    }
    s.push(']');
    s
}

/// Format a coordinate for JSON: finite, trimmed like the SVG serializer.
fn json_num(x: f64) -> String {
    let v = if x.is_finite() { x } else { 0.0 };
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".into()
    } else {
        s.into()
    }
}

/// Build the JSON diagnostic array emitted by pic `print` statements.
pub fn diagnostics_json(d: &Drawing) -> String {
    let mut s = String::from("[");
    for (i, line) in d.diagnostics.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(&json_str(line));
        s.push('"');
    }
    s.push(']');
    s
}

/// Compile options for the `*_with_options` entry points — the library
/// equivalents of the CLI flags. Preludes (`circuits`, `texlabels`) are lexed
/// as their own named source units, NOT glued in front of the user's source,
/// so diagnostic positions always stay relative to the source they belong to
/// (`Diagnostic::file` names an include/library; `None` is the user's input).
#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    /// Load the embedded circuit-element library (the `-c` flag).
    pub circuits: bool,
    /// Inject `texlabels = 1` as an initializer (the `-t` flag); the source
    /// can still override it.
    pub texlabels: bool,
    /// Resolve `copy "file"` includes relative to this directory.
    pub base: Option<std::path::PathBuf>,
    /// Policy for `copy "file"` filesystem includes — leave the default
    /// (`Unrestricted`, the CLI behavior) for local use; embedders compiling
    /// untrusted source should pick `SandboxedToBase` or `Deny`.
    pub includes: IncludePolicy,
    /// Host ceiling for animation times in seconds. `None` uses the library
    /// default; the source-level `maxanimseconds` variable may lower this, but
    /// cannot raise it.
    pub max_animation_seconds: Option<f64>,
    /// Host ceiling for finite GSAP repeat counts. `None` uses the library
    /// default; `repeat -1` remains the explicit infinite-repeat sentinel. The
    /// source-level `maxanimrepeat` variable may lower this, but cannot raise it.
    pub max_animation_repeat: Option<i64>,
    /// Host ceiling for evaluated `for` loop iterations. `None` uses the
    /// library default. Set this lower for untrusted source.
    pub max_loop_iterations: Option<u64>,
    /// Host ceiling for emitted drawing shapes. `None` uses the library
    /// default. Set this lower for untrusted source.
    pub max_shapes: Option<usize>,
}

/// Compile pic source with [`CompileOptions`] into a [`Drawing`].
pub fn compile_with_options(src: &str, opts: &CompileOptions) -> Result<Drawing, String> {
    compile_with_diagnostics(src, opts).map_err(|e| e.message)
}

/// Like [`compile_with_options`], but failures return the structured
/// [`CompileError`] (flat message + [`Diagnostic`]) instead of a bare string —
/// for bindings that attach position data to exceptions/conditions.
pub fn compile_with_diagnostics(src: &str, opts: &CompileOptions) -> Result<Drawing, CompileError> {
    let picture = parse_options(src, opts).map_err(|e| CompileError {
        message: e.to_string(),
        info: Box::new(e.diagnostic()),
    })?;
    eval::eval_with_limits(&picture, opts.eval_limits()).map_err(|e| {
        // Eval errors carry their own diagnostic when the failure site had
        // one (deferred-parse errors, unknown labels, ordinals — with spans
        // straight from the failing reference's tokens, includes included).
        let info = e
            .info
            .clone()
            .unwrap_or_else(|| Box::new(Diagnostic::new("eval", e.msg.clone())));
        CompileError {
            message: e.msg,
            info,
        }
    })
}

/// Render pic source to SVG with [`CompileOptions`].
pub fn render_svg_with_options(src: &str, opts: &CompileOptions) -> Result<String, String> {
    Ok(to_svg(&compile_with_options(src, opts)?))
}

fn parse_options(src: &str, opts: &CompileOptions) -> Result<ast::Picture, ParseError> {
    parser::parse_with_prelude(
        src,
        ast::IncludeCtx::with_policy(opts.base.clone(), opts.includes),
        opts.circuits,
        opts.texlabels,
    )
}

impl CompileOptions {
    fn eval_limits(&self) -> EvalLimits {
        EvalLimits {
            max_animation_seconds: self
                .max_animation_seconds
                .unwrap_or(eval::DEFAULT_MAX_ANIMATION_SECONDS),
            max_animation_repeat: self
                .max_animation_repeat
                .unwrap_or(eval::DEFAULT_MAX_ANIMATION_REPEAT),
            max_loop_iterations: self
                .max_loop_iterations
                .unwrap_or(eval::DEFAULT_MAX_LOOP_ITERATIONS),
            max_shapes: self.max_shapes.unwrap_or(eval::DEFAULT_MAX_SHAPES),
        }
    }
}

/// Compile to a single JSON object `{ "svg": "...", "animations": [...],
/// "diagnostics": [...], "warnings": [...] }`, or `{ "error": "...",
/// "error_info": { ... } }` on failure. The flat `error` string is kept for
/// backward compatibility with older bindings.
pub fn compile_json(src: &str) -> String {
    compile_json_with_options(src, &CompileOptions::default())
}

/// Compile to a JSON bundle, resolving `copy "file"` includes relative to
/// `base`.
pub fn compile_json_in_dir(src: &str, base: Option<&std::path::Path>) -> String {
    compile_json_with_options(
        src,
        &CompileOptions {
            base: base.map(|p| p.to_path_buf()),
            ..Default::default()
        },
    )
}

/// Compile to a JSON bundle with [`CompileOptions`]. Diagnostic positions are
/// relative to the user's `src` (or carry a `file` naming the include/library
/// they are in) — never to a concatenated stream.
pub fn compile_json_with_options(src: &str, opts: &CompileOptions) -> String {
    match compile_with_diagnostics(src, opts) {
        Ok(d) => drawing_json(&d),
        Err(e) => error_json(&e.message, &e.info),
    }
}

fn drawing_json(d: &Drawing) -> String {
    // `animate scroll` adds a top-level hint; omitted when unset so plain
    // bundles stay byte-identical.
    let scroll = if d.anim_scroll {
        ",\"scroll\":true"
    } else {
        ""
    };
    // `draggable` objects ride a top-level `interactions` array; omitted when
    // there are none so plain bundles stay byte-identical.
    let interactions = if d.interactions.is_empty() {
        String::new()
    } else {
        format!(",\"interactions\":{}", interactions_json(d))
    };
    format!(
        "{{\"svg\":\"{}\",\"animations\":{},\"diagnostics\":{},\"warnings\":{},\"objects\":{}{}{}}}",
        json_str(&to_svg(d)),
        animations_json(d),
        diagnostics_json(d),
        diagnostics_json_structured(&d.warnings),
        objects_json(d),
        interactions,
        scroll
    )
}

/// Build the JSON `interactions` array (`[{id,kind:"drag",inertia?,bounds?,
/// axis?},…]`) for the `draggable` directives — the GSAP Draggable targets.
pub fn interactions_json(d: &Drawing) -> String {
    let mut s = String::from("[");
    for (i, x) in d.interactions.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{{\"id\":\"s{}\",\"kind\":\"drag\"", x.shape));
        if x.inertia {
            s.push_str(",\"inertia\":true");
        }
        if let Some(b) = x.bounds {
            s.push_str(&format!(",\"bounds\":\"s{b}\""));
        }
        if let Some(axis) = x.axis {
            s.push_str(&format!(",\"axis\":\"{axis}\""));
        }
        s.push('}');
    }
    s.push(']');
    s
}

fn error_json(message: &str, diagnostic: &Diagnostic) -> String {
    format!(
        "{{\"error\":\"{}\",\"error_info\":{},\"warnings\":[]}}",
        json_str(message),
        diagnostic_json(diagnostic)
    )
}

fn diagnostics_json_structured(items: &[Diagnostic]) -> String {
    let mut s = String::from("[");
    for (i, d) in items.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&diagnostic_json(d));
    }
    s.push(']');
    s
}

fn diagnostic_json(d: &Diagnostic) -> String {
    let mut s = String::from("{");
    s.push_str(&format!("\"message\":\"{}\"", json_str(&d.message)));
    s.push_str(&format!(",\"line\":{}", json_opt_u32(d.line)));
    s.push_str(&format!(",\"col\":{}", json_opt_u32(d.col)));
    s.push_str(&format!(",\"end_col\":{}", json_opt_u32(d.end_col)));
    s.push_str(&format!(",\"file\":{}", json_opt_str(d.file.as_deref())));
    s.push_str(&format!(",\"kind\":\"{}\"", json_str(&d.kind)));
    s.push_str(&format!(",\"found\":{}", json_opt_str(d.found.as_deref())));
    s.push_str(&format!(
        ",\"expected\":{}",
        json_opt_str(d.expected.as_deref())
    ));
    s.push_str(&format!(",\"hint\":{}", json_opt_str(d.hint.as_deref())));
    s.push('}');
    s
}

fn json_opt_u32(v: Option<u32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "null".into())
}

fn json_opt_str(v: Option<&str>) -> String {
    v.map(|s| format!("\"{}\"", json_str(s)))
        .unwrap_or_else(|| "null".into())
}

/// Escape a string for embedding inside a JSON string literal.
fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_bundles_svg_and_animations() {
        let j = compile_json("box\nanimate last box with \"fade\"");
        assert!(j.starts_with("{\"svg\":\"<svg"));
        assert!(j.contains("<g id=\\\"s0\\\">")); // stable id, JSON-escaped
        assert!(j.contains("\"animations\":[{\"id\":\"s0\",\"effect\":\"fade\""));
        assert!(j.contains("\"diagnostics\":[]"));
        // A plain animation carries none of the optional GSAP keys, so the
        // object closes right after duration (byte-inert when unused).
        assert!(j.contains("\"effect\":\"fade\",\"start\":0,\"duration\":0.6}"));
    }

    #[test]
    fn json_emits_repeat_yoyo_ease_only_when_set() {
        let j = compile_json(
            "box\nanimate last box with \"pop\" for 0.4 repeat -1 yoyo ease \"power2.inOut\"",
        );
        assert!(
            j.contains(
                "\"effect\":\"pop\",\"start\":0,\"duration\":0.4,\"repeat\":-1,\"yoyo\":true,\"ease\":\"power2.inOut\"}"
            ),
            "{j}"
        );
    }

    #[test]
    fn json_emits_draw_range_only_when_set() {
        // A plain draw carries no range keys (byte-inert).
        let j = compile_json("line right 2\nanimate last with \"draw\" for 1");
        assert!(
            j.contains("\"effect\":\"draw\",\"start\":0,\"duration\":1}"),
            "{j}"
        );
        // A window rides `drawFrom`/`drawTo` as fractions.
        let j = compile_json("line right 2\nanimate last with \"draw\" from 40% to 60% for 1");
        assert!(
            j.contains(
                "\"effect\":\"draw\",\"start\":0,\"duration\":1,\"drawFrom\":0.4,\"drawTo\":0.6}"
            ),
            "{j}"
        );
        // `to`-only leaves the start at the beginning.
        let j = compile_json("line right 2\nanimate last with \"draw\" to 60% for 1");
        assert!(j.contains("\"duration\":1,\"drawTo\":0.6}"), "{j}");
    }

    #[test]
    fn json_emits_move_path_reference() {
        let j = compile_json("L: line right 3\nD: dot at L.start\nanimate D with \"move\" along L");
        // The dot (s1) moves along the line (s0); the path id rides the entry.
        assert!(
            j.contains(
                "\"id\":\"s1\",\"effect\":\"move\",\"start\":0,\"duration\":0.6,\"path\":\"s0\"}"
            ),
            "{j}"
        );
    }

    #[test]
    fn json_emits_highlight_colour() {
        let j = compile_json("box\nanimate last box with \"highlight\" to rgb(255,140,0)");
        assert!(
            j.contains(
                "\"effect\":\"highlight\",\"start\":0,\"duration\":0.6,\"color\":\"#ff8c00\"}"
            ),
            "{j}"
        );
    }

    #[test]
    fn json_emits_morph_target_reference() {
        let j = compile_json("A: box\nB: circle at A+(2,0)\nanimate A with \"morph\" into B for 1");
        assert!(
            j.contains(
                "\"id\":\"s0\",\"effect\":\"morph\",\"start\":0,\"duration\":1,\"morph\":\"s1\"}"
            ),
            "{j}"
        );
    }

    #[test]
    fn json_emits_scroll_hint_only_when_set() {
        let on = compile_json("box\nanimate last box with \"fade\"\nanimate scroll");
        assert!(on.contains(",\"scroll\":true}"), "{on}");
        let off = compile_json("box\nanimate last box with \"fade\"");
        assert!(!off.contains("scroll"), "{off}");
    }

    #[test]
    fn json_emits_out_and_slide_direction() {
        let j = compile_json("box\nanimate last box with \"slide\" from left for 0.4 out");
        assert!(
            j.contains(
                "\"effect\":\"slide\",\"start\":0,\"duration\":0.4,\"out\":true,\"from\":\"left\"}"
            ),
            "{j}"
        );
    }

    #[test]
    fn json_stagger_expands_to_plain_entries() {
        // stagger fans into one ordinary entry per child — no new key, so the
        // manifest schema is unchanged.
        let j = compile_json("B: [ box; box ]\nanimate B with \"fade\" for 0.2 stagger 0.1");
        assert!(
            j.contains(
                "[{\"id\":\"s0\",\"effect\":\"fade\",\"start\":0,\"duration\":0.2},{\"id\":\"s1\",\"effect\":\"fade\",\"start\":0.1,\"duration\":0.2}]"
            ),
            "{j}"
        );
    }

    #[test]
    fn json_exports_object_geometry() {
        // #227: one entry per <g id="sN">, bbox in the viewBox's units,
        // span of the producing statement.
        let j = compile_json("box wid 1 ht 0.5\narrow right 0.5");
        assert!(
            j.contains("\"objects\":[{\"id\":\"s0\",\"kind\":\"box\",\"bbox\":{"),
            "{j}"
        );
        // 1in × 0.5in box = 96 × 48 SVG px
        assert!(j.contains("\"w\":96,\"h\":48"), "{j}");
        assert!(j.contains("\"kind\":\"path\""), "{j}");
        assert!(j.contains("\"line\":2,\"col\":1"), "{j}");
    }

    #[test]
    fn json_object_geometry_marks_invisible_shapes() {
        let j = compile_json("box invis \"x\"");
        // the invisible box still draws its text: box shape null, text real
        assert!(j.contains("\"kind\":\"box\",\"bbox\":null"), "{j}");
    }

    #[test]
    fn json_object_geometry_names_library_sources() {
        // objects drawn by the circuits library carry file:"circuits";
        // the user's own statements stay file-less.
        let opts = CompileOptions {
            circuits: true,
            ..Default::default()
        };
        let j = compile_json_with_options("A:(0,0); B:(2,0)\nresistor(A,B)", &opts);
        assert!(j.contains("\"file\":\"circuits\""), "{j}");
    }

    #[test]
    fn json_object_geometry_matches_svg_rect() {
        // the exported bbox must agree with the emitted <rect> attributes
        let d = compile("box wid 1 ht 0.5").unwrap();
        let svg = to_svg(&d);
        let g = svg::object_geometries(&d);
        let (x, y, w, h) = g[0].bbox.unwrap();
        let rect = svg.lines().find(|l| l.contains("<rect")).unwrap();
        for (attr, v) in [("x", x), ("y", y), ("width", w), ("height", h)] {
            let needle = format!("{attr}=\"");
            let i = rect.find(&needle).unwrap() + needle.len();
            let s = &rect[i..rect[i..].find('"').unwrap() + i];
            let got: f64 = s.parse().unwrap();
            assert!((got - v).abs() < 1e-3, "{attr}: rect {got} vs bbox {v}");
        }
    }

    #[test]
    fn json_reports_errors() {
        let j = compile_json("copy \"oops\"");
        assert!(j.contains("\"error\""));
        assert!(j.contains("\"error_info\""));
    }

    #[test]
    fn json_reports_print_diagnostics() {
        let j = compile_json("print \"hi\"\nprint 2+3");
        assert!(j.contains("\"diagnostics\":[\"hi\",\"5\"]"));
    }

    #[test]
    fn copy_circuits_loads_the_embedded_library() {
        // `copy "circuits"` is the in-source spelling of `-c`: byte-identical
        // output, and it works with no base dir (the wasm/compile_json case,
        // where file includes are unavailable).
        let body = "A:(0,0); B:(2,0)\nresistor(A,B)";
        let via_copy = compile(&format!("copy \"circuits\"\n{body}")).unwrap();
        let via_flag = compile(&format!("{CIRCUITS}\n{body}")).unwrap();
        assert_eq!(to_svg(&via_copy), to_svg(&via_flag));
    }

    #[test]
    fn copy_circuits_is_idempotent_with_the_flag() {
        // `-c` plus an explicit `copy "circuits"` must not double-load (the
        // second load is skipped) — same bytes as the flag alone.
        let body = "A:(0,0); B:(2,0)\nresistor(A,B)";
        let both = compile(&format!("{CIRCUITS}\ncopy \"circuits\"\n{body}")).unwrap();
        let flag = compile(&format!("{CIRCUITS}\n{body}")).unwrap();
        assert_eq!(to_svg(&both), to_svg(&flag));
    }

    #[test]
    fn options_preludes_do_not_shift_user_positions() {
        // #181 acceptance: with the circuits/texlabels preludes on, an error
        // on the user's line 1 reports line 1 (not ~1093), file null.
        let opts = CompileOptions {
            circuits: true,
            texlabels: true,
            ..Default::default()
        };
        let j = compile_json_with_options("bxo\n", &opts);
        assert!(j.contains("\"line\":1"), "{j}");
        assert!(j.contains("\"col\":1"), "{j}");
        assert!(j.contains("\"file\":null"), "{j}");
        assert!(j.contains("\"error\":\"1:1: expected an object"), "{j}");
    }

    #[test]
    fn options_output_matches_the_prepending_it_replaces() {
        // The prelude splice must be behaviorally identical to the old
        // string-prepending: byte-identical SVG.
        let body = "A:(0,0); B:(2,0)\nresistor(A,B)";
        let opts = CompileOptions {
            circuits: true,
            ..Default::default()
        };
        let via_opts = compile_with_options(body, &opts).unwrap();
        let via_prepend = compile(&format!("{CIRCUITS}\n{body}")).unwrap();
        assert_eq!(to_svg(&via_opts), to_svg(&via_prepend));
    }

    #[test]
    fn options_texlabels_is_an_initializer_only() {
        // the source stays sovereign: `texlabels = 0` overrides the prelude
        let opts = CompileOptions {
            texlabels: true,
            ..Default::default()
        };
        let j = compile_json_with_options("texlabels = 0\nbox \"$x$\"\n", &opts);
        assert!(!j.contains("no math renderer"), "{j}");
    }

    #[test]
    fn options_cap_animation_limits() {
        let repeat_opts = CompileOptions {
            max_animation_repeat: Some(2),
            ..Default::default()
        };
        let err =
            compile_with_options("box\nanimate last box with \"fade\" repeat 3", &repeat_opts)
                .unwrap_err();
        assert!(err.contains("animation repeat must be at most 2"), "{err}");

        let err = compile_with_options(
            "maxanimrepeat = 3\nbox\nanimate last box with \"fade\" repeat 2",
            &repeat_opts,
        )
        .unwrap_err();
        assert!(err.contains("maxanimrepeat must be at most 2"), "{err}");

        assert!(
            compile_with_options(
                "maxanimrepeat = 1\nbox\nanimate last box with \"fade\" repeat 1",
                &repeat_opts,
            )
            .is_ok()
        );

        let seconds_opts = CompileOptions {
            max_animation_seconds: Some(0.5),
            ..Default::default()
        };
        let err =
            compile_with_options("box\nanimate last box with \"fade\"", &seconds_opts).unwrap_err();
        assert!(
            err.contains("animation duration must be at most 0.5"),
            "{err}"
        );
        assert!(
            compile_with_options(
                "maxanimseconds = 0.2\nbox\nanimate last box with \"fade\" for 0.2",
                &seconds_opts,
            )
            .is_ok()
        );
    }

    #[test]
    fn options_cap_eval_budgets() {
        let loop_opts = CompileOptions {
            max_loop_iterations: Some(2),
            ..Default::default()
        };
        let err = compile_with_options("for i = 1 to 3 do { bxo }", &loop_opts).unwrap_err();
        assert!(err.contains("for loop exceeded 2 iterations"), "{err}");

        let shape_opts = CompileOptions {
            max_shapes: Some(1),
            ..Default::default()
        };
        let err = compile_with_options("box\nbox", &shape_opts).unwrap_err();
        assert!(err.contains("drawing exceeded 1 shapes"), "{err}");
    }

    #[test]
    fn options_reject_invalid_animation_limits() {
        let repeat_opts = CompileOptions {
            max_animation_repeat: Some(-1),
            ..Default::default()
        };
        let err = compile_with_options("box", &repeat_opts).unwrap_err();
        assert!(err.contains("max_animation_repeat option must be non-negative"));

        let seconds_opts = CompileOptions {
            max_animation_seconds: Some(f64::INFINITY),
            ..Default::default()
        };
        let err = compile_with_options("box", &seconds_opts).unwrap_err();
        assert!(err.contains("max_animation_seconds option must be finite"));
    }

    #[test]
    fn diagnostics_inside_an_include_name_the_file() {
        // #181 acceptance: a warning (or error) inside a `copy`'d file carries
        // the include's name and include-relative position.
        let dir = std::env::temp_dir().join(format!("rpic_incl_diag_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("warn.pic"), "# comment\nbox \"a\" dashd\n").unwrap();
        let j = compile_json_in_dir("circle\ncopy \"warn.pic\"", Some(dir.as_path()));
        assert!(j.contains("\"kind\":\"ignored_attribute\""), "{j}");
        assert!(j.contains("\"file\":\"warn.pic\""), "{j}");
        assert!(j.contains("\"line\":2"), "{j}"); // include-relative, not stream

        std::fs::write(dir.join("bad.pic"), "bxo\n").unwrap();
        let e = compile_json_in_dir("circle\ncopy \"bad.pic\"", Some(dir.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            e.contains("\"error\":\"bad.pic:1:1: expected an object"),
            "{e}"
        );
        assert!(e.contains("\"file\":\"bad.pic\""), "{e}");
        assert!(e.contains("\"line\":1"), "{e}");
    }

    #[test]
    fn include_policy_sandboxes_and_denies() {
        // layout: root/outside.pic (outside the fence), root/base/inc.pic (in)
        let root = std::env::temp_dir().join(format!("rpic_inc_policy_{}", std::process::id()));
        let base = root.join("base");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(root.join("outside.pic"), "circle\n").unwrap();
        std::fs::write(base.join("inc.pic"), "box wid 0.5 ht 0.5\n").unwrap();

        let sandboxed = CompileOptions {
            base: Some(base.clone()),
            includes: IncludePolicy::SandboxedToBase,
            ..Default::default()
        };
        // in-base include works
        assert!(compile_with_options("copy \"inc.pic\"\nbox", &sandboxed).is_ok());
        // `..` escape is denied, with the structured kind and the copy's span
        let esc = compile_json_with_options("copy \"../outside.pic\"\nbox", &sandboxed);
        assert!(esc.contains("\"kind\":\"include_denied\""), "{esc}");
        assert!(esc.contains("outside the include base directory"), "{esc}");
        assert!(esc.contains("\"line\":1"), "{esc}");
        // absolute paths are denied outright
        let abs_src = format!("copy \"{}\"\nbox", root.join("outside.pic").display());
        let abs = compile_json_with_options(&abs_src, &sandboxed);
        assert!(abs.contains("\"kind\":\"include_denied\""), "{abs}");
        assert!(abs.contains("absolute paths are not allowed"), "{abs}");
        // symlink pointing out of the fence is caught by canonicalization
        #[cfg(unix)]
        {
            let link = base.join("link.pic");
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(root.join("outside.pic"), &link).unwrap();
            let sym = compile_json_with_options("copy \"link.pic\"\nbox", &sandboxed);
            assert!(sym.contains("\"kind\":\"include_denied\""), "{sym}");
        }
        // the embedded library is not a filesystem include — always available
        assert!(
            compile_with_options(
                "copy \"circuits\"\nA:(0,0); B:(1,0)\nresistor(A,B)",
                &sandboxed
            )
            .is_ok()
        );

        let deny = CompileOptions {
            base: Some(base.clone()),
            includes: IncludePolicy::Deny,
            ..Default::default()
        };
        let d = compile_json_with_options("copy \"inc.pic\"\nbox", &deny);
        assert!(d.contains("\"kind\":\"include_denied\""), "{d}");
        assert!(d.contains("disabled by the include policy"), "{d}");
        assert!(compile_with_options("copy \"circuits\"\nbox", &deny).is_ok());

        // default stays the CLI behavior: `..` resolution is allowed
        let open = CompileOptions {
            base: Some(base.clone()),
            ..Default::default()
        };
        assert!(compile_with_options("copy \"../outside.pic\"\nbox", &open).is_ok());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn json_in_dir_resolves_copy_includes() {
        let dir = std::env::temp_dir().join(format!("rpic_json_copy_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("inc.pic"), "box wid 0.5 ht 0.5\n").unwrap();

        let j = compile_json_in_dir("copy \"inc.pic\"\ncircle", Some(dir.as_path()));
        let _ = std::fs::remove_dir_all(&dir);

        assert!(j.starts_with("{\"svg\":\"<svg"), "{j}");
        assert!(j.contains("<rect"), "{j}");
        assert!(j.contains("<circle"), "{j}");
        assert!(j.contains("\"diagnostics\":[]"), "{j}");
        assert!(!j.contains("\"error\""), "{j}");
    }

    #[test]
    fn json_error_info_uses_user_facing_tokens_and_spans() {
        let j = compile_json("bxo\n");

        assert!(
            j.contains("\"error\":\"1:1: expected an object, found `bxo`\""),
            "{j}"
        );
        assert!(j.contains("\"kind\":\"expected_token\""), "{j}");
        assert!(j.contains("\"line\":1"), "{j}");
        assert!(j.contains("\"col\":1"), "{j}");
        assert!(j.contains("\"end_col\":4"), "{j}");
        assert!(j.contains("\"found\":\"`bxo`\""), "{j}");
        assert!(j.contains("\"expected\":\"an object\""), "{j}");
        assert!(j.contains("\"hint\":\"did you mean `box`?\""), "{j}");
    }

    #[test]
    fn json_unterminated_string_points_at_opening_quote() {
        let j = compile_json("box wid 1\n  \"oops\n");

        assert!(j.contains("\"kind\":\"unterminated_string\""), "{j}");
        assert!(j.contains("\"line\":2"), "{j}");
        assert!(j.contains("\"col\":3"), "{j}");
    }

    #[test]
    fn json_eval_errors_get_structured_locations_when_possible() {
        let label = compile_json("box at A\n");
        assert!(label.contains("\"kind\":\"unknown_label\""), "{label}");
        assert!(label.contains("\"line\":1"), "{label}");
        assert!(label.contains("\"col\":8"), "{label}");
        assert!(label.contains("\"found\":\"A\""), "{label}");

        let ordinal = compile_json("box\nbox at 3rd box\n");
        assert!(
            ordinal.contains("\"kind\":\"ordinal_out_of_range\""),
            "{ordinal}"
        );
        assert!(ordinal.contains("\"line\":2"), "{ordinal}");
        assert!(ordinal.contains("\"col\":8"), "{ordinal}");
        assert!(ordinal.contains("\"found\":\"3\""), "{ordinal}");
        assert!(ordinal.contains("\"expected\":\"1..1\""), "{ordinal}");
    }

    #[test]
    fn json_eval_error_inside_include_names_the_file() {
        // #197: the failing reference's own tokens carry the span (and its
        // include provenance) — no re-lexing of the user source involved.
        let dir = std::env::temp_dir().join(format!("rpic_eval_incl_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("inc-eval.pic"), "# comment\nbox at Missing\n").unwrap();
        let j = compile_json_in_dir("circle\ncopy \"inc-eval.pic\"", Some(dir.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(j.contains("\"error\":\"unknown label `Missing`\""), "{j}");
        assert!(j.contains("\"kind\":\"unknown_label\""), "{j}");
        assert!(j.contains("\"file\":\"inc-eval.pic\""), "{j}");
        assert!(j.contains("\"line\":2"), "{j}"); // include-relative
        assert!(j.contains("\"col\":8"), "{j}");
    }

    #[test]
    fn json_deferred_parse_errors_keep_their_structure() {
        // #197: a parse error inside a dynamically-executed body used to be
        // flattened to kind "eval" with null position/found/hint.
        let j = compile_json("x = 1\nif x then { bxo }\n");
        assert!(j.contains("\"kind\":\"expected_token\""), "{j}");
        assert!(j.contains("\"line\":2"), "{j}");
        assert!(j.contains("\"col\":13"), "{j}");
        assert!(j.contains("\"found\":\"`bxo`\""), "{j}");
        assert!(j.contains("\"hint\":\"did you mean `box`?\""), "{j}");

        let f = compile_json("for i = 1 to 2 do { bxo }\n");
        assert!(f.contains("\"kind\":\"expected_token\""), "{f}");
        assert!(f.contains("\"hint\":\"did you mean `box`?\""), "{f}");
    }

    #[test]
    fn json_success_bundle_reports_warnings() {
        let attr = compile_json("box \"a\" dashd\n");
        assert!(attr.contains("\"warnings\":[{"), "{attr}");
        assert!(attr.contains("\"kind\":\"ignored_attribute\""), "{attr}");
        assert!(attr.contains("\"found\":\"dashd\""), "{attr}");
        assert!(
            attr.contains("\"hint\":\"did you mean `dashed`?\""),
            "{attr}"
        );

        let anim = compile_json("box\nanimate 1st box with \"zoom\"\n");
        assert!(
            anim.contains("\"kind\":\"unknown_animation_effect\""),
            "{anim}"
        );
        assert!(anim.contains("\"found\":\"zoom\""), "{anim}");
        assert!(anim.contains("\"line\":2"), "{anim}");
    }

    #[test]
    fn circuits_library_compiles_and_draws() {
        let src = format!(
            "{}\nA:(0,0); B:(1.6,0); C:(3,0)\nresistor(A,B)\ncapacitor(A,B)\ninductor(A,B)\n\
             diode(A,B)\nbattery(A,B)\nground(A)\ndot(A)\nwire(A,B)\n\
             and_gate(C)\nor_gate(C)\nnand_gate(C)\nnor_gate(C)\nxor_gate(C)\n\
             xnor_gate(C)\nbuffer(C)\nnot_gate(C)\n\
             opamp(C)\nnpn(C)\npnp(C)\nac_source(A,B)\nswitch(A,B)\n\
             potentiometer(A,B)\ntransformer(C)\n\
             nmos(C)\npmos(C)\ncurrent_source(A,B)\nvsource_ctrl(A,B)\n\
             isource_ctrl(A,B)\nled(A,B)\nphotodiode(A,B)\nzener(A,B)\n\
             clabel(A,\"x\")\nterminal(A)\nvdd(A)\nantenna(A)\ncurrent(A,B)\n\
             voltage(A,B)\nlamp(A,B)\nfuse(A,B)\n\
             voltmeter(A,B)\nammeter(A,B)\nohmmeter(A,B)\nmeter(A,B,\"X\")\n\
             crystal(A,B)\nspeaker(A,B)\nhop(A,B)\nspdt(A,B,C)\n\
             iec_resistor(A,B)\nvoltage_source(A,B)\npushbutton(A,B)\nrelay(A,B)\n\
             thermistor(A,B)\nmotor(A,B)\ngenerator(A,B)\nbell(A,B)\n\
             ieee_and(C)\nieee_or(C)\nieee_xor(C)\nieee_buf(C)\niecgate(C,\"x\")\n\
             varistor(A,B)\ntline(A,B)\nbus(A,B)\n\
             polcap(A,B)\nvarcap(A,B)\nvarind(A,B)\nschottky(A,B)\nldr(A,B)\n\
             spark_gap(A,B)\nchassis_ground(A)\nsignal_ground(A)\n\
             njfet(C)\npjfet(C)\nphototransistor(C)\nic_block(C,\"x\")\nmux(C)\n\
             solar_cell(A,B)\nmicrophone(A,B)\nthermocouple(A,B)",
            CIRCUITS
        );
        let d = compile(&src).expect("circuit library should compile");
        assert!(!d.shapes.is_empty());
    }
}
