//! Python bindings for rpic (PyO3). Exposes the pic → SVG/PNG/PDF engine and
//! the animation manifest. Build with maturin.

use pyo3::prelude::*;
use pyo3::types::PyBytes;

fn with_circuits(src: &str, circuits: bool) -> String {
    if circuits {
        format!("{}\n{}", rpic_core::CIRCUITS, src)
    } else {
        src.to_string()
    }
}

fn err(e: String) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(e)
}

/// Render pic source to an SVG string.
#[pyfunction]
#[pyo3(signature = (src, circuits = false))]
fn render_svg(src: &str, circuits: bool) -> PyResult<String> {
    rpic_core::render_svg(&with_circuits(src, circuits)).map_err(err)
}

/// Render pic source to PNG bytes (scale 1.0 = 96 dpi).
#[pyfunction]
#[pyo3(signature = (src, scale = 1.0, circuits = false))]
fn render_png<'py>(
    py: Python<'py>,
    src: &str,
    scale: f32,
    circuits: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    if !(scale > 0.0) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "scale must be a positive number",
        ));
    }
    let svg = rpic_core::render_svg(&with_circuits(src, circuits)).map_err(err)?;
    let png = rpic_render::to_png(&svg, scale).map_err(err)?;
    Ok(PyBytes::new(py, &png))
}

/// Render pic source to PDF bytes.
#[pyfunction]
#[pyo3(signature = (src, circuits = false))]
fn render_pdf<'py>(py: Python<'py>, src: &str, circuits: bool) -> PyResult<Bound<'py, PyBytes>> {
    let svg = rpic_core::render_svg(&with_circuits(src, circuits)).map_err(err)?;
    let pdf = rpic_render::to_pdf(&svg).map_err(err)?;
    Ok(PyBytes::new(py, &pdf))
}

/// Compile to a JSON string `{ "svg": ..., "animations": [...],
/// "diagnostics": [...] }`
/// (or `{ "error": ... }`). Parse with `json.loads`.
#[pyfunction]
#[pyo3(signature = (src, circuits = false))]
fn compile_json(src: &str, circuits: bool) -> String {
    rpic_core::compile_json(&with_circuits(src, circuits))
}

#[pymodule]
fn rpic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render_svg, m)?)?;
    m.add_function(wrap_pyfunction!(render_png, m)?)?;
    m.add_function(wrap_pyfunction!(render_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(compile_json, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
