# Parity status

rpic was run over the upstream dpic example corpus (72 `.pic` files: the pic-manual
reproductions, the source diagnostics, and the doc figures). This page records the
outcome and the features still missing for full parity.

- **55** examples render faithfully and are included in this gallery (including
  `man31`, `man35`, and the `copy`-assembled 3-D `EscherCube`).
- A few more parse and render but are *library-definition* files with no drawing
  of their own (`Zalgebra`, `arrowmarks`, `plotlib`), or whose output is not yet
  faithful without the m4 layer (`arrows`, `man25`, `arrowheads` — the arrowhead
  *types* don't vary until arrays populate), so they are excluded from the gallery.
- **6** still exercise dpic-specific extensions rpic does not yet implement —
  all of dpic's m4 macro layer (see below).

Corpus pass rate is now **66 / 72** (was 32 at the first audit).

## Not yet supported — 6 (all [#24](https://github.com/milkway/rpic-lang/issues/24))

| Feature missing | Examples |
|---|---|
| `exec` run in the calling macro's argument scope (dynamic `$n` — populates arrays via `array(…)`) | `arrowwide`, `Spiral`, `tgraph`, `dpictools` (×2) |
| m4 token pasting (`$1$2` → one identifier) + PSTricks helpers | `circles` |
| `sh`-command shell escapes in the lexer | `dpictools` |

## Recently implemented

Pass count over this corpus rose **32 → 66**. Major features:

- **`{expr}th` ordinal counts as places** (`{i}th last box`, `` `n`th last circle ``):
  recovers `man35`.
- **dpic units & outer-label scope** ([#18](https://github.com/milkway/rpic-lang/issues/18)):
  absolute unit suffixes (`72bp__` == 1in, plus `pt__`/`mm__`/`cm__`/`in__`/`pc__`/`px__`)
  and read-only references to enclosing-scope labels from inside a block
  (`A:(0,0); [ line from A … ]`) — recovers `man31`.
- **Arrowhead sizing/type & `$+`/`(expr).x` primitives** (partial [#24](https://github.com/milkway/rpic-lang/issues/24)):
  arrowheads honour `arrowht`/`arrowwid` and `arrowhead=0` (open) vs `2` (filled);
  `$+` (macro argument count) and `.x`/`.y` on a parenthesised position
  (`($1-($2)).x`) are supported.
- **File inclusion** ([#14](https://github.com/milkway/rpic-lang/issues/14)):
  `copy "file"` splices another pic file relative to the source's directory
  (`EscherCube` + `libdp3D.pic`), including `copy`s reached only inside a taken
  `if` branch. Also fixed parenthesised between-fractions (`(X/Y) between A and B`)
  and parenthesised scalar coordinates (`((a*g)*cos t, …)`).
- **Lazy macro expansion** ([#13](https://github.com/milkway/rpic-lang/issues/13)):
  macros expand at evaluation time along the executed path — the default-argument
  idiom (`if "$1"=="" then … else …`) and **recursive macros** (`man36`'s binary
  tree) both work.
- **Block member anchors** ([#17](https://github.com/milkway/rpic-lang/issues/17)):
  `[ … ] with .A.c at P` and sub-label corners (`man07`, `Xtest`, `arcs`).
- **Subscripted array variables** ([#16](https://github.com/milkway/rpic-lang/issues/16)):
  `P[i] = …` (`trochoid`).
- **dpic SVG compatibility stubs** ([#18](https://github.com/milkway/rpic-lang/issues/18)):
  `svg_font(…)`, `dpicopt`, etc.
- **Non-pic backend preambles ignored** ([#19](https://github.com/milkway/rpic-lang/issues/19)):
  `verbatimtex … etex`, `\global…`, `\psset…` (`diag8`).
- inch unit suffix; bare-distance motions; direction + `place.attr` distance;
  `frac <p1,p2>` interpolation; full position vector arithmetic; block sub-labels
  (`B.A`); bare string-expression text objects; string equality + `$n` inside
  strings; embedded assignment expressions; `command`/`sh`/`exec` no-ops;
  bareword colours; `%`-comment lines; brace/newline tolerance in `define`/`for`/`if`.
