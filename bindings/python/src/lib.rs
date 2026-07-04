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
fn opts(circuits: bool, texlabels: bool, base: Option<PathBuf>) -> rpic_core::CompileOptions {
    rpic_core::CompileOptions {
        circuits,
        texlabels,
        base,
    }
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
) -> PyResult<rpic_core::Drawing> {
    rpic_core::compile_with_diagnostics(src, &opts(circuits, texlabels, base))
        .map_err(|e| err(py, e))
}

/// Render pic source to an SVG string.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None))]
fn render_svg(
    py: Python<'_>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
) -> PyResult<String> {
    Ok(rpic_core::to_svg(&compile_drawing(
        py, src, circuits, texlabels, base,
    )?))
}

/// Render pic source to PNG bytes (scale 1.0 = 96 dpi).
#[pyfunction]
#[pyo3(signature = (src, scale = 1.0, circuits = false, texlabels = false, base = None))]
fn render_png<'py>(
    py: Python<'py>,
    src: &str,
    scale: f32,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
) -> PyResult<Bound<'py, PyBytes>> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "scale must be a positive finite number",
        ));
    }
    let svg = rpic_core::to_svg(&compile_drawing(py, src, circuits, texlabels, base)?);
    let png = rpic_render::to_png(&svg, scale).map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(PyBytes::new(py, &png))
}

/// Render pic source to PDF bytes.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None))]
fn render_pdf<'py>(
    py: Python<'py>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
) -> PyResult<Bound<'py, PyBytes>> {
    let svg = rpic_core::to_svg(&compile_drawing(py, src, circuits, texlabels, base)?);
    let pdf = rpic_render::to_pdf(&svg).map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(PyBytes::new(py, &pdf))
}

/// Compile to the parsed bundle: a dict `{"svg": str, "animations": [dict],
/// "diagnostics": [str], "warnings": [dict]}`. Raises `CompileError` (with
/// the structured diagnostic on `exc.info`) on a pic error.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None))]
fn compile<'py>(
    py: Python<'py>,
    src: &str,
    circuits: bool,
    texlabels: bool,
    base: Option<PathBuf>,
) -> PyResult<Bound<'py, PyDict>> {
    let d = compile_drawing(py, src, circuits, texlabels, base)?;
    let out = PyDict::new(py);
    out.set_item("svg", rpic_core::to_svg(&d))?;
    let anims = PyList::empty(py);
    for a in &d.anims {
        let anim = PyDict::new(py);
        anim.set_item("id", format!("s{}", a.shape))?;
        anim.set_item("effect", &a.effect)?;
        anim.set_item("start", a.start)?;
        anim.set_item("duration", a.duration)?;
        anims.append(anim)?;
    }
    out.set_item("animations", anims)?;
    out.set_item("diagnostics", &d.diagnostics)?;
    let warnings = PyList::empty(py);
    for w in &d.warnings {
        warnings.append(diagnostic_dict(py, w)?)?;
    }
    out.set_item("warnings", warnings)?;
    Ok(out)
}

/// Compile to a JSON string `{ "svg": ..., "animations": [...],
/// "diagnostics": [...], "warnings": [...] }`
/// (or `{ "error": ..., "error_info": {...} }`). Parse with `json.loads`;
/// prefer `compile` for an already-parsed dict.
#[pyfunction]
#[pyo3(signature = (src, circuits = false, texlabels = false, base = None))]
fn compile_json(src: &str, circuits: bool, texlabels: bool, base: Option<PathBuf>) -> String {
    rpic_core::compile_json_with_options(src, &opts(circuits, texlabels, base))
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
