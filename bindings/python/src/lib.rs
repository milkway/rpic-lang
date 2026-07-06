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

fn compile_drawing(
    py: Python<'_>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
) -> PyResult<rpic_core::Drawing> {
    let opts = opts(circuits, texlabels, base, include_policy)?;
    rpic_core::compile_with_diagnostics(src, &opts).map_err(|e| err(py, e))
}

/// Render pic source to an SVG string.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None, include_policy = None))]
fn render_svg(
    py: Python<'_>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
) -> PyResult<String> {
    Ok(rpic_core::to_svg(&compile_drawing(
        py,
        src,
        circuits,
        texlabels,
        base,
        include_policy,
    )?))
}

/// Render pic source to PNG bytes (scale 1.0 = 96 dpi).
#[pyfunction]
#[pyo3(signature = (src, scale = 1.0, circuits = false, texlabels = false, base = None, include_policy = None))]
fn render_png<'py>(
    py: Python<'py>,
    src: &str,
    scale: f32,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
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
    )?);
    let png = rpic_render::to_png(&svg, scale).map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(PyBytes::new(py, &png))
}

/// Render pic source to PDF bytes.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None, include_policy = None))]
fn render_pdf<'py>(
    py: Python<'py>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
) -> PyResult<Bound<'py, PyBytes>> {
    let svg = rpic_core::to_svg(&compile_drawing(
        py,
        src,
        circuits,
        texlabels,
        base,
        include_policy,
    )?);
    let pdf = rpic_render::to_pdf(&svg).map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(PyBytes::new(py, &pdf))
}

/// Compile to the parsed bundle: a dict `{"svg": str, "animations": [dict],
/// "diagnostics": [str], "warnings": [dict]}`. Raises `CompileError` (with
/// the structured diagnostic on `exc.info`) on a pic error.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None, include_policy = None))]
fn compile<'py>(
    py: Python<'py>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
) -> PyResult<Bound<'py, PyDict>> {
    let d = compile_drawing(py, src, circuits, texlabels, base, include_policy)?;
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
    // rpic `animate scroll`: timeline-level scroll-scrub hint for the host.
    if d.anim_scroll {
        out.set_item("scroll", true)?;
    }
    Ok(out)
}

/// Compile to a JSON string `{ "svg": ..., "animations": [...],
/// "diagnostics": [...], "warnings": [...] }`
/// (or `{ "error": ..., "error_info": {...} }`). Parse with `json.loads`;
/// prefer `compile` for an already-parsed dict.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None, include_policy = None))]
fn compile_json(
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
    include_policy: Option<&str>,
) -> PyResult<String> {
    let opts = opts(circuits, texlabels, base, include_policy)?;
    Ok(rpic_core::compile_json_with_options(src, &opts))
}

#[pymodule]
fn rpic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    rpic_core::set_math_renderer(rpic_render::math::render_math);
    m.add_function(wrap_pyfunction!(render_svg, m)?)?;
    m.add_function(wrap_pyfunction!(render_png, m)?)?;
    m.add_function(wrap_pyfunction!(render_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    m.add_function(wrap_pyfunction!(compile_json, m)?)?;
    m.add("CompileError", m.py().get_type::<CompileError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
