//! `rpic` command-line front-end.
//!
//! Renders pic source to SVG (default), PNG, or PDF, and can dump the token
//! stream or syntax tree for debugging.

use std::io::Write;
use std::process::ExitCode;

mod html;

enum Mode {
    Svg,
    Png,
    Pdf,
    Ast,
    Tokens,
    Json,
    /// `--json=all`: the compile bundle plus the `scene` key.
    JsonAll,
    /// `--scene`: the scene document alone (docs/scene.md).
    Scene,
    Html,
}

fn main() -> ExitCode {
    // rpic `texlabels` extension: wire the RaTeX math renderer into the core.
    rpic_core::set_math_renderer(rpic_render::math::render_math);
    let args: Vec<String> = std::env::args().collect();
    let mut path: Option<String> = None;
    let mut out: Option<String> = None;
    let mut scale: f32 = 1.0;
    let mut mode = Mode::Svg;
    let mut circuits = false;
    let mut texlabels = false;
    let mut flatten: Option<f64> = None;
    let mut outline = false;

    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-c" | "--circuits" => circuits = true,
            "-t" | "--texlabels" => texlabels = true,
            "--svg" => mode = Mode::Svg,
            "--png" => mode = Mode::Png,
            "--pdf" => mode = Mode::Pdf,
            "--ast" => mode = Mode::Ast,
            "--tokens" => mode = Mode::Tokens,
            "--json" => mode = Mode::Json,
            "--json=all" => mode = Mode::JsonAll,
            "--scene" => mode = Mode::Scene,
            "--outline" => outline = true,
            "--flatten" => {
                i += 1;
                match args.get(i).and_then(|v| v.parse::<f64>().ok()) {
                    Some(v) if v > 0.0 && v.is_finite() => flatten = Some(v),
                    _ => {
                        eprintln!("rpic: `--flatten` needs a positive tolerance in inches");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--html" => mode = Mode::Html,
            "-o" | "--output" => {
                i += 1;
                match args.get(i) {
                    Some(v) => out = Some(v.clone()),
                    None => {
                        eprintln!("rpic: `-o` needs a file argument");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--scale" => {
                i += 1;
                match args.get(i).and_then(|v| v.parse::<f32>().ok()) {
                    Some(v) if v > 0.0 => scale = v,
                    _ => {
                        eprintln!("rpic: `--scale` needs a positive number");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            s if s.starts_with('-') => {
                eprintln!("rpic: unknown option `{s}`");
                return ExitCode::FAILURE;
            }
            s => {
                if let Err(e) = set_input_path(&mut path, s) {
                    eprintln!("rpic: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        i += 1;
    }

    let Some(path) = path else {
        print_help();
        return ExitCode::FAILURE;
    };

    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("rpic: cannot read `{path}`: {e}");
            return ExitCode::FAILURE;
        }
    };

    // `-c`/`-t` become compile options (not text prepended to the source), so
    // diagnostic positions stay relative to the user's own file.
    let opts = rpic_core::CompileOptions {
        circuits,
        texlabels,
        base: std::path::Path::new(&path)
            .parent()
            .map(|p| p.to_path_buf()),
        // the CLI keeps full filesystem access for includes
        includes: rpic_core::IncludePolicy::Unrestricted,
        ..Default::default()
    };
    // the page title for `--html`: the input file's stem
    let title = std::path::Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "rpic".to_string());
    let sopts = rpic_core::SceneOptions {
        flatten,
        outline,
        source: std::path::Path::new(&path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned()),
    };
    let result = run(&src, &mode, scale, &opts, &sopts, &title);
    match result {
        Ok(Output::Text(s)) => {
            if let Some(o) = out {
                if let Err(e) = std::fs::write(&o, s) {
                    eprintln!("rpic: cannot write `{o}`: {e}");
                    return ExitCode::FAILURE;
                }
            } else {
                print!("{s}");
            }
            ExitCode::SUCCESS
        }
        Ok(Output::Bytes(b)) => {
            if let Some(o) = out {
                if let Err(e) = std::fs::write(&o, b) {
                    eprintln!("rpic: cannot write `{o}`: {e}");
                    return ExitCode::FAILURE;
                }
            } else if let Err(e) = std::io::stdout().write_all(&b) {
                eprintln!("rpic: cannot write stdout: {e}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rpic: {path}: {e}");
            ExitCode::FAILURE
        }
    }
}

fn set_input_path(path: &mut Option<String>, value: &str) -> Result<(), String> {
    if let Some(prev) = path.as_deref() {
        return Err(format!(
            "multiple input files are not supported (`{prev}` and `{value}`)"
        ));
    }
    *path = Some(value.to_string());
    Ok(())
}

enum Output {
    Text(String),
    Bytes(Vec<u8>),
}

fn run(
    src: &str,
    mode: &Mode,
    scale: f32,
    opts: &rpic_core::CompileOptions,
    sopts: &rpic_core::SceneOptions,
    title: &str,
) -> Result<Output, String> {
    match mode {
        Mode::Tokens => {
            // The pre-expansion dump covers the user's source only (no
            // prelude tokens) — positions match the file being debugged.
            let toks = rpic_core::lex(src).map_err(|e| e.to_string())?;
            let mut s = String::new();
            for t in &toks {
                s.push_str(&format!("{:>3}:{:<3} {:?}\n", t.line, t.col, t.tok));
            }
            Ok(Output::Text(s))
        }
        Mode::Ast => {
            let pic = rpic_core::parse_with_prelude(
                src,
                rpic_core::IncludeCtx::with_policy(opts.base.clone(), opts.includes),
                opts.circuits,
                opts.texlabels,
            )
            .map_err(|e| e.to_string())?;
            Ok(Output::Text(format!("{pic:#?}\n")))
        }
        Mode::Svg => {
            let d = rpic_core::compile_with_options(src, opts)?;
            emit_diagnostics(&d);
            Ok(Output::Text(rpic_core::to_svg(&d)))
        }
        Mode::Json => Ok(Output::Text(rpic_core::compile_json_with_options(
            src, opts,
        ))),
        Mode::JsonAll => Ok(Output::Text(rpic_core::compile_json_all_with_options(
            src, opts, sopts,
        ))),
        Mode::Scene => {
            let d = rpic_core::compile_with_options(src, opts)?;
            emit_diagnostics(&d);
            let out = rpic_core::scene_json(&d, sopts);
            for w in &out.warnings {
                eprintln!("warning: {}", w.message);
            }
            Ok(Output::Text(out.json))
        }
        Mode::Html => {
            let d = rpic_core::compile_with_options(src, opts)?;
            emit_diagnostics(&d);
            Ok(Output::Text(html::to_html(&d, title)))
        }
        Mode::Png => {
            let d = rpic_core::compile_with_options(src, opts)?;
            emit_diagnostics(&d);
            let svg = rpic_core::to_svg(&d);
            Ok(Output::Bytes(rpic_render::to_png(&svg, scale)?))
        }
        Mode::Pdf => {
            let d = rpic_core::compile_with_options(src, opts)?;
            emit_diagnostics(&d);
            let svg = rpic_core::to_svg(&d);
            Ok(Output::Bytes(rpic_render::to_pdf(&svg)?))
        }
    }
}

fn emit_diagnostics(d: &rpic_core::Drawing) {
    for line in &d.diagnostics {
        eprintln!("{line}");
    }
}

fn print_help() {
    eprintln!(
        "rpic — pic graphics language → SVG / PNG / PDF\n\n\
         USAGE:\n    rpic [MODE] [-o FILE] [--scale N] <file.pic>\n\n\
         MODES:\n    \
         --svg       render to SVG (default)\n    \
         --png       render to PNG (raster)\n    \
         --pdf       render to PDF\n    \
         --ast       dump the syntax tree\n    \
         --tokens    dump the token stream\n    \
    --json      emit compile JSON (svg, animations, diagnostics, warnings, objects)\n    \
         --json=all  the compile JSON plus the device-independent scene (docs/scene.md)\n    \
         --scene     emit the scene JSON alone (the resolved draw list)\n    \
         --html      self-contained animated page (SVG + GSAP player inline; plugins from CDN)\n\n\
         OPTIONS:\n    \
         -c, --circuits      load the native circuit-element library (in-source: copy \"circuits\")\n    \
     -t, --texlabels     typeset $…$ labels as TeX math (sets texlabels = 1)\n    \
         -o, --output FILE   write to FILE (default: stdout)\n    \
         --scale N           PNG scale factor, 1.0 = 96 dpi (default 1.0)\n    \
         --flatten TOL       scene: deliver curves as polylines within TOL inches\n    \
         --outline           scene: embed typeset math as SVG glyph fragments\n    \
         -h, --help          show this help\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_input_path_accepts_first_input() {
        let mut path = None;

        set_input_path(&mut path, "a.pic").unwrap();

        assert_eq!(path.as_deref(), Some("a.pic"));
    }

    #[test]
    fn set_input_path_rejects_second_input() {
        let mut path = Some("a.pic".to_string());

        let err = set_input_path(&mut path, "b.pic").unwrap_err();

        assert!(err.contains("multiple input files are not supported"));
        assert!(err.contains("a.pic"));
        assert!(err.contains("b.pic"));
        assert_eq!(path.as_deref(), Some("a.pic"));
    }
}
