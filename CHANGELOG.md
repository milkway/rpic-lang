# Changelog

All notable changes to **rpic** are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each release is also archived on Zenodo under the concept DOI
[10.5281/zenodo.21209915](https://doi.org/10.5281/zenodo.21209915), which always
resolves to the latest version.

## [Unreleased]

### Added

- **npm: standalone player module (`@strategicprojects/rpic/player`).** The
  GSAP player (`animate`/`interactive` + helpers) moved to `player.js`, a
  **zero-import** module — it never touches the wasm compiler, so a page with
  a pre-rendered SVG (`rpic --json` on the CLI) can import just the player
  (bundler subpath `…/player`, or `…/player.js` straight from a CDN) and
  fetch nothing wasm-related. `index.js` re-exports it, so the existing
  `import { animate } from '@strategicprojects/rpic'` is unchanged; types
  split likewise (`player.d.ts`, re-exported by `index.d.ts`). A smoke-test
  guard asserts the module stays import-free. (#356)

- **`link "<url>"` — clickable hyperlinks on objects.** A new contextual
  extension with the same two forms as `class`: an inline attribute
  (`box "docs" link "https://rpic.dev"`) and a statement reaching labels,
  ordinals and macro-drawn shapes (`link A "https://…"`). The SVG backend
  wraps the object's `<g id="sN">` group in `<a href="…">` — label included,
  stable ids untouched, so `class`/`animate` compose on the same shape.
  Re-applying replaces (last one wins). URLs are validated (non-empty, no
  whitespace/control characters, no `javascript:`/`vbscript:`/`data:`
  schemes) so hosts rendering untrusted pictures gain no XSS surface. The
  compile bundle's `objects` entries carry a `"link"` key when set. The anchor
  carries `class="rpic-link"`, opting out of `a:not([class])` host prose CSS
  (no underline/colour leaking into labels) and doubling as a styling hook
  (#362). SVG-only:
  PNG/PDF render the identical picture without the link (usvg flattens the
  anchor; verified pixel-identical). Unused, every output is byte-identical —
  corpus 124/124 for both `--svg` and `--json`. (#358)

## [0.10.0] — 2026-07-10

### Added

- **`animate … with "draw" from <p> to <p>` — reveal a stroke sub-segment.** The
  `draw` effect gains two optional clauses that narrow the trace to any window of
  the stroke, given as a fraction (`60%` or the bare `0.6`); absent ends default
  to 0 and 1. Each rides the manifest only when set (`"drawFrom":0.4,
  "drawTo":0.6`, clamped to `[0,1]`), so a plain `draw` stays byte-identical.
  Both players (npm `index.js`, site `AnimatedPic.astro`) realise the window with
  a pure `stroke-dasharray` sweep — no extra GSAP plugin. `out` retracts the
  window. For `draw`, `from`/`to` mean the reveal fraction; on `slide`/`highlight`
  those words keep their direction/colour meaning. Fractions rather than
  DrawSVGPlugin's absolute pixel range (resolution-independent, plugin-free).
  (#343)
- **`draggable <place>` — make objects interactively draggable.** A new
  directive (not an `animate` effect, because dragging is interaction, not a
  play-on-load timeline) that marks an object grabbable in the browser via GSAP
  Draggable — optionally with momentum (`inertia`, InertiaPlugin), constrained
  to another object's box (`bounds <place>`), or axis-locked (`x`/`y`). It rides
  a new top-level `interactions` array in the `--json` bundle
  (`[{id, kind:"drag", inertia?, bounds?, axis?}]`), absent when unused so plain
  bundles are byte-identical. The static SVG/geometry is untouched (dragging is a
  runtime affordance), so PNG/PDF are unaffected. The npm binding gains an
  `interactive(root, interactions, Draggable)` helper; `draggable` is contextual,
  so it stays usable as a variable name. (#331)
- **`animate … with "wiggle"` — attention shake.** A quick oscillating shake
  that returns to rest — the "look here" nudge that draws the eye without moving
  the object. `wiggles <n>` sets the oscillation count (default 6) and rides the
  manifest as `"wiggles":n` only when set; `wiggles` on a non-`wiggle` effect
  warns `wiggles_without_wiggle`. Built on GSAP's CustomWiggle ease (register
  `CustomWiggle`/`CustomEase`). Byte-inert on the SVG side. Any GSAP ease still
  passes straight through `ease "<name>"` (`ease "bounce.out"`,
  `ease "elastic.out"`, or a consumer-registered `CustomBounce` by name). (#330)
- **`animate … with "scramble"` — decode-style text reveal.** A label's glyphs
  cycle through random characters and resolve into the real text. It drives the
  `<text>` element directly through GSAP's `ScrambleTextPlugin` (which, unlike
  SplitText, works on SVG `<text>`), so there is **zero** cost on the SVG side —
  the static render and the whole corpus are byte-for-byte unchanged. A custom
  charset comes from `by "<chars>"` (e.g. `by "01"` for a binary look; default
  `upperCase`) and rides the manifest as `"chars":"…"` only when set; `out`
  scrambles the label away; `by "…"` on a non-`scramble` effect warns
  `by_without_scramble`. Needs `ScrambleTextPlugin` registered (free since
  GSAP 3.13). (#329)
- **`animate … with "type"` — typewriter text reveal.** A label appears one
  character at a time (or `by word`), staggered over the effect's duration — the
  way a caption "speaks" a step. The split is native: the SVG backend wraps each
  unit of a `type` target in a `<tspan class="rpic-ch">` (GSAP's SplitText
  doesn't support SVG `<text>`), and the browser player staggers their opacity —
  no plugin to install. The tspans carry no positioning, so the static render is
  identical and any drawing without a `type` animation is byte-for-byte
  unchanged. `out` reverses it into a staggered erase; `by` on a non-`type`
  effect warns `by_without_type`. (#328)

### Fixed

- **The `invalid_color` warning now carries a source span.** A colour typo
  (`box outlined "crimsom"`) still warns with a "did you mean" hint, but the
  warning now reports the offending token's `line`/`col`/`end_col` — so an editor
  can jump to and underline it, like every other diagnostic. The colour
  attributes (`outlined`/`shaded`/`color`, `hatchcolor`, `gradient`) thread the
  token span through to the check. (#333)

## [0.9.0] — 2026-07-09

### Added

- **C ABI: full compile options via `*_ex` entry points.** A new `RpicOptions`
  struct and `rpic_render_svg_ex` / `rpic_compile_json_ex` / `rpic_render_png_ex`
  / `rpic_render_pdf_ex` let C/R embedders enable `texlabels`, sandbox
  `copy "file"` includes (`include_policy` 0/1/2), and set the include `base` —
  the circuits-only functions (unchanged ABI) previously exposed none of these,
  so untrusted-input embedders had no way to restrict filesystem access.
- **`thin` line-thickness keyword.** A pikchr-flavoured convenience for a
  lighter stroke — `line thin` / `box thin`, no value — set to two-thirds of the
  current `linethick`, so it tracks your global line width. Complements the
  existing valued `thick <n>`.
- **dvips/xcolor colour names resolve to their RGB.** The 30 dvips names no
  browser understands (`Dandelion`, `BurntOrange`, `Periwinkle`, …) now emit
  their `dvipsnam.def` RGB (`Dandelion` → `#ffb529`) instead of passing through
  as an SVG-invalid keyword that rendered as no paint. dvips names that are
  *also* CSS keywords (`Goldenrod`, `Plum`, …) stay untouched — browsers already
  render those, and the dvips values differ.
- **Unknown colour names are flagged.** A colour string that isn't a CSS named
  colour, a `#hex` / `rgb()` / `hsl()` value, or a dvips/xcolor name (the ones
  the dpic corpus uses, like `Dandelion`) now raises an `invalid_color` warning
  in the `--json` bundle — with a "did you mean" suggestion for near-miss typos
  (`"crimsom"` → `crimson`) — instead of silently rendering blank. The colour is
  still passed through unchanged, so the warning is advisory and never blocks or
  alters rendering (SVG output is byte-for-byte unchanged).
- **Colours can be held in a variable or computed.** In colour position
  (`outlined`, `shaded`, `color`, `hatchcolor`, `gradient`, `animate … to`), a
  bareword that names a variable now resolves to its value as a `0xRRGGBB`
  colour (`accent = 0x2f855a; … outlined accent`), and a parenthesised
  expression is evaluated (`shaded (accent + 0x60)`) — so a palette defined once
  can drive a whole figure. A bareword that is *not* a variable still stays a
  literal colour name, so existing sources are byte-for-byte unaffected.

### Security

- **`font "…"` family names are now XML-attribute-escaped.** A double quote in a
  per-string font family was emitted into the `font-family="…"` attribute
  unescaped (it used the text-content escaper, not the attribute one), which
  closed the attribute early — producing malformed SVG and letting crafted input
  inject stray attributes (e.g. an event handler) onto the `<text>` element, an
  XSS vector when the SVG is embedded inline in HTML. The value now escapes `"`
  to `&quot;` like every other attribute. Byte-for-byte unchanged for any figure
  without a quote in a font name (the whole corpus). (#317)

### Changed

- **SVG escapers are named by context, closing the #317 class of bug.** The two
  near-identical helpers are now `escape_text` (element content) and
  `escape_attr` (attribute values, escaping `"`), so a text escaper used for an
  attribute reads wrong at the call site instead of silently emitting malformed
  markup. Every user-controlled attribute (colour, class, gradient stop, hatch
  colour, font family) routes through `escape_attr`, guarded by a test that
  feeds hostile text through each and asserts the quote can't break out. Also
  drops three needless `stroke` clones (`as_deref().unwrap_or("black")`).
  Output is byte-for-byte unchanged. (#324)
- **The evaluator is split into an `eval/` module.** `eval.rs` had grown to
  7.6k lines — by far the largest file in the crate. It is now `eval.rs` (the
  `State` type, entry points, statement/animation dispatch) plus `eval/build.rs`
  (primitive construction), `eval/resolve.rs` (position/place resolution and
  expression evaluation), `eval/helpers.rs` (pure free functions) and
  `eval/tests.rs`, each comfortably under 2k lines. Pure reorganization: no
  public-API change and SVG output is byte-for-byte identical (corpus 124/124).
  (#323)

### Fixed

- **A rotated, justified label no longer vanishes.** A `"…" ljust rotated 90`
  (or `rjust`) label is rotated about its text anchor, but its bounds were
  computed about the rect centre — offset by half the width — so a long one
  landed entirely outside the viewBox and rendered blank. The bounds now rotate
  about the same anchor the renderer uses.
- **`after <block>` waits for a whole staggered block** even when the block
  leads with an invisible spine (a `move`): the stagger's end time was recorded
  on the first visible child, not the block, so `after` a stagger could start
  mid-way through it.
- **Macro-argument splices space by source position, not rendered length**, so
  a normalized float or escaped string in a spliced arg (`lbl((1.50,2.50))`) no
  longer gains stray spaces (`(1.5 ,2.5 )`).
- **Hostile input can no longer crash the process.** Several unbounded paths
  that aborted (uncatchable, unlike every other error) on adversarial `.pic`
  source — reachable from the CLI and the wasm binding — now fail cleanly:
  deeply nested parentheses/blocks (`((((…))))`) hit a recursive-descent depth
  limit and return a "nested too deeply" error instead of overflowing the stack;
  flat operator chains (`1+1+…`) are capped; a `sprintf` precision like
  `"%.999999999f"` is clamped (to 512 digits) instead of allocating gigabytes;
  and four constructs the earlier passes missed — brace-group nesting
  (`{{{…}}}`), a leading corner chain (`.n.n…`), string concatenation
  (`"a"+"a"+…`), and a trailing member/corner chain (`A.B.B…`) — now hit the
  same depth/chain-length limits rather than overflowing on parser recursion or
  on the drop/evaluation of an unbounded left-deep AST. (#318)
- **The C ABI no longer aborts the host on a Rust panic.** Every `extern "C"`
  entry point now runs its body inside `catch_unwind`, returning the crate's
  null-pointer failure convention instead of letting a panic unwind across the
  FFI boundary and abort the C/C++/R host process. (A stack overflow is a hard
  abort `catch_unwind` cannot intercept — that is guarded separately by bounding
  recursion in the core; this covers ordinary panics.) The Python binding was
  already panic-safe via PyO3; wasm traps cleanly. (#319)
- **wasm `compile()` denies filesystem includes like its siblings.** It
  delegated to the Unrestricted-default core entry, so a `copy "file"` gave an
  opaque io error instead of the clean policy error `compile_circuits`/
  `compile_with` return; it now forces `Deny` (wasm has no filesystem;
  `copy "circuits"` still works).
- **JS: complete animation TypeScript types and docs-player parity.** The
  `Anim` type now declares the optional manifest keys the player actually reads
  (`repeat`/`yoyo`/`ease`/`path`/`color`/`out`/`from`/`morph`) and `Bundle`
  gains `scroll?`; the docs-site GSAP player (`AnimatedPic.astro`) gained the
  unknown-effect fade fallback and the shared `draw`-label timing so it matches
  the shipped npm player exactly. The npm runtime is unchanged.
- **A gradient-only fill now honours `opacity`.** `box gradient … opacity 0.3`
  rendered fully opaque, while solid `fill`/`shaded` and `hatch` fills (and
  gradient+hatch) honoured it — the opacity predicate didn't count a gradient as
  a fill.
- **Macro-argument splices (`"$n"` inside a string) reproduce the argument's
  source text.** A multi-token argument used to be glued without separators,
  with keywords silently dropped and string quotes stripped — a statically
  exec'd `"$2"` holding `box shaded "#00ff00"` re-lexed as `boxshaded` and the
  bare `#…` started a comment. Spacing now comes from the tokens' source spans
  (adjacent tokens like `2L` stay glued; separated words like `$\beta V$` keep
  their gap — that TeX label used to collapse into the undefined `$\betaV$`),
  keywords render, and inner strings keep their quotes. A lone quoted argument
  still splices as bare content, so the classic `box "$1"` label idiom is
  untouched.
- **`gradient` + `hatch` now paints one gradient across the whole object.** An
  SVG pattern tile is stamped once and replicated, so the gradient embedded as
  the tile background restarted in every hatch cell (a "quilted" look) instead
  of spanning the object. The gradient is now painted by a separate underlay
  element with the hatch pattern (transparent tile) on top — in every shape
  kind, and identically in browsers and the PNG/PDF rasterizer. A solid
  `fill`/`shaded` background is uniform, so it stays in the tile as before.
- **`define`s inside `exec` now persist, and the `dpicopt`/`opt*` constants
  match dpic's.** A macro defined by exec'd text landed in a discarded clone of
  the macro table, and the backend-option constants were zero-based where dpic's
  are one-based (`dpic -v` prints `dpicopt=9`, `optMFpic=1` … `optxfig=12`).
  Together these broke the dpic test suite's `DefineRGBColor(name, r, g, b)`
  machinery — its `case(dpicopt, …)` exec-dispatch picked the PSTricks branch
  and the colour macro never registered, so `shaded Custom` emitted a broken
  `fill="Custom"`. User-defined colours now resolve to the same `rgb(…)` string
  dpic emits.
- **Standalone labels (`"text" at P`) no longer clip at the page edge.** Their
  glyph ink is now included in the drawing bounds — matching what attached
  labels already did — so a wide or edge-anchored label is fully visible without
  a hand-tuned `margin`. This intentionally diverges from dpic, which bounds
  attached-label ink but leaves standalone labels zero-width (and so clips them).
- **A quoted `"0xRRGGBB"` colour string no longer slips through to invalid
  SVG.** It is normalised to `#rrggbb` (the bare `0xRRGGBB` literal already
  worked); previously the string form was emitted verbatim as `stroke="0x…"`,
  which no renderer understands.

## [0.8.1] — 2026-07-06

### Fixed

- **JS player crashed on the `move` and `morph` effects.** GSAP's MotionPath and
  MorphSVG plugins need a real `<path>`, but rpic emits primitives
  (`<line>`/`<rect>`/`<circle>`/`<polygon>`), so `@strategicprojects/rpic`'s
  `animate()` threw "Expecting a `<path>` element or an SVG path data string" and
  looped under `repeat`. The player now converts the referenced shapes to
  `<path>` up front (pure DOM, no plugin dependency). The engine, manifest and
  the other bindings were unaffected — this is a browser-player fix, so it ships
  only in the npm package; crates.io / PyPI 0.8.0 were already correct.

### Changed

- The `highlight` effect now also thickens the outline and adds a small scale
  pulse alongside the colour tween, so the emphasis reads at a glance.

## [0.8.0] — 2026-07-06

### Highlights — a complete animation subsystem

`animate` grew from three enter effects into a full, declarative storytelling
layer. Everything stays opt-in and **byte-for-byte inert when unused** (the
123-example corpus renders identically), and every timeline is emitted as a flat
JSON manifest — readable without a player, and driven in the browser by GSAP.

**Effects**

- **`slide` from a compass direction** — enter by translating in from
  `up`/`down`/`left`/`right`, offset by the object's own extent.
- **`move` along a path** — a token/dot travels along another object's geometry
  (a wire, an arrow) via GSAP MotionPathPlugin. `along <path>`.
- **`highlight`** — emphasise an object by tweening its outline to a colour
  (any rpic colour form), or a colour-free scale pulse. `to <colour>`.
- **`morph` into another shape** — tween one object's outline into another's
  geometry (box → circle, symbol → symbol) via GSAP MorphSVGPlugin.
  `into <shape>`.

**Modifiers & structure**

- **`out`** — play *any* effect as an exit instead of an entrance (fade away,
  pop out, retract a `draw`, slide off), for two-beat build-up / tear-down.
- **`repeat` / `yoyo` / `ease`** — GSAP passthroughs: loop count (`-1` = forever
  without stalling the sequence), ping-pong, and a custom easing name.
- **`stagger`** — point `animate` at a `[ … ]` block to fan the effect across
  its visible children, each offset by a fixed delay.
- **`animate scroll`** — a timeline-level hint (surfaced as top-level
  `scroll: true`) that the host should scrub the timeline on scroll; the
  consumer wires GSAP ScrollTrigger.

**Timing** stays sequential by default, or absolute (`at`), or relative
(`after <object>`), each with an optional `delay`.

Every clause that an effect requires (`along`, `into`, `from`, `to`) is a
compile error when missing and a warning (never fatal) when used on the wrong
effect; optional manifest keys are emitted only when set. The reference page and
live examples are at
[rpic.dev/docs/extensions/animate](https://rpic.dev/docs/extensions/animate).

### Added

- Animation effects `slide`, `move`, `highlight`, `morph`; the `out` exit
  modifier; `repeat`/`yoyo`/`ease` passthroughs; block `stagger`; and the
  `animate scroll` directive (see Highlights).
- New warnings: `yoyo_without_repeat`, `along_without_move`,
  `to_without_highlight`, `from_without_slide`, `into_without_morph`,
  `stagger_without_block`, plus the extended `unknown_animation_effect` set.
- `Shape::is_visible()` helper (skips invisible `move`/`invis` spines when
  staggering a block).
- README and homepage refreshed with the full animation palette and a live
  "Watch it build" showcase.

### Changed

- The JSON animation manifest gained optional `repeat` / `yoyo` / `ease` /
  `path` / `color` / `out` / `from` / `morph` per-entry keys and a top-level
  `scroll` flag — all present only when the source uses them, so existing
  manifests are unchanged.
- The JS and Astro players register GSAP's MotionPathPlugin and MorphSVGPlugin
  for the `move` and `morph` effects.

## [0.7.1] — 2026-07-06

- **#240 pikchr-compat** (opt-in, corpus 121/121): `.start`/`.end` as `with`
  anchors on closed objects, `previous` as a synonym for `last`, `aligned` text
  (rotate a label to its segment's angle), and `big`/`small` size words.

## [0.7.0] — 2026-07-05

- Per-object geometry export (`objects` in the compile bundle) for visual
  editors (#227).
- `canvas from <pos> to <pos>` — a fixed page / stable viewBox (#226).
- Per-string font attributes: `bold`/`italic`/`mono`/`font "…"`/`fontsize n`
  (#225).
- `rotated <deg>` labels and native colour literals `rgb(r,g,b)` / `0xRRGGBB`
  (#228).
- First minted DOI via the GitHub ↔ Zenodo webhook.

[Unreleased]: https://github.com/milkway/rpic-lang/compare/v0.10.0...HEAD
[0.10.0]: https://github.com/milkway/rpic-lang/releases/tag/v0.10.0
[0.9.0]: https://github.com/milkway/rpic-lang/releases/tag/v0.9.0
[0.8.1]: https://github.com/milkway/rpic-lang/releases/tag/v0.8.1
[0.8.0]: https://github.com/milkway/rpic-lang/releases/tag/v0.8.0
[0.7.1]: https://github.com/milkway/rpic-lang/releases/tag/v0.7.1
[0.7.0]: https://github.com/milkway/rpic-lang/releases/tag/v0.7.0
