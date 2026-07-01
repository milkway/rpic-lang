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

## Adoption Matrix

| Decision | Candidate | Rationale |
| --- | --- | --- |
| Adopt | `margin` and side margin variables | High value, low semantic risk, fixes canvas framing without hidden geometry. Track in #107. |
| Maybe | `fit` attribute for text-sized objects | Useful, but text metrics are approximate and backend-sensitive. Needs a spec before implementation. Track in #108. |
| Maybe | `behind <object>` layering | Useful for highlights and backgrounds, but it touches render ordering, SVG ids, and animation references. Track in #109. |
| Maybe | Simple aliases such as `invisible`, `previous`, and `first` | Ergonomic and likely low risk, but aliases should be grouped and tested separately from parity work. |
| Maybe | New object types: `dot`, `diamond`, `oval`, `file`, `cylinder` | `dot` and `diamond` look tractable; `file`/`cylinder` need new geometry and anchor rules. Best handled as explicit extensions. |
| Maybe | Text styling: `bold`, `italic`, `mono`, `big`, `small`, `aligned` | Good SVG-era ergonomics, but requires IR/text model changes and careful fallback behavior. |
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
