//! Python bindings for rpic (PyO3). Exposes the pic → SVG/PNG/PDF engine and
//! the animation manifest. Build with maturin.

use pyo3::create_exception;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::path::PathBuf;

create_exception!(
    rpic,
    CompileError,
    pyo3::exceptions::PyValueError,
    "A pic compile error. `str(exc)` is the readable message; `exc.info` is a \
     dict with the structured diagnostic: message, line, col, end_col, file, \
     kind, found, expected, hint (file is None for your own source, or names \
     the `copy` include / library the position is relative to)."
);

/// Compile options (not text prepended to the source), so diagnostic
/// positions stay relative to the caller's own `src`. `texlabels` is an
/// initializer only — the source can still override with `texlabels = 0`.
/// `include_policy` governs `copy "file"` filesystem includes:
/// "unrestricted" (default, the CLI behavior), "sandboxed" (only files
/// inside `base`; absolute paths and `..`/symlink escapes are errors) or
/// "deny" (no filesystem includes). `copy "circuits"` always works.
fn opts(
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
    max_loop_iterations: Option<u64>,
    max_shapes: Option<usize>,
) -> PyResult<rpic_core::CompileOptions> {
    let includes = match include_policy.unwrap_or("unrestricted") {
        "unrestricted" => rpic_core::IncludePolicy::Unrestricted,
        "sandboxed" => rpic_core::IncludePolicy::SandboxedToBase,
        "deny" => rpic_core::IncludePolicy::Deny,
        other => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "include_policy must be 'unrestricted', 'sandboxed' or 'deny' (got '{other}')"
            )));
        }
    };
    Ok(rpic_core::CompileOptions {
        circuits,
        texlabels,
        base,
        includes,
        max_loop_iterations,
        max_shapes,
        ..Default::default()
    })
}

fn diagnostic_dict<'py>(
    py: Python<'py>,
    d: &rpic_core::Diagnostic,
) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    out.set_item("message", &d.message)?;
    out.set_item("line", d.line)?;
    out.set_item("col", d.col)?;
    out.set_item("end_col", d.end_col)?;
    out.set_item("file", d.file.as_deref())?;
    out.set_item("kind", &d.kind)?;
    out.set_item("found", d.found.as_deref())?;
    out.set_item("expected", d.expected.as_deref())?;
    out.set_item("hint", d.hint.as_deref())?;
    Ok(out)
}

/// Raise [`CompileError`] carrying the structured diagnostic as `exc.info`.
fn err(py: Python<'_>, e: rpic_core::CompileError) -> PyErr {
    let exc = CompileError::new_err(e.message.clone());
    if let Ok(info) = diagnostic_dict(py, &e.info) {
        let _ = exc.value(py).setattr("info", info);
    }
    exc
}

#[allow(clippy::too_many_arguments)]
fn compile_drawing(
    py: Python<'_>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
    max_loop_iterations: Option<u64>,
    max_shapes: Option<usize>,
) -> PyResult<rpic_core::Drawing> {
    let opts = opts(
        circuits,
        texlabels,
        base,
        include_policy,
        max_loop_iterations,
        max_shapes,
    )?;
    rpic_core::compile_with_diagnostics(src, &opts).map_err(|e| err(py, e))
}

/// Render pic source to an SVG string.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None, include_policy = None, max_loop_iterations = None, max_shapes = None))]
#[allow(clippy::too_many_arguments)]
fn render_svg(
    py: Python<'_>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
    max_loop_iterations: Option<u64>,
    max_shapes: Option<usize>,
) -> PyResult<String> {
    Ok(rpic_core::to_svg(&compile_drawing(
        py,
        src,
        circuits,
        texlabels,
        base,
        include_policy,
        max_loop_iterations,
        max_shapes,
    )?))
}

/// Render pic source to PNG bytes (scale 1.0 = 96 dpi).
#[pyfunction]
#[pyo3(signature = (src, scale = 1.0, circuits = false, texlabels = false, base = None, include_policy = None, max_loop_iterations = None, max_shapes = None))]
#[allow(clippy::too_many_arguments)]
fn render_png<'py>(
    py: Python<'py>,
    src: &str,
    scale: f32,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
    max_loop_iterations: Option<u64>,
    max_shapes: Option<usize>,
) -> PyResult<Bound<'py, PyBytes>> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "scale must be a positive finite number",
        ));
    }
    let svg = rpic_core::to_svg(&compile_drawing(
        py,
        src,
        circuits,
        texlabels,
        base,
        include_policy,
        max_loop_iterations,
        max_shapes,
    )?);
    let png = rpic_render::to_png(&svg, scale).map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(PyBytes::new(py, &png))
}

/// Render pic source to PDF bytes.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None, include_policy = None, max_loop_iterations = None, max_shapes = None))]
#[allow(clippy::too_many_arguments)]
fn render_pdf<'py>(
    py: Python<'py>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
    max_loop_iterations: Option<u64>,
    max_shapes: Option<usize>,
) -> PyResult<Bound<'py, PyBytes>> {
    let svg = rpic_core::to_svg(&compile_drawing(
        py,
        src,
        circuits,
        texlabels,
        base,
        include_policy,
        max_loop_iterations,
        max_shapes,
    )?);
    let pdf = rpic_render::to_pdf(&svg).map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(PyBytes::new(py, &pdf))
}

/// Build [`rpic_core::SceneOptions`] from the Python keywords, validating
/// `flatten`.
fn scene_opts(flatten: Option<f64>, outline: bool) -> PyResult<rpic_core::SceneOptions> {
    if let Some(f) = flatten
        && (!f.is_finite() || f <= 0.0)
    {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "flatten must be a positive finite tolerance in inches",
        ));
    }
    Ok(rpic_core::SceneOptions {
        flatten,
        outline,
        source: None,
    })
}

/// Parse a JSON document into Python objects via the stdlib `json` module.
fn json_loads<'py>(py: Python<'py>, json: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("json")?.call_method1("loads", (json,))
}

/// Compile pic source to the device-independent **scene** draw list
/// (docs/scene.md) as a parsed dict: versioned, inches, y-up, curves as cubic
/// Béziers (`flatten` delivers polylines instead; `outline` embeds typeset
/// math as SVG fragments). Raises `CompileError` on a pic error. The scene's
/// own `scene_unsupported` warnings are emitted via `warnings.warn`;
/// `compile(..., scene=True)["warnings"]` carries them structured.
#[pyfunction]
#[pyo3(signature = (src, flatten = None, outline = false, circuits = false, texlabels = false, base = None, include_policy = None, max_loop_iterations = None, max_shapes = None))]
#[allow(clippy::too_many_arguments)]
fn scene<'py>(
    py: Python<'py>,
    src: &str,
    flatten: Option<f64>,
    outline: bool,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
    max_loop_iterations: Option<u64>,
    max_shapes: Option<usize>,
) -> PyResult<Bound<'py, PyAny>> {
    let d = compile_drawing(
        py,
        src,
        circuits,
        texlabels,
        base,
        include_policy,
        max_loop_iterations,
        max_shapes,
    )?;
    let out = rpic_core::scene_json(&d, &scene_opts(flatten, outline)?);
    // "never silent": surface the scene's own warnings through Python's
    // warnings machinery (compile(..., scene=True) carries them structured)
    if !out.warnings.is_empty() {
        let warnings_mod = py.import("warnings")?;
        for w in &out.warnings {
            warnings_mod.call_method1("warn", (format!("rpic scene: {}", w.message),))?;
        }
    }
    json_loads(py, &out.json)
}

/// Compile to the parsed bundle: a dict `{"svg": str, "animations": [dict],
/// "diagnostics": [str], "warnings": [dict], "objects": [dict]}` — plus
/// `"places"` / `"passthrough"` when the source defines named places or
/// carries troff lines. Raises `CompileError` (with
/// the structured diagnostic on `exc.info`) on a pic error. With
/// `scene=True` (or a `flatten` tolerance) the dict also carries the
/// device-independent `"scene"` draw list and its `scene_unsupported`
/// warnings join `"warnings"` — the default output is unchanged otherwise.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None, include_policy = None, max_loop_iterations = None, max_shapes = None, scene = false, flatten = None, outline = false))]
#[allow(clippy::too_many_arguments)]
fn compile<'py>(
    py: Python<'py>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
    max_loop_iterations: Option<u64>,
    max_shapes: Option<usize>,
    scene: bool,
    flatten: Option<f64>,
    outline: bool,
) -> PyResult<Bound<'py, PyDict>> {
    let d = compile_drawing(
        py,
        src,
        circuits,
        texlabels,
        base,
        include_policy,
        max_loop_iterations,
        max_shapes,
    )?;
    let scene_out = if scene || flatten.is_some() {
        Some(rpic_core::scene_json(&d, &scene_opts(flatten, outline)?))
    } else {
        None
    };
    let out = PyDict::new(py);
    out.set_item("svg", rpic_core::to_svg(&d))?;
    let anims = PyList::empty(py);
    for a in &d.anims {
        let anim = PyDict::new(py);
        anim.set_item("id", format!("s{}", a.shape))?;
        anim.set_item("effect", &a.effect)?;
        anim.set_item("start", a.start)?;
        anim.set_item("duration", a.duration)?;
        // Optional GSAP overrides — only present when the source set them.
        if a.repeat != 0 {
            anim.set_item("repeat", a.repeat)?;
        }
        if a.yoyo {
            anim.set_item("yoyo", true)?;
        }
        if let Some(ease) = &a.ease {
            anim.set_item("ease", ease)?;
        }
        if let Some(path) = a.path {
            anim.set_item("path", format!("s{path}"))?;
        }
        if let Some(color) = &a.color {
            anim.set_item("color", color)?;
        }
        if a.out {
            anim.set_item("out", true)?;
        }
        if let Some(from) = &a.from {
            anim.set_item("from", from)?;
        }
        if let Some(morph) = a.morph {
            anim.set_item("morph", format!("s{morph}"))?;
        }
        anims.append(anim)?;
    }
    out.set_item("animations", anims)?;
    out.set_item("diagnostics", &d.diagnostics)?;
    let warnings = PyList::empty(py);
    for w in &d.warnings {
        warnings.append(diagnostic_dict(py, w)?)?;
    }
    if let Some(s) = &scene_out {
        for w in &s.warnings {
            warnings.append(diagnostic_dict(py, w)?)?;
        }
    }
    out.set_item("warnings", warnings)?;
    let objects = PyList::empty(py);
    for (i, g) in rpic_core::svg::object_geometries(&d).iter().enumerate() {
        let obj = PyDict::new(py);
        obj.set_item("id", format!("s{i}"))?;
        obj.set_item("kind", g.kind)?;
        match g.bbox {
            Some((x, y, w, h)) => {
                let bbox = PyDict::new(py);
                bbox.set_item("x", x)?;
                bbox.set_item("y", y)?;
                bbox.set_item("w", w)?;
                bbox.set_item("h", h)?;
                obj.set_item("bbox", bbox)?;
            }
            None => obj.set_item("bbox", py.None())?,
        }
        if let Some(span) = d.shape_spans.get(i).and_then(|s| s.as_ref()) {
            obj.set_item("line", span.line)?;
            obj.set_item("col", span.col)?;
            obj.set_item("end_col", span.end_col)?;
            if let Some(f) = &span.file {
                obj.set_item("file", f.as_ref())?;
            }
        }
        objects.append(obj)?;
    }
    out.set_item("objects", objects)?;
    // named places (SVG user units, the `objects` frame) and troff
    // passthrough lines: only when the source has some — parsed from the
    // core's own serializer so the dict matches the CLI bundle by construction
    if !d.places.is_empty() {
        out.set_item("places", json_loads(py, &rpic_core::places_json(&d))?)?;
    }
    if !d.passthrough.is_empty() {
        out.set_item(
            "passthrough",
            json_loads(py, &rpic_core::passthrough_json(&d))?,
        )?;
    }
    // rpic `animate scroll`: timeline-level scroll-scrub hint for the host.
    if d.anim_scroll {
        out.set_item("scroll", true)?;
    }
    if let Some(s) = &scene_out {
        out.set_item("scene", json_loads(py, &s.json)?)?;
    }
    Ok(out)
}

/// Compile to a JSON string `{ "svg": ..., "animations": [...],
/// "diagnostics": [...], "warnings": [...] }`
/// (or `{ "error": ..., "error_info": {...} }`). Parse with `json.loads`;
/// prefer `compile` for an already-parsed dict.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None, include_policy = None, max_loop_iterations = None, max_shapes = None))]
fn compile_json(
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
    max_loop_iterations: Option<u64>,
    max_shapes: Option<usize>,
) -> PyResult<String> {
    let opts = opts(
        circuits,
        texlabels,
        base,
        include_policy,
        max_loop_iterations,
        max_shapes,
    )?;
    Ok(rpic_core::compile_json_with_options(src, &opts))
}

create_exception!(
    rpic,
    GeoError,
    pyo3::exceptions::PyValueError,
    "A georeferencing error (rpic.geo). `str(exc)` is the readable message; \
     `exc.info` is a dict: kind, message, hint."
);

fn geo_err(py: Python<'_>, e: geopic_core::GeoError) -> PyErr {
    let exc = GeoError::new_err(e.to_string());
    if let Ok(info) = json_loads(py, &serde_json::to_string(&e).unwrap_or_default()) {
        let _ = exc.value(py).setattr("info", info);
    }
    exc
}

/// A host feature as Python passes it: `(name, lon, lat, bbox)` with
/// `bbox = (west, south, east, north)` or `None`.
type PyFeature = (String, f64, f64, Option<(f64, f64, f64, f64)>);

/// `features` as `[(name, lon, lat[, (w, s, e, n)]), …]`.
fn geo_features(features: Option<Vec<PyFeature>>) -> Vec<geopic_core::NamedFeature> {
    features
        .unwrap_or_default()
        .into_iter()
        .map(|(name, lon, lat, bbox)| geopic_core::NamedFeature {
            name,
            lon,
            lat,
            bbox: bbox.map(|(w, s, e, n)| [w, s, e, n]),
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn geo_options(
    frame: Option<&str>,
    anchor: Option<(f64, f64)>,
    scale: Option<f64>,
    bearing: Option<f64>,
    features: Option<Vec<PyFeature>>,
    flatten: f64,
    view_scale: Option<f64>,
    screen_classes: Option<Vec<String>>,
) -> PyResult<geopic_core::GeoOptions> {
    let frame = match frame {
        None => None,
        Some(f) => Some(
            f.parse::<geopic_core::Frame>()
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.message))?,
        ),
    };
    let defaults = geopic_core::GeoOptions::default();
    Ok(geopic_core::GeoOptions {
        frame,
        anchor: anchor.map(|(lon, lat)| geopic_core::LonLat { lon, lat }),
        scale,
        bearing,
        features: geo_features(features),
        flatten,
        view_scale,
        screen_classes: screen_classes.unwrap_or(defaults.screen_classes),
    })
}

/// Georeference pic source (rpic.geo.compile's native half): returns
/// `{"geojson": dict, "warnings": [dict], "diagnostics": [str],
/// "registration": dict, "canvas": {"rescale", "scale"}}`. Raises
/// `CompileError` on a pic error and `GeoError` on a registration error.
/// Animations cannot ride into GeoJSON: a source with `animate` statements
/// adds an `ignored` warning naming how many timelines were dropped.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None, include_policy = None, max_loop_iterations = None, max_shapes = None, frame = None, anchor = None, scale = None, bearing = None, features = None, flatten = 1e-3, view_scale = None, screen_classes = None))]
#[allow(clippy::too_many_arguments)]
fn geo_compile<'py>(
    py: Python<'py>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
    max_loop_iterations: Option<u64>,
    max_shapes: Option<usize>,
    frame: Option<&str>,
    anchor: Option<(f64, f64)>,
    scale: Option<f64>,
    bearing: Option<f64>,
    features: Option<Vec<PyFeature>>,
    flatten: f64,
    view_scale: Option<f64>,
    screen_classes: Option<Vec<String>>,
) -> PyResult<Bound<'py, PyDict>> {
    let d = compile_drawing(
        py,
        src,
        circuits,
        texlabels,
        base,
        include_policy,
        max_loop_iterations,
        max_shapes,
    )?;
    let gopts = geo_options(
        frame,
        anchor,
        scale,
        bearing,
        features,
        flatten,
        view_scale,
        screen_classes,
    )?;
    let scene = rpic_core::scene_json(&d, &rpic_core::SceneOptions::default());
    let mut g = geopic_core::georeference(&scene.json, &gopts).map_err(|e| geo_err(py, e))?;
    if !d.anims.is_empty() {
        g.warnings
            .push(geopic_core::Warning::animations_ignored(d.anims.len()));
    }
    let out = PyDict::new(py);
    out.set_item("geojson", json_loads(py, &g.features.to_json())?)?;
    let warnings = PyList::empty(py);
    // engine warnings first, then the scene's, then geopic's
    for w in d.warnings.iter().chain(scene.warnings.iter()) {
        warnings.append(diagnostic_dict(py, w)?)?;
    }
    for w in &g.warnings {
        warnings.append(json_loads(
            py,
            &serde_json::to_string(w).unwrap_or_default(),
        )?)?;
    }
    out.set_item("warnings", warnings)?;
    // pic `print` lines and engine notices, as in `compile`'s bundle
    out.set_item("diagnostics", d.diagnostics.clone())?;
    out.set_item(
        "registration",
        json_loads(
            py,
            &serde_json::to_string(&g.registration).unwrap_or_default(),
        )?,
    )?;
    // what the host needs to write canvas positions back as source
    // coordinates: pic `scale` units per inch and the whole-picture rescale
    let canvas = PyDict::new(py);
    canvas.set_item("rescale", d.rescale)?;
    let scale_var = d
        .vars
        .iter()
        .find(|(k, _)| k == "scale")
        .map_or(1.0, |(_, v)| *v);
    canvas.set_item("scale", scale_var)?;
    out.set_item("canvas", canvas)?;
    Ok(out)
}

/// Canvas positions (inches) of WGS84 points under a registration dict.
#[pyfunction]
fn geo_to_canvas_many(registration: &str, points: Vec<(f64, f64)>) -> PyResult<Vec<(f64, f64)>> {
    let reg: geopic_core::Registration = serde_json::from_str(registration)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("bad registration: {e}")))?;
    Ok(points
        .into_iter()
        .map(|(lon, lat)| {
            let p = reg.to_canvas(geopic_core::LonLat { lon, lat });
            (p[0], p[1])
        })
        .collect())
}

/// Canvas position (inches) of a WGS84 point under a registration dict (as
/// returned by `geo_compile`) — the host uses it to place features before
/// compiling (rpic.geo's two-pass feature injection).
#[pyfunction]
fn geo_to_canvas(registration: &str, lon: f64, lat: f64) -> PyResult<(f64, f64)> {
    let reg: geopic_core::Registration = serde_json::from_str(registration)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("bad registration: {e}")))?;
    let p = reg.to_canvas(geopic_core::LonLat { lon, lat });
    Ok((p[0], p[1]))
}

/// The MapLibre style-spec layers (docs/geopic.md § 6) for a geopic
/// FeatureCollection given as JSON text; returns the layers array as JSON
/// text (rpic.geo.maplibre_layers parses it). `opts_json` holds the layer
/// options (`{"text_scale": k}`; empty = default). Raises GeoError
/// (`invalid_geojson`) when the input is not a FeatureCollection,
/// (`invalid_options`) when the options do not validate.
#[pyfunction]
#[pyo3(signature = (fc_json, opts_json = ""))]
fn maplibre_layers(py: Python<'_>, fc_json: &str, opts_json: &str) -> PyResult<String> {
    geopic_core::maplibre::layers_json_with(fc_json, opts_json).map_err(|e| geo_err(py, e))
}

#[pymodule(name = "_native")]
fn rpic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    rpic_core::set_math_renderer(rpic_render::math::render_math);
    m.add_function(wrap_pyfunction!(render_svg, m)?)?;
    m.add_function(wrap_pyfunction!(render_png, m)?)?;
    m.add_function(wrap_pyfunction!(render_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    m.add_function(wrap_pyfunction!(compile_json, m)?)?;
    m.add_function(wrap_pyfunction!(scene, m)?)?;
    m.add_function(wrap_pyfunction!(geo_compile, m)?)?;
    m.add_function(wrap_pyfunction!(geo_to_canvas, m)?)?;
    m.add_function(wrap_pyfunction!(geo_to_canvas_many, m)?)?;
    m.add_function(wrap_pyfunction!(maplibre_layers, m)?)?;
    m.add("CompileError", m.py().get_type::<CompileError>())?;
    m.add("GeoError", m.py().get_type::<GeoError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
