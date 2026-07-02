# rpic — Design & Roadmap

> Provisional name: **rpic** ("PIC, in Rust"). A modern reimplementation of the
> **pic** picture-drawing language (Brian W. Kernighan, *PIC — A Language for
> Typesetting Graphics*, Software—Practice & Experience, 1982), focused on
> **SVG / PNG / PDF** output and **GSAP-based animation**, descended in spirit
> and semantics from **dpic** (J. D. Aplevich, BSD-2-Clause).

## Acknowledgments

`rpic` builds directly on the work of three people, credited in full in
[`ACKNOWLEDGMENTS.md`](ACKNOWLEDGMENTS.md):

- **Brian W. Kernighan** — creator of **pic** and its 1982 paper; the paradigm
  `rpic` preserves.
- **Dwight (J. D.) Aplevich** — author of **dpic** (our reference backends) and
  **circuit_macros** (inspiration for the native element library).
- **D. Richard Hipp** — author of **pikchr**, which showed the SVG-first,
  self-contained pic-family approach this project follows.

## Semantic Stance

`rpic` is **Kernighan-first**. Brian W. Kernighan's paper and manual define the
shape of the language: terse textual descriptions, a current point and current
direction, useful defaults, local blocks, labels, corners, macros, and geometry
that reads like a drawing being constructed step by step.

`dpic` is our practical oracle. When the original documents are terse or a
corner case needs executable confirmation, we compare against `dpic -v` and use
that behavior as the default compatibility target. rpic-specific additions, such
as animation metadata or native SVG/PNG/PDF output, should stay additive and
should not change the meaning of classic pic input. The current command and
backend-compatibility audit lives in
[`docs/dpic-compat-audit.md`](docs/dpic-compat-audit.md), including the raw
backend policy: dpic's `command`/`sh` directives are tolerated as silent
no-ops — `sh` is never executed and `command` text is never injected into the
output.

Pikchr is treated as a modern pic-family design reference, not as the oracle for
classic semantics. Features inspired by Pikchr must be explicit rpic extensions:
opt-in, documented, credited, and inert for existing dpic-compatible input. The
current research notes and adoption matrix live in [`docs/pikchr.md`](docs/pikchr.md).

Visual styling beyond pic/dpic attributes (CSS classes, gradients, themes) is
evaluated in [`docs/svg-styles.md`](docs/svg-styles.md): structured `class`
hooks and linear gradients are adopted as explicit extensions, named styles are
the existing `define` macro idiom, and raw CSS from `.pic` sources is rejected
under the same policy that keeps `command` text out of the output.

TeX math in labels is evaluated in [`docs/tex-labels.md`](docs/tex-labels.md):
not a dpic parity gap (dpic's SVG mode is also literal), adopted as the opt-in
`texlabels` extension rendered natively by RaTeX (pure-Rust KaTeX port) with
exact metrics; JS engines and mandatory TeX installations are rejected.

## Goals

1. **Preserve Kernighan's pic paradigm.** The language stays declarative and
   "english-like": you imagine walking around a plane dropping primitives.
   Relative positioning, default dimensions, automatic joining in the current
   direction of motion, named labels, compass corners, ordinals, `with`
   placement, fractions/`between`, blocks `[...]`, and `define` macros — all kept.
2. **SVG / PNG / PDF as first-class native outputs.** No troff, no LaTeX, no
   ImageMagick in the critical path. PNG comes free by rasterizing our own SVG.
3. **Simple animation via GSAP.** A small declarative `animate` extension to the
   language emits an animation manifest; a thin TypeScript layer turns it into a
   GSAP timeline in the browser.
4. **One core, many targets.** A single Rust core serves a native CLI *and*
   compiles to WASM to run in the browser.

## Why Rust (decision record)

dpic is C (machine-translated from Pascal via `p2c`), ~17.7k hand-written lines,
BSD-2-Clause. Its architecture is the classic three stages: **lexer → parser →
primitive tree → backend emitter**, with shared geometry helpers (arrowheads,
fills, splines, line styles). That maps 1:1 onto Rust and gives us, uniquely:

| Need              | Rust solution                          |
|-------------------|----------------------------------------|
| Lexer / parser    | hand-written lexer + recursive-descent (grammar.txt as spec) |
| Geometry engine   | plain `f64` structs                    |
| SVG               | string emission (reference: dpic `svg.c`) |
| **PNG**           | `resvg` + `tiny-skia` (no system deps) — dpic has *no* PNG |
| **PDF**           | direct emit (reference `pdf.c`) or `svg2pdf` |
| **Browser + GSAP**| `wasm-bindgen` → SVG in DOM → GSAP     |

The reference `dpic` binary is installed locally (`/usr/local/bin/dpic`); we use
it as a **test oracle** — compile the same source with `dpic -v` and diff the
geometry against our SVG backend.

## Architecture

```
            (later) native circuit element library  ──┐
                                                       │ define/for macros
   source .pic ─► [LEXER] ─► [PARSER] ─► AST ─► [EVAL] ─► primitive tree (IR)
                  token.rs    parser.rs         eval.rs        ir.rs
                                                                 │
                  ┌──────────────────────────────────┬──────────┴───────┐
                  ▼                ▼                   ▼                  ▼
              render/svg.rs   render/png.rs      render/pdf.rs      anim manifest
              (SVG 1.1)       (resvg/tiny-skia)  (direct / svg2pdf)  (JSON) ─► GSAP (TS)
                                                                                  │
                                                            crates/wasm ─► web playground
```

### Crate layout

- `crates/core` — the engine: `lexer`, `token`, `parser`, `ast`, `eval`,
  `ir`, `geom`, `render::{svg,png,pdf}`, `anim`. No I/O; pure `compile(&str) -> Drawing`.
- `crates/cli` — the `rpic` binary (file in → SVG/PNG/PDF out).
- `crates/wasm` *(later)* — `wasm-bindgen` wrapper exposing `compile()` to JS.
- `web/` *(later)* — TypeScript playground + GSAP animation driver.
- `std/` *(later)* — bundled circuit element library in the native macro dialect.

## The pic language (spec we implement)

Tokens: `crates/core` mirrors dpic's `dpic.toks`. Grammar: mirrors dpic's
`grammar.txt` (LALR). We implement it as recursive descent. Highlights:

- **Primitives:** `box circle ellipse arc line arrow move spline` + `"text"`.
- **Object attributes:** `ht wid rad diam thick scaled`, direction (`up down
  left right`), line type (`solid dotted dashed invis`), `chop`, `fill`,
  arrowheads (`<- -> <->`), `then cw ccw same`, `at from to by with`, text
  position (`ljust rjust above below center`), color (`color/outline/shade`).
- **Positions:** pairs `(x,y)`, `expr between p and p`, `expr of the way
  between …`, places, compass corners (`.n .ne .center …`), ordinals (`last`,
  `2nd last box`), `.x/.y`, `Here`.
- **Control:** `[ … ]` blocks (local scope), `if … then { } else { }`,
  `for v=a to b [by c] do { }`, `define name X…X` macros, `sprintf(…)`, `print`,
  `reset`, `sh`, `copy`, environment variables (`boxht`, `linewid`, `scale`, …).

### Animation extension (proposed, additive — keeps the paradigm)

```
box "A"; arrow; box "B"
animate last arrow  with "draw"  for 0.5
animate 2nd box     with "fade"  for 0.3  after last arrow
```

`animate <object-ref> with "<effect>" [for <dur>] [after <ref>|at <t>]` compiles
to entries in a JSON manifest keyed by the SVG element id assigned to each
primitive. The browser TS layer maps effects (`draw`, `fade`, `pop`, `move`, …)
to GSAP tweens on a single timeline. Nothing about static rendering changes.

## Roadmap (tracked as tasks)

1. ✅ Scaffold workspace + this design doc.
2. **Lexer** — full token set, tests. *(in progress)*
3. Parser + AST — full grammar.
4. Eval / geometry engine — positions, corners, ordinals, bbox.
5. SVG backend (validate vs `dpic -v`).
6. PNG backend (resvg/tiny-skia).
7. PDF backend.
8. Animation syntax + GSAP TS layer.
9. WASM build + web playground.
10. Native macro system + circuit element library (replaces m4/circuit_macros).

## Build & run

```sh
cargo build
cargo test
cargo run -p rpic-cli -- examples/pipeline.pic                 # → SVG on stdout
cargo run -p rpic-cli -- --png --scale 2 -o out.png examples/pipeline.pic
cargo run -p rpic-cli -- --pdf -o out.pdf examples/pipeline.pic
cargo run -p rpic-cli -- --ast examples/pipeline.pic           # debug: syntax tree
```

PNG/PDF live in `crates/render` (pure Rust: `resvg`/`tiny-skia`, `svg2pdf`) so
`rpic-core` stays dependency-free and WASM-friendly.

### Browser playground (WASM + GSAP)

```sh
./web/build.sh                       # wasm-pack build → web/pkg
cd web && python3 -m http.server 8080
# open http://localhost:8080/
```

`crates/wasm` exposes `compile(src) -> JSON {svg, animations, diagnostics}`. `web/app.js`
injects the SVG and builds a GSAP timeline from the manifest. Toolchain note:
`wasm-pack`/the wasm build use the **rustup** stable toolchain (which carries the
`wasm32-unknown-unknown` std); a Homebrew `rustc` on `PATH` lacks it, so
`build.sh` prepends the rustup bin dir.

### Animation syntax (implemented)

```
animate <object> with "<effect>" [for <dur>] [at <t> | after <object>] [delay <d>]
```

Effects: `draw` (stroke-on), `fade`, `pop`. Timing is sequential by default, or
absolute (`at`) / relative to another object's end (`after`). Each primitive gets
a stable SVG id `s<N>`; the manifest references it; GSAP tweens it.

## Circuit element library (native, replacing m4/circuit_macros)

`crates/core/src/std/circuits.pic` is a native `define`-dialect library
(embedded as `rpic_core::CIRCUITS`, loaded with the CLI `-c/--circuits` flag).
Elements are rotation-aware and drawn between two named points:

```
A: (0,0); B: (2,0)
resistor(A, B)        # also: capacitor inductor diode battery wire dot ; ground(P)
```

This is an independent re-authoring (in spirit) of circuit_macros (LPPL) —
**79 elements** across 10 batches:

- **Analog 2-terminal:** resistor, iec_resistor, capacitor, polcap, varcap, inductor, varind, diode, schottky, zener, led, photodiode, battery, voltage_source, ac_source, current_source, fuse, lamp, thermistor, ldr, varistor, crystal, spark_gap, wire, dot, tline, bus
- **Logic — distinctive:** buffer, not, and, nand, or, nor, xor, xnor
- **Logic — IEEE rectangular:** iecgate, ieee_and, ieee_or, ieee_xor, ieee_buf
- **Active devices:** opamp, npn, pnp, nmos, pmos, njfet, pjfet, phototransistor, switch, pushbutton, spdt, relay, potentiometer, transformer
- **Sources (controlled):** vsource_ctrl, isource_ctrl
- **Instruments:** meter, voltmeter, ammeter, ohmmeter
- **Transducers / machines:** speaker, microphone, bell, motor, generator, solar_cell, thermocouple, antenna
- **Grounds & rails:** ground, chassis_ground, signal_ground, vdd, terminal
- **Digital blocks:** ic_block, mux
- **Annotations:** clabel, current, voltage, hop

Two-terminal elements take two named points (`resistor(A,B)`); centered devices
take one (`and_gate(C)`) and expose terminal coordinates as globals
(`gA_*`/`gB_*`/`gY_*`, `gBase_*`/`gColl_*`/`gEmit_*`, etc.). Note: element scratch
variables must not collide with macro names (macros expand textually).

See `examples/flashlight.pic` for a complete annotated schematic
(`rpic -c --png -o flashlight.png examples/flashlight.pic`). The web playground
has a **circuit library** checkbox.

## Licensing

- Our code: **BSD-2-Clause** (matches dpic, lets us use dpic's backends as
  reference specs).
- `circuit_macros` is LPPL 1.3c; we are **re-authoring** the element library
  natively rather than redistributing the m4 sources, so the new library is our
  own work informed by the documented element geometry.
