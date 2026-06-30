# Parity Status

This directory is the checked-in, curated dpic example corpus used by rpic's
local parity checks. It currently contains **56** `.pic` files: manual
reproductions, source diagnostics, 3-D projection examples, and shape demos.

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
- `same`, `chop r1 chop r2`, dash lengths, `linethick`, `rand()`, and
  arbitrary-delimiter `define` bodies follow dpic/classic behavior more closely.
- SVG output now follows dpic more closely for two-point lines, open-object
  fills (`line`/`spline`/`arc`), arc arrowheads, stroke-aware picture sizing,
  block-attached text, and compass anchors on circles/ellipses.

The `svg_font(...)` backend helper is intentionally a no-op in rpic, so bare font
names such as `monospace` are accepted without variable lookup.

In this checkout, `dpic -v` itself fails on `3d/EscherCube.pic` and
`manual/man31.pic`; those remain covered by the rpic render pass above.

Credits: the language and original examples trace back to **Brian W.
Kernighan**'s pic; the reference implementation and this corpus come from
**Dwight (J. D.) Aplevich**'s dpic work; rpic also acknowledges **D. Richard
Hipp**'s pikchr as a modern SVG-first influence. See the top-level
`ACKNOWLEDGMENTS.md`.
