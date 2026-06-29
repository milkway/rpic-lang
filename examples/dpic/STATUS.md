# Parity status

rpic was run over the upstream dpic example corpus (72 `.pic` files: the pic-manual
reproductions, the source diagnostics, and the doc figures). This page records the
outcome and the features still missing for full parity.

- **53** examples render faithfully and are included in this gallery (including
  the `copy`-assembled 3-D `EscherCube`).
- A few more parse and render but are *library-definition* files with no drawing
  of their own (`Zalgebra`, `arrowmarks`, `plotlib`), or whose output is not yet
  meaningful (`arrows`, `man25`), so they are excluded from the gallery.
- **9** still exercise dpic-specific extensions rpic does not yet implement,
  grouped below.

Corpus pass rate is now **63 / 72** (was 32 at the first audit).

## Not yet supported — 9

| Feature missing | Examples | Tracking |
|---|---|---|
| dpic macro-library metaprogramming (`$+` arg count, `exec`) — also pulled in by `copy` | `dpictools` (×2), `arrowheads`, `arrowwide`, `Spiral`, `tgraph` | [#15](https://github.com/milkway/rpic-lang/issues/15) |
| PSTricks helper macros (`lozenge`, `\dpicshdraw`) | `circles` | [#15](https://github.com/milkway/rpic-lang/issues/15) |
| dpic unit-suffixed numbers (`11bp__`) | `man31` | [#18](https://github.com/milkway/rpic-lang/issues/18) (partial) |
| Macro argument used in an expression slot (`{i}th` ncount) | `man35` | [#13](https://github.com/milkway/rpic-lang/issues/13) |

## Recently implemented

Pass count over this corpus rose **32 → 63**. Major features:

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
