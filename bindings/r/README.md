# rpic (R)

R bindings for [rpic](https://github.com/milkway/rpic-lang) — the pic graphics
language rendered to SVG / PNG / PDF, with a **knitr engine** for inline diagrams
in R Markdown / Quarto.

```r
rpic_svg('box "hi"; arrow; circle "x"')
rpic_png('A:(0,0); B:(2,0)\nresistor(A,B)', "circuit.png", scale = 2, circuits = TRUE)
rpic_pdf('box "hi"', "out.pdf")
jsonlite::fromJSON(rpic_manifest('box\nanimate last box with "pop"'))
```

### knitr engine

```r
rpic::rpic_register_knitr()
```
then in an R Markdown / Quarto document:

````
```{rpic, circuits=TRUE, scale=2}
A:(0,0); B:(2,0)
resistor(A,B)
```
````

## Build / develop

This package wraps the Rust crates `rpic-core` / `rpic-render` via
[extendr](https://extendr.github.io/). During monorepo development they are
referenced by **path**, so build in place:

```r
devtools::load_all("bindings/r")     # compiles the Rust and loads the package
```

`R CMD INSTALL` copies sources to a temp dir, which breaks the relative path
dependencies. For a self-contained, installable/CRAN-ready package, vendor the
Rust sources first:

```r
rextendr::vendor_pkgs("bindings/r")  # bundle crate sources into the package
```

(Tracked in issue #1.)
