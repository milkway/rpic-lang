# SVG Styles, Classes, and Gradients — Research Notes

Status: design evaluation for issue #116.
Date: 2026-07-02.

These notes evaluate how much visual-styling surface rpic should expose beyond
the pic/dpic attributes it already supports. The stance is unchanged:
**Kernighan-first, dpic as practical oracle**. Everything below is an explicit
rpic extension — opt-in, documented, and inert for classic input.

## Philosophy Anchors

Every decision here follows rules the project has already committed to:

1. **Explicit composition over hidden effects.** pic gives primitives and the
   author composes; the engine does not apply invisible render magic. This is
   why automatic label masking was rejected in favor of the documented swatch
   idiom (#126).
2. **No raw text injection.** The #129 policy: unvalidated raw snippets never
   ride into the structured SVG. Its rationale applies with equal force to raw
   CSS from `.pic` sources.
3. **Backend-stable output.** SVG is the source of truth; PNG/PDF rasterize
   from the same tree (resvg/svg2pdf). Anything adopted must render
   identically offline and deterministically.
4. **Extensions are contextual attributes with defaults that no-op.** The
   established shape: `hatch`/`hatchangle`/… (#124), `opacity` (#127),
   `fit` (#121), `behind` (#119), `close` (#130). Same recipe here.
5. **Delegation layers are additive metadata.** The `animate` layer emits
   metadata keyed to stable `s<N>` ids and lets GSAP do the work. Styling
   hooks can delegate the same way — to the host document's CSS.

## Finding: Named Styles Already Exist — They Are Macros

pic's own reuse mechanism covers the "style `warn`" use case with no new
syntax. Verified against the current build:

```pic
.PS
define warn { outlined "red" dashed thick 1.2 }
define note { shaded "lightyellow" outlined "gray" }
box warn() "error"
box note() "notice"
circle warn() note() "both compose"
.PE
```

Macro calls expand inline to attribute tokens, so they parameterize
(`define sev { outlined $1 thick $2 }`), compose left-to-right, and stay
100% classic pic. **Decision: document the idiom; do not add a `style`
keyword.** A named-style feature would duplicate `define` with less power.

## Proposal 1 — `class` Hooks (adopt)

A contextual attribute that attaches CSS class names to a shape's existing
`<g id="sN">` group:

```pic
.PS
box class "critical" "payment"
arrow class "critical dataflow"
.PE
```

emitting `<g id="s0" class="critical">…`. Semantics:

- **Inert by itself.** A class changes nothing in rpic's own rendering; SVG,
  PNG, and PDF output are visually identical with or without it. Styling
  happens only when the *host document* that embeds the SVG provides CSS —
  the same delegation contract as `animate`/GSAP.
- **Validated, not raw.** Class names are restricted to
  `[A-Za-z0-9_-]` and spaces (multiple classes), rejected otherwise. No
  attribute-injection surface, nothing to escape creatively.
- **No id collision.** The internal `s<N>` ids stay untouched and remain the
  GSAP/animation targets. A user-facing `id "name"` attribute is deliberately
  **out of scope** for the first pass: uniqueness validation across blocks
  and macro expansion has real edge cases, and `class` covers the styling and
  JS-hook use cases. Revisit only with a concrete need.
- IR: `Style.class: Option<String>`; SVG backend appends the attribute to the
  shape group. Nothing else changes.

This is the cheapest possible bridge to "themes and CSS variables": rpic
never learns CSS; the embedding page owns it, where CSS already lives.

## Proposal 2 — Linear Gradients (adopt, small surface)

A structured fill attribute, following the hatch recipe (contextual keywords,
`<defs>` machinery, per-use pattern ids):

```pic
.PS
box gradient "steelblue" "white"
circle gradient "gold" "orangered" gradientangle 45
.PE
```

- `gradient "<from>" "<to>"` — two color stops, same color grammar as
  `outlined`/`shaded`/`hatchcolor`.
- `gradientangle <expr>` — degrees in pic coordinates, default `0`
  (left-to-right); `90` is bottom-to-top, matching how `hatchangle` measures.
- Emits `<defs><linearGradient id="gradN" …>` with `gradientUnits`
  = `objectBoundingBox`, and `fill="url(#gradN)"` on the shape — the exact
  `next_pattern` id scheme hatch already uses.
- **Backend-stable**: `linearGradient` is core SVG 1.1 static; resvg and
  svg2pdf both support it, so PNG/PDF match the SVG (the implementation PR
  must include render tests, as hatch did).
- Interactions kept simple in the first pass: `gradient` occupies the fill
  slot (`fill`/`shaded` on the same object is an error, or last-wins matching
  existing attribute precedence); combining with `hatch` (gradient as pattern
  background) and `opacity` composes naturally since both already act on the
  fill attribute. Closed shapes and `fill_open` paths only.
- **Deferred**: multi-stop lists, `radialgradient`, per-stop opacity,
  userSpace coordinates. Each is additive later; none is needed to prove the
  feature.

PSTricks precedent: `fillstyle=gradient` with `gradbegin`/`gradend`/
`gradangle` — same two-color + angle shape as proposed here (credit noted in
`docs/pstricks.md` when implemented).

## Proposal 3 — Raw CSS / `<style>` Blocks (do not adopt)

Rejected for `.pic` sources, by direct application of the #129 rationale:

- Raw CSS is raw backend text. Injecting it from picture sources would break
  the guarantee that structured output cannot be corrupted by input.
- resvg implements a limited CSS subset; a stylesheet that looks fine in the
  browser silently diverges in PNG/PDF. That violates backend stability —
  the exact failure mode the policy exists to prevent.
- Escaping and `</style>` breakouts create a real injection surface for
  rendered-on-server use cases (playground, bindings).

The `class` hook is the sanctioned path: the *host document* styles the SVG,
so CSS stays where CSS is testable and rpic output stays deterministic. If a
future need arises for self-contained themed SVG files, the least-bad design
is a **CLI-level** `--css <file>` flag — styling chosen at render invocation
by the user, never by the `.pic` source — but that is explicitly deferred
until a concrete use case exists.

## Adoption Matrix

| Decision | Candidate | Rationale |
| --- | --- | --- |
| Adopt | `class "name"` hook on shape groups | Inert, validated, delegates theming to the host page like `animate` delegates motion. Smallest useful surface. |
| Adopt | `gradient "a" "b"` + `gradientangle` | Structured fill in the hatch mold; core SVG 1.1, offline-stable in resvg/svg2pdf. |
| Idiom (no feature) | Named styles / `style "warn"` | Already expressible: `define warn { outlined "red" … }` + `box warn()`. Document, don't duplicate `define`. |
| Defer | User-facing `id "name"` | Uniqueness/collision semantics need a concrete driving use case; `class` covers styling and JS hooks. |
| Defer | Multi-stop / radial gradients, CSS variables, themes, `--css` CLI flag | Additive later; each needs its own justification. |
| Do not adopt | Raw CSS or `<style>` from `.pic` sources | Violates #129 (raw injection), backend stability (resvg CSS subset), and opens escaping/injection surface. |

## Follow-up Issues

- `class` hook implementation — tracked in
  [#133](https://github.com/milkway/rpic-lang/issues/133).
- Linear gradient implementation — tracked in
  [#134](https://github.com/milkway/rpic-lang/issues/134).

Each implementation PR must keep the established extension contract: byte-for-
byte dpic-compatible output when the attribute is absent, contextual keywords
that still work as variable names, docs in `docs/extensions.md`, and SVG +
render tests.
