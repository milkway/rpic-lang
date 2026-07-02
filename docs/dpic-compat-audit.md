# dpic Compatibility Audit

Status: working notes for issue #128; raw backend policy decided in #129.

rpic remains Kernighan-first, with dpic as the practical oracle. This document
tracks dpic language and backend features that are easy to confuse with rpic
extensions, especially around color, fill, backend command snippets, and path
closure.

## Current Findings

| Feature | dpic source/behavior | rpic status | Decision |
| --- | --- | --- | --- |
| `shaded "color"` | Native color attribute in `dpic.y` (`object Xcolrspec stringexpr`). SVG emits `fill="..."` for closed objects and filled linear objects. | Supported. For open objects, `shaded` sets `fill_open`. | Keep as compatibility feature. |
| `outlined "color"` | Native color attribute. SVG uses the string as stroke color where the object can be outlined. | Supported. | Keep as compatibility feature. |
| `colored "color"` / `color "color"` | dpic stores both shade and outline for objects that support both. For open lines, the practical SVG behavior is stroke/arrowhead color, not a filled area. | Supported. rpic records fill and stroke but does not enable open-area fill for `colored`, matching dpic's line behavior. | Keep as compatibility feature. |
| `rgbstring(r,g,b)` | Not a dpic lexer token or builtin. It is defined in `examples/sources/dpictools.pic`, with an SVG branch producing `rgb(R,G,B)`. | Supported: static backend `if dpicopt == optSVG` folds before parsing, so copied `dpictools.pic` can define `rgbstring` before `shaded rgbstring(...)` is parsed. | Compatibility fix, not an rpic extension. |
| backend option strings in `shaded` | dpic passes backend-specific strings through to PGF/PSTricks/etc. For SVG, strings are color values. | SVG color strings are passed through. TikZ/PSTricks option lists are not parsed into native SVG style. | Leave backend-option parsing to #116 unless a dpic SVG parity bug appears. |
| `command "..."` | Native dpic element that emits raw backend text. dpic examples use it for TeX/PSTricks/SVG snippets. | Recognized and skipped as a silent no-op; raw text is never injected into the output. | **Policy decided (#129)**: skip-only is the permanent behavior. See [Raw backend policy](#raw-backend-policy). |
| `sh "..."` | Native dpic shell escape. | Recognized and skipped as a silent no-op; never executed. | **Policy decided (#129)**: never executed, not even behind a flag. See [Raw backend policy](#raw-backend-policy). |
| `close` path command | Not a dpic command in 2025.08.01. `line right then up close` reports `close` as a missing variable. | Implemented as an explicit rpic extension on `line`, inspired by Pikchr. It remains contextual so classic variable uses still work. | Keep documented outside dpic parity. Use explicit final segments for classic/dpic-compatible input. |
| `xslanted` / `yslanted` | Not native tokens in dpic 2025.08.01. `dpictools.pic` provides a `slantbox(wid,ht,xslant,yslant,attributes)` macro. | No native attributes. Macro-style slanted boxes work when written as paths/macros. | Do not add native attributes under dpic parity. Revisit only as an explicit extension. |
| `opacity <expr>` | Not a dpic core attribute. Some dpic docs show backend-specific strings such as PGF `shaded "orange, opacity=0.5"`. | rpic extension from #118: fill-only opacity via SVG `fill-opacity`. | Keep separate from `shaded` color strings; broader style syntax belongs to #116. |

## Verified Cases

### dpictools `rgbstring`

Input:

```pic
.PS
copy "/private/tmp/dpic-2025.08.01/examples/sources/dpictools.pic"
circle shaded rgbstring(1,0.84,0) outlined "black"
.PE
```

dpic 2025.08.01 emits SVG with:

```svg
<circle fill="rgb(255,214,0)" stroke="black" ... />
```

rpic now emits:

```svg
<circle ... fill="rgb(255,214,0)" stroke="black" ... />
```

The key fix is not a special `rgbstring` builtin. It is static folding of the
dpic backend guard `if dpicopt == optSVG then { ... }`, allowing the copied
macro definition to exist before the following object is parsed.

### dpic Has No `close`

Input:

```pic
.PS
line right then up close shaded "yellow"
.PE
```

dpic 2025.08.01 reports `Variable not found` / `Search failure for "close"`.
This is not a dpic parity feature. rpic implements `close` only as an explicit
extension on `line`, following Pikchr's polygon idiom. Polygonal paths that
must remain classic/dpic-compatible should still be closed in pic style:

```pic
line from A to B then to C then to A shaded "yellow"
```

## Raw Backend Policy

Decided in #129. rpic treats dpic's raw backend directives as **tolerated,
silent no-ops**:

- **`sh "..."` is never executed** — permanently, and not behind any flag. A
  picture-description language has no business running a shell; rendering a
  `.pic` file from an untrusted source must always be safe. The directive is
  tolerated (not an error) because dpic corpus sources use it
  (`manual/man19`, `manual/man31`, `sources/basictests`, `sources/arcs`,
  `sources/diag6`) and only need it to parse, not to run.
- **`command "..."` raw text is never injected** into the SVG/PNG/PDF output.
  rpic's SVG is structured — stable `s<N>` shape ids drive the GSAP animation
  layer, and PNG/PDF rasterize from that same tree — so unvalidated raw
  snippets could silently corrupt every backend. dpic uses `command` mostly
  for TeX/PSTricks lines that are meaningless in rpic's native SVG anyway.
- Both parse as true no-ops: no shapes, no diagnostics, no effect on the
  current position or direction. Geometry flows across them unchanged.
- Styling needs that dpic solved with raw `command` snippets belong to the
  structured style extension work (#116); TeX/LaTeX labels belong to #115 as
  their own layer. If #116 ever leaves a real gap, a raw-SVG escape hatch
  would need its own explicit, opt-in design — it must not ride in through
  dpic parity.

## Follow-up Candidates

| Candidate | Priority | Notes |
| --- | --- | --- |
| Raw backend `command`/`sh` parity policy | Done | Policy decided in #129: permanent skip-only no-ops, `sh` never executed. Documented above. |
| Style/CSS/gradients | High | Track in #116. Should not overload dpic `shaded` color strings unless the syntax is explicit and backend-stable. |
| TeX/LaTeX labels | Medium | Track in #115. Backend raw commands and TeX labels interact, but should remain separate from geometry parity. |
| Native `close` extension | Done | Implemented as a contextual `line` attribute inspired by Pikchr; documented in `docs/extensions.md`. |
| Native slanted box attributes | Low | Prefer macro/path idioms unless a small explicit extension is justified. |
