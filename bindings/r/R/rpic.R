#' @keywords internal
"_PACKAGE"

#' Render pic source to an SVG string
#' @param src pic source code.
#' @param circuits load the native circuit-element library.
#' @return an SVG string.
#' @examples
#' rpic_svg('box "hi"; arrow; circle "x"')
#' @export
rpic_svg <- function(src, circuits = FALSE) {
  rpic_svg_(src, circuits)
}

#' Render pic source to a PNG file
#' @param src pic source code.
#' @param file output path.
#' @param scale raster scale (1 = 96 dpi).
#' @param circuits load the native circuit-element library.
#' @return the file path, invisibly.
#' @export
rpic_png <- function(src, file, scale = 2, circuits = FALSE) {
  invisible(rpic_png_(src, file, scale, circuits))
}

#' Render pic source to a PDF file
#' @inheritParams rpic_png
#' @return the file path, invisibly.
#' @export
rpic_pdf <- function(src, file, circuits = FALSE) {
  invisible(rpic_pdf_(src, file, circuits))
}

#' Compile to a JSON `{svg, animations}` bundle (as a string)
#' @inheritParams rpic_svg
#' @return a JSON string.
#' @examples
#' rpic_manifest('box; animate last box with "pop"')
#' @export
rpic_manifest <- function(src, circuits = FALSE) {
  rpic_manifest_(src, circuits)
}

#' knitr engine for ```{rpic}``` chunks
#'
#' Register with [rpic_register_knitr()], then write pic code in a chunk:
#' ````
#' ```{rpic, circuits=TRUE}
#' A:(0,0); B:(2,0)
#' resistor(A,B)
#' ```
#' ````
#' @param options knitr chunk options.
#' @export
rpic_knitr_engine <- function(options) {
  src <- paste(options$code, collapse = "\n")
  circuits <- isTRUE(options$circuits)
  scale <- if (is.null(options$scale)) 2 else options$scale
  path <- knitr::fig_path(".png", options)
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
  rpic_png(src, path, scale = scale, circuits = circuits)
  knitr::engine_output(options, options$code, knitr::include_graphics(path))
}

#' Register the rpic knitr engine
#' @export
rpic_register_knitr <- function() {
  knitr::knit_engines$set(rpic = rpic_knitr_engine)
}
