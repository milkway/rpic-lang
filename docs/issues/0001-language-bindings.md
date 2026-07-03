# Issue: Integration interface for other technologies (R, Python, web)

**Labels:** enhancement, bindings, R, Python
**Status:** proposal / draft

## Summary

Expose `rpic` (the pic → SVG/PNG/PDF + animation engine) through clean,
idiomatic interfaces so it can be driven from **R** and **Python** (and the web),
not only the CLI. Goal: a user writes pic source in their language of choice and
gets back an SVG string, PNG/PDF bytes, and the animation manifest.

The engine already has the right shape for this: `rpic-core` is pure Rust with no
I/O and a tiny surface (`compile`, `render_svg`, `compile_json`, `CIRCUITS`), and
`rpic-render` adds PNG/PDF. We just need thin language wrappers over that surface.

## Why

- **R**: the project directory started life as an RStudio project; a natural fit
  is an R package + a **knitr/R Markdown language engine** so that
  ```` ```{rpic} ```` chunks render circuits/diagrams inline in reports and books
  (bookdown/Quarto). This mirrors how `tikz`/`dot` engines work today.
- **Python**: a native extension (no system deps) lets users render diagrams in
  scripts, **Jupyter**, Sphinx, and docs pipelines.
- **Web/notebooks**: the WASM `compile()` already returns `{svg, animations}`;
  document it as a supported integration (Observable, JupyterLite).

## Proposed approaches (to evaluate)

1. **Stable C ABI (`rpic-capi`, cdylib).** Export:
   - `rpic_render_svg(src, circuits) -> char*`
   - `rpic_compile_json(src, circuits) -> char*` (svg + manifest)
   - `rpic_render_png(src, scale, circuits, out_len*) -> unsigned char*`
   - `rpic_render_pdf(src, circuits, out_len*) -> unsigned char*`
   - `rpic_free_string(ptr)` for string results
   - `rpic_free_bytes(ptr, len)` for PNG/PDF buffers
   - `circuits` is the `0`/`1` flag that prepends the native circuit library.
   This single ABI backs every other binding and keeps the contract small.

2. **Python: `rpic-py` via [PyO3] + [maturin].** Idiomatic API:
   ```python
   import rpic
   svg = rpic.render_svg(src, circuits=True)
   png = rpic.render_png(src, scale=2.0)
   pdf = rpic.render_pdf(src)
   bundle = rpic.compile(src)          # {"svg": ..., "animations": [...]}
   ```
   Plus a Jupyter `_repr_svg_`/`_repr_png_` helper and an IPython cell magic
   `%%rpic`. Wheels built by maturin (pure Rust → no system deps).

3. **R: `rpic` R package via [extendr] (or Rcpp over the C ABI).** API:
   ```r
   rpic::render_svg(src, circuits = TRUE)
   rpic::render_png(src, scale = 2, file = "out.png")
   ```
   Plus a **knitr engine** (`knitr::knit_engines$set(rpic = ...)`) so
   ```` ```{rpic} ```` chunks render in R Markdown / Quarto / bookdown, and an
   htmlwidget that ships the SVG + manifest and plays the GSAP timeline in
   HTML output.

4. **WASM/JSON (already shipping).** `crates/wasm::compile` returns the
   `{svg, animations}` bundle; document Observable / JupyterLite usage.

5. **CLI subprocess fallback.** Works from any language today
   (`rpic --svg/--png/--pdf [-c]`); document as the zero-dependency baseline.

## Deliverables

- [ ] `crates/capi` — stable C ABI + generated `rpic.h` (cbindgen).
- [ ] `bindings/python` (`rpic-py`, PyO3+maturin) with wheels + Jupyter repr + `%%rpic` magic.
- [ ] `bindings/r` (extendr) with `render_*()` + a **knitr engine** + htmlwidget for animation.
- [ ] Docs: one quickstart per language; a shared examples gallery.
- [ ] CI: build/test the C ABI, Python wheels (cibuildwheel), R package check.

## Acceptance criteria

- From R and Python: compile a pic source (incl. `-c` circuit library) to SVG,
  PNG, and PDF, and obtain the animation manifest, with no system dependencies.
- An R Markdown / Quarto document with a ```` ```{rpic} ```` chunk renders a
  diagram inline.
- A Jupyter notebook displays a diagram via `rpic.compile(...)`.

## Open questions

- R: extendr vs Rcpp-over-C-ABI — which is lower friction for CRAN?
- Animation in static outputs (PDF/PNG): export the first/last frame or a
  storyboard; keep GSAP only for HTML/web targets.
- Packaging: one polyglot monorepo vs per-language release artifacts.

## Notes

Drafted locally (no GitHub remote configured yet). To publish as a real GitHub
issue, a remote repo must exist first — see the parent task discussion.
