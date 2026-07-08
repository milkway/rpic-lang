# Changelog

All notable changes to **rpic** are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each release is also archived on Zenodo under the concept DOI
[10.5281/zenodo.21209915](https://doi.org/10.5281/zenodo.21209915), which always
resolves to the latest version.

## [Unreleased]

### Added

- **`thin` line-thickness keyword.** A pikchr-flavoured convenience for a
  lighter stroke — `line thin` / `box thin`, no value — set to two-thirds of the
  current `linethick`, so it tracks your global line width. Complements the
  existing valued `thick <n>`.
- **Colours can be held in a variable or computed.** In colour position
  (`outlined`, `shaded`, `color`, `hatchcolor`, `gradient`, `animate … to`), a
  bareword that names a variable now resolves to its value as a `0xRRGGBB`
  colour (`accent = 0x2f855a; … outlined accent`), and a parenthesised
  expression is evaluated (`shaded (accent + 0x60)`) — so a palette defined once
  can drive a whole figure. A bareword that is *not* a variable still stays a
  literal colour name, so existing sources are byte-for-byte unaffected.

### Fixed

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

[0.8.0]: https://github.com/milkway/rpic-lang/releases/tag/v0.8.0
[0.7.1]: https://github.com/milkway/rpic-lang/releases/tag/v0.7.1
[0.7.0]: https://github.com/milkway/rpic-lang/releases/tag/v0.7.0
