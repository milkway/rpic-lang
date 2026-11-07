# TeX/LaTeX Math Labels — Viability Analysis

Status: design evaluation for issue #115.
Date: 2026-07-02.

Can rpic typeset TeX math in labels (`$G(s)$`, `$-\frac{T}{2}$`) instead of
printing them literally? This document evaluates the candidates against
rpic's hard constraints and records the decision. Stance unchanged:
**Kernighan-first, dpic as practical oracle** — everything here is an
explicit, opt-in rpic extension.

## Framing: Not a Parity Gap

Verified against dpic 2025.08.01 and its manual:

- `dpic -v` (SVG mode) emits `$\beta$` **literally**, dollars included —
  exactly what rpic does today. rpic is already at parity.
- In the dpic ecosystem, math is typeset by **LaTeX** compiling dpic's
  pgf/pstricks output. The manual is explicit: "dpic knows nothing of text
  formatting except for SVG output"; for math-quality SVG it recommends
  "LaTeX to produce a pdf file, followed by a pdf-to-svg converter".
- circuit_macros even ships `svg.m4` (`svg_frac`, `<tspan>` sub/superscripts,
  Greek entities) as a hand-rolled workaround for SVG output.

So TeX labels are **pure extension territory** — but with a real audience:
the rpic corpus alone has **118 unique math labels across 45 files**
(`$Q_1$`, `$F_1(\omega)$`, `$-\frac{T}{2}$`, …), written by circuit_macros
users who expect LaTeX to typeset them eventually.

## Hard Constraints

1. **Pure-Rust core** — no JS runtime, no mandatory external processes.
2. **Backend stability** — PNG/PDF rasterize from the SVG via resvg/svg2pdf;
   any math output must be static SVG that resvg accepts. resvg has **no
   `<foreignObject>`/HTML support** (confirmed: usvg drops it).
3. **#129 policy** — the `.pic` source never triggers process execution.
4. **WASM budget** — the playground wasm is ~316 KB today.
5. **Exact metrics needed** — label bbox/anchors feed pic layout; rpic's
   current text metrics are heuristic (0.6 em/char).

## Candidates (researched July 2026)

| Candidate | Verdict | Why |
| --- | --- | --- |
| **RaTeX** (pure-Rust KaTeX port) | **Adopt** | v0.1.12 (Jun 2026), very active, MIT, on crates.io. >99.5% KaTeX syntax coverage. `embed_glyphs` + `embed-fonts` emit self-contained path-only SVG; KaTeX fonts bundled (~548 KB). **Spike-validated below.** |
| Typst + mitex (LaTeX→Typst) | Defer (2nd choice) | Pure Rust, high quality, path-only SVG — but ~5–10 MB binary cost, heavy build, ~10 MB compressed wasm. Overkill for formula-only labels. Revisit if RaTeX stalls. |
| LaTeX + dvisvgm shell-out | Defer (optional escape hatch) | Highest fidelity; `--no-fonts` gives path-only SVG + exact width/height/depth via the `preview` package. But 60–200 MB external TeX dependency, ~0.5–1.5 s/formula, and process execution — per #129 this could only ever be a **render-time CLI flag** (`--latex`), never source-triggered. Defer until demand. |
| MathJax v4 via embedded JS engine | Reject | SVG output is proven (path-only, `fontCache:'none'`), but embedding costs: rusty_v8/deno_core ≈ +30 MB (proven), quickjs ≈ +1–2 MB (MathJax-on-quickjs unproven). A JS dependency inside a pure-Rust core for something RaTeX does natively. |
| KaTeX (JS) in core | Reject | **Architecturally unusable offline**: outputs HTML+CSS/MathML only, no SVG mode; resvg drops `foreignObject`. Fine in a *browser host page* (see WASM strategy). |
| ReX (Rust TeX engine) | Reject | Upstream dead since 2020; the living fork (KenyC) is unpublished on crates.io. |

Ecosystem check: mermaid/kroki need headless Chrome or Node microservices
for math; pandoc/quarto delegate to the browser. **Penrose** is the closest
architectural cousin (TeX→SVG paths server-side via MathJax in its own
TS runtime). rpic with RaTeX would be the rare tool doing this natively.

## Spike Results (RaTeX 0.1.12, validated locally)

Pipeline `ratex_parser::parse → ratex_layout::layout → to_display_list →
ratex_svg::render_to_svg { embed_glyphs: true }` on real corpus labels,
rasterized by resvg **with no font database**:

| Formula | layout+SVG | resvg raster | SVG `<text>` elems |
| --- | --- | --- | --- |
| `\beta` | 1.8 ms (cold) | 0.6 ms | 0 (paths only) |
| `G(s)` | 0.2 ms | 0.2 ms | 0 |
| `-\frac{T}{2}` | 0.2 ms | 0.1 ms | 0 |
| `F_1(\omega)` | 0.2 ms | 0.2 ms | 0 |
| `\int_0^\infty e^{-st}f(t)\,dt` | 0.4 ms | 0.5 ms | 0 |

- Output quality is KaTeX-grade (visually verified, PNG via resvg).
- `DisplayList` exposes **width, height, and depth** (baseline) in em units —
  *exact* metrics, better than the current 0.6 em/char heuristic and better
  than dpic's own `dptextratio` approximation.
- Binary cost measured: spike binary (ratex + full resvg) is **7.29 MB vs
  rpic's current 7.19 MB** — since rpic already links resvg, the marginal
  cost is ≈ **1–2 MB** (fonts are 548 KB).

## Recommended Design

**Adopt RaTeX as the native math renderer, opt-in at the source level.**

- **Opt-in switch**: a `texlabels = 1` environment variable (like `margin`).
  When on, label strings **fully delimited** as `$…$` are typeset as math;
  everything else renders as today. Default off → byte-for-byte classic
  output, corpus untouched. This matches circuit_macros' own convention
  (element labels "assumed to be in math mode").
- No security concern with a source-level switch: rendering is a pure
  library call — no process execution, so #129 is not implicated.
- **Metrics**: the math label's bbox comes from the `DisplayList`
  width/height/depth — exact anchors, correct baseline alignment with
  neighboring plain labels.
- **IR/backend**: a math label becomes a measured group of glyph paths
  embedded at the label anchor (a new text-line kind carrying the rendered
  sub-SVG + metrics). PNG/PDF inherit it through resvg/svg2pdf unchanged.
- **Architecture**: keep `rpic-core` pure — define a small `MathRenderer`
  hook (measure + render) in core; the RaTeX implementation lives in
  `rpic-render` (or a new `rpic-math` crate) behind a cargo feature, on by
  default in CLI/binding builds.
- **WASM strategy**: feature stays **off** in the default `rpic-wasm` build
  (the ~400 KB budget). Since [#174] the npm package additionally ships a
  math-enabled artifact (`pkg/rpic_wasm_math_bg.wasm`, ~3 MB — RaTeX + the
  KaTeX glyph data): `rpic-wasm --features math` pulls `rpic-render` with
  `default-features = false, features = ["math"]` (no rasterizer) and
  registers the renderer at module init. Apps opt in lazily with
  `ready(wasmInput, { math: true })`, so the lean fast path is untouched.
  Post-hoc KaTeX in the host page is *not* equivalent — layout (`fit`,
  `textwid`, box sizing) needs the formula metrics before placement.

[#174]: https://github.com/milkway/rpic-lang/issues/174
- **Fallback**: if RaTeX fails to parse a `$…$` string, render it literally
  (today's behavior) and emit a `print`-style diagnostic — never fail the
  picture.
- **Escaping**: TeX commands take a **single** backslash in the pic source
  (`$\frac{T}{2}$`) — the lexer passes string backslashes through verbatim.
  `\\` in the pic source is therefore a TeX *line break*; binding examples
  must write `"$\\frac{…}$"` in host-language string literals, not
  `"$\\\\frac{…}$"` (which typesets a broken multi-line label — caught by
  visual QA of the R vignettes).

### Risks

- RaTeX is young (0.1.x, effectively one project) — mitigations: pin the
  version, feature-gate the dependency, keep the `MathRenderer` hook neutral
  so Typst/mitex can slot in if RaTeX stalls, and the literal fallback means
  regressions degrade gracefully.
- Known RaTeX gaps are DOM-only KaTeX extensions (`\htmlClass`,
  `\includegraphics`) — irrelevant to pic labels.
- Fonts add ~548 KB and glyph-path output makes label text unselectable in
  the SVG — same trade dvisvgm/Typst make; acceptable for diagram labels.

## On wasm

Status: shipped ([#174]; documented as the sanctioned path in [#449]).
RaTeX builds for `wasm32-unknown-unknown`, so the browser typesets `$…$`
labels with the same renderer, the same glyphs and the same metrics as the
native CLI. Nothing needs to be pre-typeset on a server or at build time.

### The three npm artifacts

`@strategicprojects/rpic` ships three builds of the same crate
(`crates/wasm`). Each is a separate glue module plus `.wasm`, and only the one
you ask for is fetched:

| Build | Files | Cargo feature | `.wasm` raw | gzip -9 |
| --- | --- | --- | --- | --- |
| lean (default) | `pkg/rpic_wasm.js`, `pkg/rpic_wasm_bg.wasm` | none | 495 KB (495,298 B) | 200 KB (199,804 B) |
| math | `pkg/rpic_wasm_math.js`, `pkg/rpic_wasm_math_bg.wasm` | `math` | 3.05 MB (3,048,333 B) | 1.15 MB (1,149,804 B) |
| geo | `pkg/rpic_wasm_geo.js`, `pkg/rpic_wasm_geo_bg.wasm` | `geo` | 716 KB (716,372 B) | 296 KB (295,851 B) |

Measured on v0.16.1 (`npm run build:wasm` in `bindings/js`: wasm-pack,
`opt-level = "z"`, LTO, `wasm-opt -Oz`). The math build costs about
+2.55 MB over lean (+0.95 MB gzipped). Nearly all of that is
RaTeX's layout tables and the embedded KaTeX glyph outlines. The rasterizer is
left out: the math build depends on `rpic-render` with
`default-features = false, features = ["math"]`.

### Loading it

```js
import * as rpic from '@strategicprojects/rpic';
await rpic.ready(undefined, { math: true }); // fetches pkg/rpic_wasm_math_bg.wasm
rpic.renderSvg('box "$T_1$" fit', { texlabels: true });
```

In Node, pass the artifact's bytes:
`ready(readFileSync(new URL('…/pkg/rpic_wasm_math_bg.wasm', import.meta.url)), { math: true })`.
The math build compiles everything the lean build does. It just also typesets.

### One build per module instance

`ready()` initializes one build per loaded instance of the JS module, and
the first call fixes which one:

- A later `ready()` asking for the **same** build returns the same promise.
  Concurrent callers share one init.
- A later `ready()` asking for a **different** build, while the first is in
  flight or after it succeeded, rejects with
  `rpic: ready() already initialized the lean build; cannot switch to math-enabled`
  (or the reverse). It never silently reuses the first build.
- `{ math: true, geo: true }` rejects immediately: the two extras are separate
  artifacts and cannot be combined.
- A **failed** init (fetch error, bad bytes) clears the slot, so `ready()` can
  be retried, with either build.

There is no in-place upgrade from lean to math. To change builds you need a
fresh module instance (in practice, a page reload).

### Lazy loading

Because the first `ready()` decides, choose the build **before** that call.
Look at the source you are about to compile: if any label is fully delimited
by `$…$` (and `texlabels` is on), load math. Otherwise load lean:

```js
const wantsMath = /"\s*\$[^"$]+\$\s*"/.test(src); // a "$…$" label
await rpic.ready(undefined, { math: wantsMath });
```

An editor whose source changes after the first load can't switch builds
later. It can either load math from the start when in doubt, or remember the
preference and reload. rpic Studio does both: it loads math when the opening
document has `$…$` or when the user asked for math before, and otherwise
stays lean until a reload.

### Embedding the renderer in your own wasm

A crate that builds its own wasm module on `rpic-core` (instead of using the
npm package) gets math the same way `crates/wasm` does: enable the renderer
behind a feature and register it once at module init.

```toml
[features]
math = ["dep:rpic-render"]

[dependencies]
rpic-core = "0.16"
rpic-render = { version = "0.16", default-features = false, features = ["math"], optional = true }
```

```rust
#[cfg(feature = "math")]
#[wasm_bindgen(start)]
pub fn init_math() {
    rpic_core::set_math_renderer(rpic_render::math::render_math);
}
```

This is exactly what [`crates/wasm/src/lib.rs`](../crates/wasm/src/lib.rs)
does. The hook is process-wide and first-call-wins (`set_math_renderer` returns
whether this call installed it). Register it before the first compile, since
labels compiled earlier have already fallen back. `default-features = false`
keeps resvg/svg2pdf out of the module. Then compile with `texlabels`
(`CompileOptions { texlabels: true, .. }`, or `texlabels = 1` in the source).

### When a build has no renderer

The lean and geo builds, and any embedder that never calls
`set_math_renderer`, still compile `texlabels` sources. Each `$…$` label is
drawn as **literal text**, dollars included (exactly what dpic does in SVG
mode), and the bundle's `diagnostics` gains one notice per label:

```
texlabels: no math renderer in this build; `$T_1$` kept literal
```

The picture never fails because of a math label. If an embedder sees this
notice, the fix is to load (or build) the math variant, not to change the
source.

[#449]: https://github.com/milkway/rpic-lang/issues/449

## Follow-up

- Implementation tracked in
  [#138](https://github.com/milkway/rpic-lang/issues/138) (`texlabels` +
  RaTeX behind a feature, metrics-driven bbox, fallback, corpus demo).
- Deferred, each behind its own future decision: `--latex`/dvisvgm CLI
  escape hatch; Typst backend. Math under wasm shipped via the opt-in
  `rpic_wasm_math` artifact ([#174]).
