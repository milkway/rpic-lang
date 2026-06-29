# rpic

**A modern reimplementation of the [pic] picture-drawing language in Rust,
focused on SVG / PNG / PDF output and GSAP-based animation.**

`rpic` keeps Brian Kernighan's original pic paradigm — you describe a drawing by
"walking around a plane dropping primitives," with relative positioning, default
dimensions, compass corners, ordinals, blocks and macros — and targets the
modern web era: native SVG/PNG/PDF with **no system dependencies**, plus a small
declarative animation layer that plays in the browser via [GSAP].

```pic
.PS
ellipse "document"; arrow; box "PIC"; arrow; ellipse "typesetter"
.PE
animate 1st ellipse with "pop"
animate last arrow   with "draw"
```

## Features

- **Faithful pic language** — primitives (`box circle ellipse arc line arrow move
  spline`), relative positioning, corners/ordinals, blocks `[...]`, `define`
  macros, `if`/`for`, `sprintf`, environment variables.
- **SVG / PNG / PDF**, all pure-Rust (`resvg`/`tiny-skia`, `svg2pdf`).
- **Animation** — `animate <obj> with "draw|fade|pop" [for s] [at s | after <obj>]`
  compiles to a JSON manifest driven by GSAP; runs in the browser via WASM.
- **Circuit element library** — 79 native elements (resistors, transistors, logic
  gates, op-amps, meters, …); enable with `-c`.
- **Browser playground** — edit pic, render, and watch the animation.

## Install / build

```sh
cargo build --release
cargo test
```

## Usage

```sh
rpic diagram.pic                 # SVG to stdout
rpic --png --scale 2 -o out.png diagram.pic
rpic --pdf -o out.pdf diagram.pic
rpic -c --png -o circuit.png examples/flashlight.pic   # with circuit library
```

### Browser playground (WASM + GSAP)

```sh
./web/build.sh                   # wasm-pack build → web/pkg
cd web && python3 -m http.server 8080   # open http://localhost:8080/
```

## Layout

| Path | What |
|------|------|
| `crates/core` | engine: lexer, parser, eval, IR, SVG backend, `std/circuits.pic` |
| `crates/render` | PNG/PDF (resvg, svg2pdf) |
| `crates/cli` | the `rpic` binary |
| `crates/wasm` | WASM bindings |
| `web/` | browser playground (GSAP) |
| `bindings/` | R and Python language bindings *(in progress)* |

## Acknowledgments

`rpic` stands on the shoulders of giants — see [ACKNOWLEDGMENTS.md]:
**Brian W. Kernighan** (pic), **Dwight Aplevich** ([dpic], [circuit_macros]),
and **D. Richard Hipp** ([pikchr]).

## License

BSD-2-Clause. See [LICENSE](LICENSE).

[pic]: https://en.wikipedia.org/wiki/Pic_language
[GSAP]: https://gsap.com/
[dpic]: https://gitlab.com/aplevich/dpic
[circuit_macros]: https://gitlab.com/aplevich/circuit_macros
[pikchr]: https://pikchr.org
[ACKNOWLEDGMENTS.md]: ACKNOWLEDGMENTS.md
