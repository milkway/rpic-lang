//! `rpic` command-line front-end.
//!
//! Renders pic source to SVG (default), PNG, or PDF, and can dump the token
//! stream or syntax tree for debugging.

use std::io::Write;
use std::process::ExitCode;

enum Mode {
    Svg,
    Png,
    Pdf,
    Ast,
    Tokens,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut path: Option<String> = None;
    let mut out: Option<String> = None;
    let mut scale: f32 = 1.0;
    let mut mode = Mode::Svg;
    let mut circuits = false;

    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-c" | "--circuits" => circuits = true,
            "--svg" => mode = Mode::Svg,
            "--png" => mode = Mode::Png,
            "--pdf" => mode = Mode::Pdf,
            "--ast" => mode = Mode::Ast,
            "--tokens" => mode = Mode::Tokens,
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
            s => path = Some(s.to_string()),
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

    let src = if circuits {
        format!("{}\n{}", rpic_core::CIRCUITS, src)
    } else {
        src
    };

    let result = run(&src, &mode, scale);
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

enum Output {
    Text(String),
    Bytes(Vec<u8>),
}

fn run(src: &str, mode: &Mode, scale: f32) -> Result<Output, String> {
    match mode {
        Mode::Tokens => {
            let toks = rpic_core::lex(src).map_err(|e| e.to_string())?;
            let mut s = String::new();
            for t in &toks {
                s.push_str(&format!("{:>3}:{:<3} {:?}\n", t.line, t.col, t.tok));
            }
            Ok(Output::Text(s))
        }
        Mode::Ast => {
            let pic = rpic_core::parse(src).map_err(|e| e.to_string())?;
            Ok(Output::Text(format!("{pic:#?}\n")))
        }
        Mode::Svg => Ok(Output::Text(rpic_core::render_svg(src)?)),
        Mode::Png => {
            let svg = rpic_core::render_svg(src)?;
            Ok(Output::Bytes(rpic_render::to_png(&svg, scale)?))
        }
        Mode::Pdf => {
            let svg = rpic_core::render_svg(src)?;
            Ok(Output::Bytes(rpic_render::to_pdf(&svg)?))
        }
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
         --tokens    dump the token stream\n\n\
         OPTIONS:\n    \
         -c, --circuits      load the native circuit-element library\n    \
         -o, --output FILE   write to FILE (default: stdout)\n    \
         --scale N           PNG scale factor, 1.0 = 96 dpi (default 1.0)\n    \
         -h, --help          show this help\n"
    );
}
