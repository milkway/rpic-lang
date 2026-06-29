# Parity status

rpic was run over the upstream dpic example corpus (72 `.pic` files: the pic-manual
reproductions, the source diagnostics, and the doc figures). This page records the
outcome and the features still missing for full parity.

- **49** examples render faithfully and are included in this gallery.
- **7** more parse and render but are *library-definition* files with no drawing
  of their own (`Zalgebra`, `arrowmarks`, `plotlib`), or whose output is not yet
  meaningful (`arrows`, `man25` rely on features listed below), so they are
  excluded from the gallery.
- **16** exercise dpic-specific extensions rpic does not yet implement, grouped
  below.

## Included (rendered) — 49

**Manual figures (`manual/`, 36):** man01–man06, man08–man22, man24, man26,
man28–man30, man32–man34, man45–man50 (incl. the recursive binary tree `man36`
and the 3-D `slantbox` block `man19`).

**Source diagnostics (`sources/`, 10):** basictests, diag1, diag2, diag3, diag5,
diag6, diag9, diagA, diagB, diagC.

**Doc figures (`doc/`, 2):** arrow, spline. **Shapes (`shapes/`, 1):** trellis.

## Not yet supported — 16

| Feature missing | Examples | Tracking |
|---|---|---|
| File inclusion (`copy "file"`, `copy … thru`) | `Spiral`, `EscherCube`, `tgraph` | [#14](https://github.com/milkway/rpic-lang/issues/14) |
| dpic macro-library metaprogramming (`$+` arg count, `exec`) | `dpictools` (×2), `arrowheads`, `arrowwide`, `quick` | [#15](https://github.com/milkway/rpic-lang/issues/15) |
| Sub-label anchor in `with` (`with .A.c at …`) | `Xtest`, `arcs`, `man07` | [#17](https://github.com/milkway/rpic-lang/issues/17) |
| Subscripted array variables (`P[i]`) | `trochoid` | [#16](https://github.com/milkway/rpic-lang/issues/16) |
| Macro argument used in an expression slot (`{i}th` ncount) | `man35` | [#13](https://github.com/milkway/rpic-lang/issues/13) |
| dpic predefined globals (`dpicopt`, `svg_font`, …) | `man31` | [#18](https://github.com/milkway/rpic-lang/issues/18) |
| Non-pic preamble / backend directives (TeX, PSTricks) | `diag8`, `circles` | [#19](https://github.com/milkway/rpic-lang/issues/19) |

## Recently implemented

The pass count over this corpus rose from **32 → 56** (49 curated). Highlights:

- **Lazy macro expansion** ([#13](https://github.com/milkway/rpic-lang/issues/13)):
  macros now expand at evaluation time along the executed path, so the
  default-argument idiom (`if "$1"=="" then … else …`) and **recursive macros**
  (`man36`'s binary tree) work — textual pre-expansion could do neither.
- inch unit suffix (`.5i`); bare-distance motions (`move 1`, `move -0.1`, `spline x`);
- direction followed by a `place.attr` distance (`line down G.ht`);
- the `frac <p1,p2>` interpolation shorthand;
- full position vector arithmetic with precedence (`(w,h)/2 + (xs,ys)/2`, `p*s`);
- block sub-labels (`B.A`, `last [].Outer.wid`);
- bare string-expression text objects (`sprintf(…) at …`);
- string equality in expressions (`"$1" == ""`) and `$n` substitution inside strings;
- embedded assignment expressions (`if (s = sin(i)) > 0.8 …`);
- `command`/`sh`/`exec` treated as no-ops for SVG output;
- bareword colours (`shaded Custom`), `%`-comment lines, brace/newline tolerance
  in `define`/`for`/`if`, and uppercase macro names.

> Note: parity issues [#14](https://github.com/milkway/rpic-lang/issues/14),
> [#16](https://github.com/milkway/rpic-lang/issues/16),
> [#17](https://github.com/milkway/rpic-lang/issues/17),
> [#18](https://github.com/milkway/rpic-lang/issues/18) and
> [#19](https://github.com/milkway/rpic-lang/issues/19) are being addressed on
> separate branches and will raise these numbers further once merged.
