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

## Follow-up

- Implementation tracked in
  [#138](https://github.com/milkway/rpic-lang/issues/138) (`texlabels` +
  RaTeX behind a feature, metrics-driven bbox, fallback, corpus demo).
- Deferred, each behind its own future decision: `--latex`/dvisvgm CLI
  escape hatch; Typst backend. Math under wasm shipped via the opt-in
  `rpic_wasm_math` artifact ([#174]).
