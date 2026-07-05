# Pikchr Research Notes

Status: research for [#106](https://github.com/milkway/rpic-lang/issues/106).
Date: 2026-07-01.

These notes summarize a small source-and-documentation pass over Pikchr, by
D. Richard Hipp, to identify ideas that are useful for rpic without weakening
the project stance: **Kernighan-first, dpic as practical oracle**.

Sources inspected:

- Pikchr project home: <https://pikchr.org/home>
- Pikchr download notes: <https://pikchr.org/home/doc/trunk/doc/download.md>
- Pikchr differences from pic: <https://pikchr.org/home/doc/trunk/doc/differences.md>
- Pikchr user manual: <https://pikchr.org/home/doc/trunk/doc/userman.md>
- Pikchr trunk source, especially `pikchr.y`: <https://pikchr.org/home/raw/pikchr.y?ci=trunk>

The Pikchr source was downloaded to `/private/tmp` for inspection only; no code
is vendored or copied into rpic.

## Stance

Pikchr is not the oracle for classic pic semantics. For classic programs, rpic
continues to follow Brian Kernighan's language model and use dpic as the
practical executable oracle. Pikchr is valuable as a modern pic-family design
reference for explicit rpic extensions: features should be opt-in, documented,
and default to zero behavioral change for dpic-compatible input.

## Margin

Pikchr does not implement `margin` as a drawing command. It treats `margin`,
`topmargin`, `rightmargin`, `bottommargin`, and `leftmargin` as variables that
are consulted during final SVG layout. In `pik_render()`, Pikchr computes the
object bounding box, expands that box by the margin variables, then derives the
SVG canvas from the expanded box. Object coordinates, labels, anchors, and
subsequent geometry are not changed by those variables.

That maps well to rpic as an explicit extension because it controls only the
final canvas. It is a cleaner solution for cases like `manual/man50`, where both
dpic and rpic can produce visually matching output that is still clipped by the
inherited canvas policy.

Recommended rpic behavior:

- Add environment variables `margin`, `topmargin`, `rightmargin`,
  `bottommargin`, and `leftmargin`, all defaulting to 0.
- Apply `margin` to all four sides, then add the side-specific margin.
- Expand the native output canvas only; do not alter shape coordinates,
  ordinals, labels, anchors, current point, or the geometry bbox used by pic
  semantics.
- Preserve current dpic-compatible output byte-for-byte when all margin
  variables are 0.
- Treat margin values as pic dimensions that interact with `scale`, `.PS`
  sizing, and page clamping like visible geometry.
- Consider allowing negative side margins, as Pikchr does, while guarding
  against a non-positive final canvas.

Implementation is tracked in
[#107](https://github.com/milkway/rpic-lang/issues/107).

## Behind

Pikchr implements `behind <object>` as a render-layer adjustment. The current
object keeps its normal semantic position, but when its layer is not already
below the referenced object, Pikchr lowers it to one layer behind that target.

That maps well to rpic as an explicit extension if the implementation keeps
source/evaluation order separate from backend paint order:

- Add `behind <object>` as a contextual object attribute, not a global reserved
  keyword, so existing variables named `behind` can still parse normally.
- Store a render layer per shape in the evaluated IR.
- Sort only the backend emission order by layer, keeping original shape indices
  stable for ids such as `s0`/`s1` and animation targets.
- Keep labels, anchors, ordinals, `last`, and object placement based on normal
  pic evaluation order.
- Preserve dpic-compatible output order when no object uses `behind`.

Implementation is tracked in
[#109](https://github.com/milkway/rpic-lang/issues/109).

## Fit

Pikchr implements `fit` as an attribute on closed objects. In `pikchr.y`, the
attribute looks only at text declared before `fit`, estimates the text box from
the current character metrics, and asks the object type to resize itself. Text
declared later remains normal attached text, but it is not part of the fit
calculation.

That maps to rpic as a useful explicit extension if it stays conservative:

- keep `fit` contextual, so ordinary variables named `fit` still work in
  expression positions and on non-fitted object types;
- use rpic's existing text-bbox estimate instead of adding a second text metric
  model;
- apply it only to `box`, `ellipse`, and `circle` at first;
- preserve explicit dimensions so an author can combine `fit` with fixed
  `wid`, `ht`, `rad`, or `diam` without hidden overrides;
- keep every classic pic program unchanged when `fit` is absent.

Implementation is tracked in
[#108](https://github.com/milkway/rpic-lang/issues/108).

## Adoption Matrix

| Decision | Candidate | Rationale |
| --- | --- | --- |
| Adopt | `margin` and side margin variables | High value, low semantic risk, fixes canvas framing without hidden geometry. Track in #107. |
| Adopt | `behind <object>` layering | Useful for highlights and backgrounds when implemented as backend paint-order metadata with stable semantic ids. Track in #109. |
| Adopt | `fit` attribute for text-sized objects | Useful when opt-in and constrained to rpic's own text-bbox estimate. Track in #108. |
| Adopt | `close` attribute for line polygons | Useful for filled or hatched polygons when contextual and documented as an rpic extension, not dpic parity. |
| Maybe | Simple aliases such as `invisible`, `previous`, and `first` | Ergonomic and likely low risk, but aliases should be grouped and tested separately from parity work. |
| Adopt | `dot` object | Implemented as a contextual primitive with `dotrad` (#153): the native form of the circuit-macros junction idiom. |
| Maybe | New object types: `diamond`, `oval`, `file`, `cylinder` | `diamond` looks tractable; `file`/`cylinder` need new geometry and anchor rules. Best handled as explicit extensions. |
| **Adopted (v0.7)** | Text styling: `bold`, `italic`, `mono` (+ rpic's `font "…"`/`fontsize n`) | Per-string attributes binding like `ljust`; styled metrics feed `fit`/bboxes; PNG/PDF embed the styled Go faces. `big`/`small` are covered by `fontsize`; `aligned` still open. |
| Maybe | Path conveniences: `go ... heading`, `until even with`, `same as <object>` | Nice authoring improvements, but parser/evaluator surface is larger. Should wait until parity regressions are quiet. |
| Do not adopt | Pikchr omissions of classic pic features (`copy`, `for`, `if`, `sprintf`, `sh`, block scoping changes) | rpic's compatibility target is classic pic/dpic. Security or simplicity choices in Pikchr must not remove supported pic semantics. |
| Do not adopt | Pikchr arc approximation as a replacement for dpic arc behavior | rpic should keep dpic-compatible arc geometry; Pikchr explicitly treats legacy arc semantics as obscure and approximates them. |
| Do not adopt | Silent changes to default placement, scale, or block-variable semantics | These would break the Kernighan/dpic contract and make visual parity harder to reason about. |

## Notes For Future Work

When implementing any Pikchr-inspired feature, document it under an "rpic
extension" heading. The wording should make the relationship explicit:
Kernighan/dpic remain the default semantic target, and Pikchr is credited as the
source of a modern ergonomic idea.

Tests should separate two cases:

- Existing dpic-compatible programs render exactly as before when the extension
  is not used.
- A small extension-specific example demonstrates the new behavior visually and
  with an SVG/backend regression where practical.
