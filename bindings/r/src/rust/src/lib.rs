use extendr_api::prelude::*;

// Low-level wrappers (no `@export`): they are package-internal and called by the
// user-facing R functions in R/rpic.R. Keeping them undocumented avoids R CMD
// check `\usage`/export warnings.

fn with_circuits(src: &str, circuits: bool) -> String {
    if circuits {
        format!("{}\n{}", rpic_core::CIRCUITS, src)
    } else {
        src.to_string()
    }
}

#[extendr]
fn rpic_svg_(src: &str, circuits: bool) -> std::result::Result<String, Error> {
    rpic_core::render_svg(&with_circuits(src, circuits)).map_err(Error::Other)
}

#[extendr]
fn rpic_png_(
    src: &str,
    file: &str,
    scale: f64,
    circuits: bool,
) -> std::result::Result<String, Error> {
    let svg = rpic_core::render_svg(&with_circuits(src, circuits)).map_err(Error::Other)?;
    let png = rpic_render::to_png(&svg, scale as f32).map_err(Error::Other)?;
    std::fs::write(file, png).map_err(|e| Error::Other(e.to_string()))?;
    Ok(file.to_string())
}

#[extendr]
fn rpic_pdf_(src: &str, file: &str, circuits: bool) -> std::result::Result<String, Error> {
    let svg = rpic_core::render_svg(&with_circuits(src, circuits)).map_err(Error::Other)?;
    let pdf = rpic_render::to_pdf(&svg).map_err(Error::Other)?;
    std::fs::write(file, pdf).map_err(|e| Error::Other(e.to_string()))?;
    Ok(file.to_string())
}

#[extendr]
fn rpic_manifest_(src: &str, circuits: bool) -> String {
    rpic_core::compile_json(&with_circuits(src, circuits))
}

// Macro to generate exports.
extendr_module! {
    mod rpic;
    fn rpic_svg_;
    fn rpic_png_;
    fn rpic_pdf_;
    fn rpic_manifest_;
}
