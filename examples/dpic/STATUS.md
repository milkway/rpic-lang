# Parity Status

This directory is the checked-in, curated dpic example corpus used by rpic's
local parity checks. It currently contains **56** `.pic` files: **55 figures**
(manual reproductions, source diagnostics, 3-D projection examples, and shape
demos) plus `3d/libdp3D.pic`, the 3-D support library that `EscherCube.pic`
copies.

Current compile/render status:

| Corpus | Command | Status |
|---|---|---|
| `examples/dpic/**/*.pic` | `./target/debug/rpic --svg <file>` | **56 / 56 pass** |
| `examples/dpic/**/*.pic` where local `dpic -v` succeeds | element-count + `stroke-dasharray` parity | **54 / 54 match** |

The latest pass was run after restoring several classic pic semantics from
Brian W. Kernighan's paper/manual:

- `scale` keeps dimensional variables in user units and converts geometry to
  inches at use sites.
- `[ ... ]` block assignments to variables and environment parameters are local.
- Unknown variables are errors instead of implicit zeroes.
- Standalone text occupies the invisible `textwid` by `n * textht` box.
- `same`, `chop r1 chop r2` (including negative chops that extend endpoints),
  dash lengths, `linethick`, and arbitrary-delimiter `define` bodies follow
  dpic/classic behavior more closely.
- `rand(seed)` is the practical oracle for deterministic random examples;
  unseeded `dpic rand()` is initialized from `time()` and is not a stable
  visual-parity target by itself.
- SVG output now follows dpic more closely for two-point lines, open-object
  fills (`line`/`spline`/`arc`), arc arrowheads, stroke-aware picture sizing,
  block-attached text, `textoffset` on left/right-justified text, scaled
  arrowhead/dash metadata on already-emitted geometry, `move` geometry in output
  bounds, compass anchors on circles/ellipses, and text extents that enlarge
  only the rendered bbox, not the geometric bbox used by block anchors such as
  `last [].s`. Block placement also follows dpic for coordinate-pair anchors such as
  `[ ... ] with (0,0) at P`, where the pair names a local block coordinate.
  Standalone text objects honor explicit `wid`/`ht` bounds for their rendered
  bbox instead of deriving the bbox from the literal string length, and their
  `above`/`below` offsets follow dpic's SVG baseline placement.

The `svg_font(...)` backend helper is intentionally a no-op in rpic, so bare font
names such as `monospace` are accepted without variable lookup.

## Known dpic SVG Quirk: `manual/man50`

`manual/man50.pic` is intentionally kept faithful to the dpic source corpus.
After the parity fixes in this branch history, rpic's SVG geometry matches
`dpic -v` for this file, including a visible quirk: both SVGs clip the top of
the thick red circle.

This is a backend framing issue inherited from dpic's SVG output, not a separate
rpic geometry bug. With the original `.PS 3.5` input, `dpic -v` emits a viewBox
that is too tight for the circle's stroke width, so the painted stroke extends
above the SVG canvas. The same dpic input rendered through PostScript (`dpic -r`)
uses a high-resolution bounding box that does not show the same top clipping.

The corpus does not add a compensating invisible move or rpic-only `margin`
variable to `manual/man50.pic`, because these examples are the dpic oracle set:
their value is that classic pic input keeps the same meaning and output shape
under rpic and dpic. For presentation-oriented rpic documents outside this
oracle corpus, prefer the documented canvas margin extension or an explicit
geometry margin when extra framing is desired.

Two files sit outside the dpic-eligible set in the table above, for different
reasons. `manual/man31.pic` genuinely fails under `dpic -v`: its
`svg_font(...)` guard errors at parse time. `3d/EscherCube.pic` is not a dpic
failure at all: dpic resolves `copy "libdp3D.pic"` relative to the working
directory, not the source file, so it compiles cleanly when dpic is invoked
from `3d/` — and, run that way, it matches rpic on the parity metric
(element counts and `stroke-dasharray`: 11 polylines + 45 lines, no dashes,
both sides; checked against dpic 2025.08.01). The parity harness runs from
the repository root, which is why the table counts 54; running the oracle
per-directory would make it 55. Both files remain covered by the rpic render
pass above.

Credits: the language and original examples trace back to **Brian W.
Kernighan**'s pic; the reference implementation and this corpus come from
**Dwight (J. D.) Aplevich**'s dpic work; rpic also acknowledges **D. Richard
Hipp**'s pikchr as a modern SVG-first influence. See the top-level
`ACKNOWLEDGMENTS.md`.
