# Parity status

rpic was run over the upstream dpic example corpus (72 `.pic` files: the pic-manual
reproductions, the source diagnostics, and the doc figures). This page records the
outcome and the features still missing for full parity.

- **47** examples render faithfully and are included in this gallery.
- **7** more parse and render but are *library-definition* files with no drawing
  of their own (`Zalgebra`, `arrowmarks`, `plotlib`), or depend on features whose
  output is not yet meaningful (`arrows`, `man25` rely on
  features below), so they are excluded from the gallery.
- **18** exercise dpic-specific extensions rpic does not yet implement, grouped
  below.

## Included (rendered) — 47

**Manual figures (`manual/`, 34):** man01–man06, man08–man18, man20, man21, man22,
man24, man26, man28, man29, man30, man32, man33, man34, man45, man46, man47, man48,
man49, man50.

**Source diagnostics (`sources/`, 10):** basictests, diag1, diag2, diag3, diag5,
diag6, diag9, diagA, diagB, diagC.

**Doc figures (`doc/`, 2):** arrow, spline. **Shapes (`shapes/`, 1):** trellis.

## Not yet supported — 18

| Feature missing | Examples | Notes |
|---|---|---|
| File inclusion (`copy "file"`, `copy … thru`) | `Spiral`, `EscherCube`, `tgraph` | pulls another file's pic/data into the drawing |
| dpic macro-library metaprogramming (`$+` arg count, `exec`) | `dpictools` (×2), `arrowheads`, `arrowwide`, `quick` | the m4-style utility layer, not core pic |
| Runtime / recursive macro expansion (lazy `if`/default-arg idiom `"$1"==""`) | `man07`, `man19`, `man35`, `man36` | rpic expands macros before evaluation, so conditionally-recursive or empty-argument bodies can't be parsed lazily |
| Subscripted array variables (`P[i]`) | `trochoid` | array storage for variables |
| Sub-label anchor in `with` (`with .A.c at …`) | `Xtest`, `arcs` | anchoring a block by one of its own forward-referenced members |
| dpic predefined globals (`dpicopt`, `svg_font`, …) | `man31` | dpic preamble identifiers, not part of pic |
| Non-pic preamble / backend directives (TeX, PSTricks) | `diag8`, `circles` | `verbatimtex … etex`, `\global…` aimed at non-SVG backends |

## Features implemented for this corpus

Bringing the pass count from 32 to 54 (and curating 47) added, among others:

- inch unit suffix (`.5i`), bare-distance motions (`move 1`, `move -0.1`, `spline x`);
- direction followed by a `place.attr` distance (`line down G.ht`);
- the `frac <p1,p2>` interpolation shorthand;
- full position vector arithmetic with precedence (`(w,h)/2 + (xs,ys)/2`, `p*s`);
- block sub-labels (`B.A`, `last [].Outer.wid`);
- bare string-expression text objects (`sprintf(…) at …`);
- string equality in expressions (`"$1" == ""`) and `$n` substitution inside strings;
- embedded assignment expressions (`if (s = sin(i)) > 0.8 …`);
- `command`/`sh`/`exec` treated as no-ops for SVG output;
- bareword colours (`shaded Custom`), `%`-comment lines, and brace/newline tolerance
  in `define`/`for`/`if`.
