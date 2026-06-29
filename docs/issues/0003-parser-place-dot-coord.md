# Issue: parser rejects `(place.x, expr)` coordinate pairs

**Labels:** bug, parser
**Status:** proposal / draft

## Problem

A coordinate pair whose component is a scalar drawn from a place's `.x`/`.y`
(or `.ht`/`.wid`/…) fails to parse:

```pic
A: box
"label" at (A.x, -0.55)     # error: expected Rparen, found DotX
```

Classic pic accepts this — `A.x` and `A.y` are numbers, so `(A.x, A.y - 0.55)`
is a normal `(expr, expr)` pair.

## Cause

In `crates/core/src/parser.rs`, `parse_position` decides between a *place*
location and an *expression* pair by lookahead: if the first token starts a place
(`Label`, `last`, `Here`, a corner, …) it commits to the place branch
(`parse_location_operand` → `parse_place`). `parse_place` stops at `.x`/`.y`
(those become `Expr::DotX/DotY` only inside `parse_primary`), so the trailing
`.x , …` is left unconsumed and the `,` / `)` check fails.

The ambiguity: a leading `Label` can begin **either** a point-valued place
(`A`, `A.ne`) **or** a scalar expression (`A.x`, `A.wid`).

## Possible fixes

1. **Lookahead for a scalar accessor.** When the position starts with a place,
   peek past the place tokens: if it is immediately followed by `.x` / `.y` /
   an attribute (`.ht` …), parse the whole component as an expression and take
   the expression-pair branch instead of the place branch.
2. **Unify on a backtracking/`pratt` parser** for positions that can yield a
   point or a scalar, resolving by what follows.
3. **Try-parse**: attempt the expression-pair branch first inside `(`,
   fall back to the location branch on failure.

Option 1 is the smallest, most targeted change.

## Worth doing?

Pros: it is idiomatic pic and currently forces a workaround (define a label for
the point, e.g. `L:(A.x,-0.55); "label" at L`). Cons: low severity (clean
workaround exists). Suggested priority: medium — nice correctness/ergonomics win
for hand-written diagrams and labels.

## Acceptance

`("t" at (A.x, A.y - 0.55))` and `(B.x, C.y)` parse and render; add parser +
eval tests.
