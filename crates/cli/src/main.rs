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
    /// `--geojson`: the scene georeferenced as a GeoJSON FeatureCollection
    /// (docs/geopic.md), via geopic-core. With `maplibre` (`--maplibre`):
    /// `{"geojson": …, "layers": […]}`, the MapLibre layers that draw it,
    /// built with these options (`--text-scale`).
    GeoJson {
        maplibre: Option<geopic_core::maplibre::LayerOptions>,
    },
    Html,
}

/// Parse and validate `LON,LAT` (degrees), naming the flag on failure.
fn parse_lonlat(flag: &str, v: &str) -> Result<geopic_core::LonLat, String> {
    let bad = || format!("rpic: `{flag}` needs LON,LAT in degrees, got `{v}`");
    let (lon, lat) = v.split_once(',').ok_or_else(bad)?;
    let lon = lon.trim().parse::<f64>().map_err(|_| bad())?;
    let lat = lat.trim().parse::<f64>().map_err(|_| bad())?;
    geopic_core::LonLat::checked(lon, lat).map_err(|e| format!("rpic: `{flag}`: {e}"))
}

/// Parse `NAME=LON,LAT` (a host feature for `--geo-feature`).
fn parse_feature(v: &str) -> Result<geopic_core::NamedFeature, String> {
    let (name, pos) = v
        .split_once('=')
        .filter(|(n, _)| !n.trim().is_empty())
        .ok_or_else(|| format!("rpic: `--geo-feature` needs NAME=LON,LAT, got `{v}`"))?;
    let ll = parse_lonlat("--geo-feature", pos)?;
    Ok(geopic_core::NamedFeature {
        name: name.trim().to_string(),
        lon: ll.lon,
        lat: ll.lat,
        bbox: None,
    })
}

/// The value after a flag, as a finite number (positive when `positive`);
/// `what` names the unit in the error.
fn number_arg(
    args: &[String],
    i: &mut usize,
    flag: &str,
    what: &str,
    positive: bool,
) -> Result<f64, String> {
    *i += 1;
    match args.get(*i).and_then(|v| v.parse::<f64>().ok()) {
        Some(v) if v.is_finite() && (!positive || v > 0.0) => Ok(v),
        _ => Err(format!(
            "rpic: `{flag}` needs a {}{what}",
            if positive { "positive " } else { "" }
        )),
    }
}

/// The value after a flag, as a string.
fn string_arg(args: &[String], i: &mut usize, flag: &str, what: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("rpic: `{flag}` needs {what}"))
}

/// One stderr format for every non-fatal diagnostic: `warning: <message>
/// (<kind>[, <item id>])`.
fn emit_warning(kind: &str, id: Option<&str>, message: &str) {
    match id {
        Some(id) => eprintln!("warning: {message} ({kind}, {id})"),
        None => eprintln!("warning: {message} ({kind})"),
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
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
    let mut maplibre = false;
    let mut text_scale: Option<f64> = None;
    let mut geo = geopic_core::GeoOptions::default();
    // `--prelude NAME=FILE` args, read once the input file is known
    let mut prelude_args: Vec<String> = Vec::new();

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
            "--geojson" => mode = Mode::GeoJson { maplibre: None },
            "--maplibre" => maplibre = true,
            "--text-scale" => match number_arg(&args, &mut i, a, "font-size factor", true) {
                Ok(v) => text_scale = Some(v),
                Err(e) => return fail(&e),
            },
            "--geo-anchor" => match args.get(i + 1).map(|v| parse_lonlat(a, v)) {
                Some(Ok(ll)) => {
                    i += 1;
                    geo.anchor = Some(ll);
                }
                Some(Err(e)) => return fail(&e),
                None => return fail(&format!("rpic: `{a}` needs LON,LAT in degrees")),
            },
            "--geo-scale" => {
                match number_arg(&args, &mut i, a, "number of metres per inch", true) {
                    Ok(v) => geo.scale = Some(v),
                    Err(e) => return fail(&e),
                }
            }
            "--geo-bearing" => match number_arg(&args, &mut i, a, "angle in degrees", false) {
                Ok(v) => geo.bearing = Some(v),
                Err(e) => return fail(&e),
            },
            "--geo-frame" => {
                match string_arg(&args, &mut i, a, "`enu` or `mercator`").and_then(|v| {
                    v.parse::<geopic_core::Frame>()
                        .map_err(|e| format!("rpic: `{a}`: {e}"))
                }) {
                    Ok(f) => geo.frame = Some(f),
                    Err(e) => return fail(&e),
                }
            }
            "--geo-view-scale" => {
                match number_arg(&args, &mut i, a, "number of pixels per inch", true) {
                    Ok(v) => geo.view_scale = Some(v),
                    Err(e) => return fail(&e),
                }
            }
            "--geo-feature" => match args.get(i + 1).map(|v| parse_feature(v)) {
                Some(Ok(f)) => {
                    i += 1;
                    geo.features.push(f);
                }
                Some(Err(e)) => return fail(&e),
                None => return fail(&format!("rpic: `{a}` needs NAME=LON,LAT")),
            },
            "--prelude" => match string_arg(&args, &mut i, a, "NAME=FILE") {
                Ok(v) => prelude_args.push(v),
                Err(e) => return fail(&e),
            },
            "--outline" => outline = true,
            "--flatten" => match number_arg(&args, &mut i, a, "tolerance in inches", true) {
                Ok(v) => flatten = Some(v),
                Err(e) => return fail(&e),
            },
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
            "-V" | "--version" => {
                println!("rpic {}", env!("CARGO_PKG_VERSION"));
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

    if text_scale.is_some() && !maplibre {
        return fail("rpic: `--text-scale` only applies to `--geojson --maplibre`");
    }
    if maplibre {
        let mut layer_opts = geopic_core::maplibre::LayerOptions::default();
        if let Some(k) = text_scale {
            layer_opts = layer_opts.with_text_scale(k);
        }
        if let Err(e) = layer_opts.validate() {
            return fail(&format!("rpic: `--text-scale`: {e}"));
        }
        match &mut mode {
            Mode::GeoJson { maplibre: m } => *m = Some(layer_opts),
            _ => {
                return fail(
                    "rpic: `--maplibre` only applies to `--geojson` (another output mode was selected; the last mode flag wins)",
                );
            }
        }
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

    // relative prelude files resolve against the input's directory, like
    // `copy "file"` (an input with no directory part: the working directory)
    let input_dir = std::path::Path::new(&path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty());
    let mut preludes = Vec::with_capacity(prelude_args.len());
    for arg in &prelude_args {
        match read_prelude(arg, input_dir) {
            Ok(p) => preludes.push(p),
            Err(e) => return fail(&e),
        }
    }

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
        // `--prelude NAME=FILE` units, in command-line order
        preludes,
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
    let result = run(&src, &mode, scale, &opts, &sopts, &geo, &title);
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
    geo: &geopic_core::GeoOptions,
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
            let pic = rpic_core::parse_with_options(src, opts).map_err(|e| e.message)?;
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
                emit_warning(&w.kind, None, &w.message);
            }
            Ok(Output::Text(out.json))
        }
        Mode::GeoJson { maplibre } => {
            let d = rpic_core::compile_with_options(src, opts)?;
            emit_diagnostics(&d);
            let scene = rpic_core::scene_json(&d, sopts);
            for w in &scene.warnings {
                emit_warning(&w.kind, None, &w.message);
            }
            let mut geo = geo.clone();
            if let Some(f) = sopts.flatten {
                geo.flatten = f;
            }
            let mut g = geopic_core::georeference(&scene.json, &geo).map_err(|e| e.to_string())?;
            if !d.anims.is_empty() {
                g.warnings
                    .push(geopic_core::Warning::animations_ignored(d.anims.len()));
            }
            for w in &g.warnings {
                emit_warning(&w.kind, w.id.as_deref(), &w.message);
            }
            if let Some(layer_opts) = maplibre {
                return geopic_core::maplibre::bundle_json_with(&g.features, layer_opts)
                    .map(Output::Text)
                    .map_err(|e| e.to_string());
            }
            Ok(Output::Text(g.features.to_json()))
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
         --geojson   georeference the drawing: GeoJSON FeatureCollection, WGS84 (docs/geopic.md)\n    \
         --html      self-contained animated page (SVG + GSAP player inline; plugins from CDN)\n\n\
         OPTIONS:\n    \
         -c, --circuits      load the native circuit-element library (in-source: copy \"circuits\")\n    \
     -t, --texlabels     typeset $…$ labels as TeX math (sets texlabels = 1)\n    \
         --prelude NAME=FILE load FILE as a named unit before the input (repeatable; positions in it report NAME;\n    \
         \x20                   a relative FILE resolves against the input file's directory, like `copy`)\n    \
         -o, --output FILE   write to FILE (default: stdout)\n    \
         --scale N           PNG scale factor, 1.0 = 96 dpi (default 1.0)\n    \
         --flatten TOL       scene: deliver curves as polylines within TOL inches\n    \
         --outline           scene: embed typeset math as SVG glyph fragments\n    \
         --geo-anchor LON,LAT    geojson: position of the pic origin (overrides geo_lon/geo_lat)\n    \
         --geo-scale M           geojson: metres per authored inch (overrides geo_scale)\n    \
         --geo-bearing DEG       geojson: azimuth of +y, clockwise from north (overrides geo_bearing)\n    \
         --geo-frame F           geojson: `enu` (default) or `mercator`\n    \
         --geo-view-scale PX     geojson: pixels per inch when the source sets geo_px = 1\n    \
         --geo-feature N=LON,LAT geojson: a named feature (control point for the label N); repeatable\n    \
         --maplibre              geojson: emit {{\"geojson\": …, \"layers\": […]}} with the MapLibre layers\n    \
         --text-scale K          maplibre: multiply every label's font size by K (0 < K <= 16; default 1)\n    \
         -h, --help          show this help\n    \
         -V, --version       print the version (`rpic X.Y.Z`) and exit\n"
    );
}

/// `--prelude NAME=FILE`: the named unit, its text read here (the user named
/// the file, so no include policy applies). A relative FILE resolves against
/// `dir` (the input file's directory) when given, else the working
/// directory. Name validation is the core's.
fn read_prelude(arg: &str, dir: Option<&std::path::Path>) -> Result<rpic_core::Prelude, String> {
    let Some((name, file)) = arg.split_once('=') else {
        return Err(format!("rpic: `--prelude` needs NAME=FILE (got `{arg}`)"));
    };
    if file.is_empty() {
        return Err(format!("rpic: `--prelude {arg}`: missing FILE"));
    }
    let path = match dir {
        Some(d) if std::path::Path::new(file).is_relative() => d.join(file),
        _ => std::path::PathBuf::from(file),
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("rpic: cannot read prelude `{}`: {e}", path.display()))?;
    Ok(rpic_core::Prelude::new(name, text))
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
