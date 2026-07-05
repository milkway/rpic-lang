<p align="center">
  <img src="assets/logo.svg" alt="rpic logo — a lowercase r annotated like an engineering blueprint, drawn in rpic itself" width="110">
</p>

<h1 align="center">rpic</h1>

<p align="center">
  <strong>A modern reimplementation of the <a href="https://en.wikipedia.org/wiki/Pic_language">pic</a> picture-drawing language in Rust —<br>
  SVG / PNG / PDF output, GSAP animation, and a native circuit-element library.</strong><br>
  <sub>Even the logo is a pic program: <a href="assets/logo.pic">assets/logo.pic</a>, rendered by rpic.</sub>
</p>

<p align="center">
  <strong>📖 Documentation: <a href="https://rpic.dev">rpic.dev</a></strong> — language tour, every extension with live examples, spec and the pic-family history.
</p>

<p align="center">
  <a href="https://github.com/milkway/rpic-lang/actions/workflows/ci.yml"><img src="https://github.com/milkway/rpic-lang/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/milkway/rpic-lang/releases"><img src="https://img.shields.io/github/v/release/milkway/rpic-lang?sort=semver&display_name=tag" alt="Release"></a>
  <a href="https://crates.io/crates/rpic-cli"><img src="https://img.shields.io/crates/v/rpic-cli?label=crates.io&color=informational" alt="crates.io"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-BSD--2--Clause-blue.svg" alt="License"></a>
  <a href="https://pypi.org/project/rpiclang/"><img src="https://img.shields.io/pypi/v/rpiclang?label=PyPI&color=informational" alt="PyPI"></a>
  <a href="https://www.npmjs.com/package/@strategicprojects/rpic"><img src="https://img.shields.io/npm/v/%40strategicprojects%2Frpic?label=npm&color=informational" alt="npm"></a>
  <img src="https://img.shields.io/badge/rust-edition%202024-orange.svg" alt="Rust 2024">
</p>

<p align="center">
  <img src="assets/pipeline.svg" alt="A pic diagram rendered by rpic" width="620">
</p>

> The figure above is rendered by `rpic` from this source — the very example
> Brian Kernighan used to introduce pic in 1982:
>
> ```pic
> ellipse "document"; arrow; box "PIC"; arrow
> box "TBL/EQN" "(optional)" dashed; arrow
> box "TROFF"; arrow; ellipse "typesetter"
> ```

## Why rpic

`rpic` keeps Kernighan's original pic paradigm — you describe a drawing by
*"walking around a plane dropping primitives"*, with relative positioning,
default dimensions, compass corners, ordinals, blocks and macros — and brings it
to the modern web era:

- **SVG / PNG / PDF**, all **pure-Rust** — no troff, no LaTeX, no ImageMagick,
  no system libraries.
- **Fast**: single-digit-millisecond cold renders, flat with diagram size —
  ~80× faster than mermaid-cli in a docs pipeline
  ([benchmark](https://rpic.dev/docs/performance/), reproducible via
  [`tools/bench`](tools/bench)).
- A small **declarative animation** layer (`animate …`) that plays in the
  browser with [GSAP](https://gsap.com/).
- A native **circuit-element library** (79 elements) — a from-scratch
  re-imagining of `circuit_macros`.
- **Editor-grade diagnostics**: structured errors with exact spans,
  did-you-mean hints, and a warnings channel for accepted-but-ignored input —
  positions always relative to your own source, even across includes and
  loaded libraries.
- One core, **many targets**: a native CLI, WebAssembly, and bindings for
  **Python**, **R**, and **JavaScript/TypeScript**.

The compatibility rule is deliberately **Kernighan-first**: the 1982 paper and
manual set the language philosophy and the meaning of classic pic constructs.
When the texts leave room for interpretation, `dpic` is the practical oracle:
we compare against `dpic -v`, keep its well-tested geometry and macro behavior
in view, and document any intentional rpic extension separately.

## The language

```pic
.PS
boxht = 0.3; boxwid = 0.6
A: box "input"
arrow
B: box "process" fill 0.9
arrow
ellipse "output"
arc -> from A.n to last ellipse.n
.PE
```

Primitives: `box circle ellipse arc line arrow move spline` + text. Positioning:
named labels, compass corners (`.n .ne .center …`), ordinals (`last`,
`2nd last box`), `with … at`, fractions (`1/3 between A and B`), blocks `[ … ]`.
Programmability: `define` macros with `$1…$9`, `for`, `if`, `sprintf`,
environment variables.

Explicit rpic extensions — `margin`, `fit`, `behind`, `close`, `brace`,
`hatch`, `gradient`, `opacity`, `class` hooks, `dot`, **`texlabels`**
(KaTeX-grade TeX math in labels, rendered natively) and the `animate` layer —
are opt-in and inert for classic pic/dpic-compatible input. Each has a page with live
examples at [rpic.dev](https://rpic.dev/docs/extensions/margin); the design
notes live in [`docs/extensions.md`](docs/extensions.md).

## Outputs

```sh
rpic diagram.pic                       # SVG to stdout
rpic --png --scale 2 -o out.png diagram.pic
rpic --pdf -o out.pdf diagram.pic
rpic -c circuit.pic                    # load the circuit-element library
rpic -t paper.pic                      # typeset $…$ labels as TeX math
rpic --json diagram.pic                # {svg, animations, diagnostics, warnings}
```

## Examples

A gallery of diagrams from the dpic distribution — including reproductions of
**Brian Kernighan's** original pic-manual figures — rendered by rpic itself lives
in [`examples/dpic/`](examples/dpic). Each `.pic` is paired with its rendered
`.svg`, with full credits and a parity matrix in
[`examples/dpic/STATUS.md`](examples/dpic/STATUS.md).

```sh
rpic --svg examples/dpic/manual/man16.pic -o man16.svg
```

[`examples/figuras/`](examples/figuras) collects circuit_macros figures from
**André Leite's** personal collection, rendered by rpic.
[`examples/lib3d/`](examples/lib3d) shows 3D drawings (axonometric projection,
à la circuit_macros' `lib3D`) rendered to flat SVG.

## Animation (GSAP)

A declarative extension, faithful to pic's style:

```pic
box "A"; arrow; box "B"
animate 1st box   with "pop"   for 0.4
animate 1st arrow with "draw"
animate 2nd box   with "fade"  after 1st arrow
```

This compiles to an SVG plus a JSON manifest; the browser layer turns it into a
GSAP timeline. Try it in the **playground**:

```sh
./web/build.sh && (cd web && python3 -m http.server 8080)   # http://localhost:8080
```

## Circuit library

Enable with `-c`, or in-source with `copy "circuits"` (the analog of
`texlabels = 1` for `-t`). Two-terminal elements take two named points;
centered devices take one and expose their terminals as variables.

```pic
.PS
SW:(0,0); NW:(0,1.4); NE:(2.6,1.4); SE:(2.6,0)
battery(SW,NW); resistor(NW,NE); capacitor(NE,SE); inductor(SE,SW)
.PE
```

<p align="center">
  <img src="assets/rlc.svg" alt="RLC circuit" height="150">
  &nbsp;&nbsp;
  <img src="assets/logic.svg" alt="Logic gates" height="150">
</p>

**79 elements** across analog parts, distinctive & IEEE logic gates, BJT/MOSFET/
JFET transistors, op-amps, sources, meters, transducers, grounds and
annotations. See [`crates/core/src/std/circuits.pic`](crates/core/src/std/circuits.pic).

## Install

```sh
# from source (any platform)
cargo install --git https://github.com/milkway/rpic-lang rpic-cli

# Homebrew (macOS / Linux)
brew install milkway/rpic/rpic

# Scoop (Windows)
scoop install https://raw.githubusercontent.com/milkway/rpic-lang/main/packaging/scoop/rpic.json

# Debian/Ubuntu — download the .deb from the Releases page, then:
sudo dpkg -i rpic_*.deb
```

Prebuilt binaries for macOS / Linux / Windows are attached to each
[release](https://github.com/milkway/rpic-lang/releases).

## Language bindings

### Python — [`bindings/python`](bindings/python)

```sh
pip install rpiclang          # distribution name; the module is `rpic`
```

```python
import rpic
svg = rpic.render_svg('box "hi"; arrow; circle "x"')
open("out.png", "wb").write(rpic.render_png('box "hi"', scale=2.0))
bundle = rpic.compile('box\nanimate last box with "pop"')
# bundle["diagnostics"] = pic `print` output; bundle["warnings"] = structured
# warnings; errors raise rpic.CompileError with the diagnostic on `exc.info`
```

### R — [milkway/rpic-r](https://github.com/milkway/rpic-r) (separate repo)

```r
remotes::install_github("milkway/rpic-r")
rpic::rpic_svg('A:(0,0); B:(2,0)\nresistor(A,B)', circuits = TRUE)
rpic::rpic_register_knitr()        # ```{rpic} chunks in R Markdown / Quarto
```

### JavaScript / TypeScript — [`bindings/js`](bindings/js)

```js
import * as rpic from '@strategicprojects/rpic';
await rpic.ready();                      // or ready(undefined, { math: true })
const { svg, animations, diagnostics, warnings } = rpic.compile('box "A"; arrow; box "B"');
rpic.animate(stage, animations, gsap);   // GSAP timeline
```

The default wasm is lean; `ready(undefined, { math: true })` lazy-loads a
math-enabled build so `texlabels` typeset `$…$` labels in the browser.
Compile errors throw with structured `err.errorInfo` (span, kind,
did-you-mean hint) for editor integrations.

## Build from source

```sh
cargo build --release      # CLI in target/release/rpic
cargo test                 # full test suite
```

| Path | What |
|------|------|
| `crates/core` | engine: lexer, parser, eval, IR, SVG backend, `std/circuits.pic` |
| `crates/render` | PNG/PDF (resvg, svg2pdf) |
| `crates/cli` | the `rpic` binary |
| `crates/capi` | stable C ABI (`rpic.h`) |
| `crates/wasm` | WebAssembly bindings |
| `bindings/{python,js}` | Python & JS/TS bindings (R lives at [milkway/rpic-r](https://github.com/milkway/rpic-r)) |
| `web/` | browser playground (GSAP) |
| `packaging/` | deb / Homebrew / Scoop config |

## Acknowledgments

`rpic` stands on the shoulders of giants — see [ACKNOWLEDGMENTS.md](ACKNOWLEDGMENTS.md):
**Brian W. Kernighan** (pic), **Dwight Aplevich**
([dpic](https://gitlab.com/aplevich/dpic),
[circuit_macros](https://gitlab.com/aplevich/circuit_macros)), and
**D. Richard Hipp** ([pikchr](https://pikchr.org)).

## License

[BSD-2-Clause](LICENSE).
