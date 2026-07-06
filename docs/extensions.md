# rpic Extensions

rpic's default language target remains **Kernighan-first, dpic as practical
oracle**. The features in this document are explicit rpic extensions: they are
available to authors who opt into them, but classic pic input should keep its
dpic-compatible meaning when the extension is not used.

## Reusable Styles Are Macros (idiom, not an extension)

pic already has named, parameterized, composable styles: `define`. A macro
body can hold any attribute list, and a call expands inline on the object:

```pic
.PS
define warn { outlined "red" dashed thick 1.2 }
define note { shaded "lightyellow" outlined "gray" }
box warn() "error"
box note() "notice"
circle warn() note() "both compose"
.PE
```

This is 100% classic pic — it needs no rpic extension, works with every
attribute on this page (`hatch`, `opacity`, `behind`, …), and parameterizes
naturally (`define sev { outlined $1 thick $2 }`). Prefer it over asking for
a dedicated "style" keyword. The design notes behind this decision live in
[`docs/svg-styles.md`](svg-styles.md).

## Canvas Margins

`margin`, `topmargin`, `rightmargin`, `bottommargin`, and `leftmargin` are
canvas-framing variables inspired by Pikchr. They add whitespace around the
native SVG/PNG/PDF output without moving objects or changing pic geometry.

```pic
.PS
topmargin = 0.25
box "keeps its anchors"
.PE
```

`margin` applies to all four sides. Side-specific variables are additive, so
this adds 0.25 inches everywhere except the left side:

```pic
margin = 0.25
leftmargin = -0.25
```

Margins are dimensions in the current pic units. They follow `scale`,
picture-wide `.PS` sizing, and `maxpswid`/`maxpsht` page clamping, but they do
not affect ordinals, labels, anchors, the current point, object dimensions, or
the drawing order.

## Fixed Canvas

`canvas from <position> to <position>` fixes the output page to the rectangle
spanned by the two corners, independent of the drawn content. Without it the
viewBox is normalized to the content's bounding box, so moving the leftmost or
topmost object shifts the whole drawing on screen; with a fixed canvas the
frame — origin and size — is stable, which is what a visual editor needs to
move one object without reflowing the rest.

```pic
.PS
canvas from (0,0) to (4,3)
box "stays put" at (1,1)
.PE
```

The corners are ordinary pic positions, so the page can be anchored to
geometry (`canvas from F.sw to F.ne`, with `F` an `invis` frame). Rules:

- The rectangle must have positive width and height; corners may be given in
  either order. The last `canvas` statement wins.
- Content outside the canvas is clipped by the viewBox (the SVG default;
  PNG/PDF renderers behave the same).
- It composes with the rest of the framing pipeline: coordinates are user
  units (they follow `scale`), `margin` variables add whitespace *around* the
  fixed page, `.PS` width sizing and `maxpswid`/`maxpsht` clamping scale the
  page and content together.
- Contextual keyword: only the exact `canvas from …` spelling triggers.
  `canvas = 2`, a macro named `canvas`, or any other use of the name keeps its
  classic meaning; unused, output is byte-for-byte classic.

The per-object geometry export (`--json` → `objects`) uses the same frame, so
bboxes stay consistent with the pinned viewBox.

## Render Layers

`behind <object>` is an rpic-only attribute inspired by Pikchr. It paints the
current object behind the referenced object while preserving normal pic
evaluation order.

```pic
.PS
A: box shaded 0.8
box shaded 0.95 behind A at A
.PE
```

This changes only backend drawing order. Labels, anchors, ordinals such as
`last box` and `2nd box`, object ids such as `s0`/`s1`, and animation targets
continue to follow source/evaluation order. When `behind` is absent, rpic keeps
the dpic-compatible natural drawing order.

## Junction Dots

`dot` is an rpic-only object, inspired by Pikchr: a tiny solid circle for
marking junctions and points — the native form of the classic
circuit-macros `dot(P)` idiom.

```pic
.PS
line right 1
dot at Here
line -> down 0.5 then right 0.5
dot at (0.35, 0) colored "red" rad 0.05
.PE
```

- Radius defaults to the new `dotrad` variable (0.035, tracking `scale`);
  `rad`/`diam` override per dot.
- Filled solid (gray 0) unless `shaded`/`fill`/`colored` says otherwise —
  byte-identical to the old `-c` macro's `circle rad 0.035 fill 0`.
- Behaves as a normal (tiny) closed object: anchors, `at`/`with`, `class`,
  fills; dots count as **circles** for ordinals (`last circle`).
- Contextual: `dot = 2` stays an ordinary assignment. The circuit library's
  `dot(P)` macro was retired in favor of the primitive; the figuras
  compatibility shim keeps its own local macro.

## Class Hooks

`class` is an rpic-only extension that attaches CSS class names to the SVG
group (`<g id="sN">`) already emitted for each shape. It has two forms that
write to the same hook:

```pic
.PS
box class "critical" "payment"          # inline, at creation

A: circle "cache"
line right from A.e
class A "storage"                       # statement: by label
class last line "dataflow"              # statement: by ordinal
.PE
```

The statement form reuses pic's native object references — labels, `last
line`, `2nd circle`, `25th box` — exactly like `animate` targets, so it also
reaches shapes drawn inside macros (e.g. circuit-library elements) that inline
attributes cannot annotate, and lets class lines cluster at the end of a
picture as a theme block.

Rules:

- **Inert by itself.** A class changes nothing in rpic's own rendering: SVG,
  PNG, and PDF output are visually identical with or without it. Styling
  happens only when the *host document* that embeds the SVG provides CSS —
  the same delegation contract as the `animate`/GSAP layer.
- **Validated, not raw.** Each whitespace-separated name must match
  `[A-Za-z_][A-Za-z0-9_-]*`; anything else is an error. There is no
  attribute-injection surface.
- Multiple applications append: `box class "a" class "b"` and a later
  `class last box "c"` yield `class="a b c"`.
- The internal `s<N>` ids stay untouched and remain the GSAP/animation
  targets; the class rides alongside on the same group.
- `class` is contextual: `class = 2` is still an assignment and `box wid
  class` still reads the variable.
- Resolution happens at the point of the statement (like `animate`):
  reassigning a label later does not move the class.
- Blocks are not supported yet — class the inner objects instead. Attaching
  a class to a point label is an error (there is no drawn shape).

Note that host-page CSS targets inline-embedded SVG (`<svg>…</svg>` in the
HTML); an `<img src="…svg">` reference isolates the document and external CSS
will not apply. Classic pic input remains dpic-compatible when `class` is not
used — no `class` attribute is emitted at all.

Motion can escape the canvas: the SVG root clips at its viewBox, so a hover
`scale(…)` or an animation overshoot gets cut at the edge. Pick the remedy by
who owns the motion — if the **host** animates (CSS hover, a GSAP timeline it
wrote), the host unclips with `svg { overflow: visible }`; if the **picture**
declares motion itself (rpic's `animate`), reserve room in the source with the
[canvas margin extension](#canvas-margins), e.g. `margin = 0.15`. rpic never
adds space automatically.

## Declarative Animation

`animate` is an rpic-only extension that declares how objects enter the drawing.
It is **metadata only**: the static SVG is byte-for-byte unchanged except for the
stable per-shape ids (`s0`, `s1`, …) rpic already emits, and PNG/PDF/plain-SVG
consumers ignore it entirely. A thin web player drives a GSAP timeline against
those ids; the `compile_json` bundle carries the timeline as a separate
`animations` array.

```
animate <place> with "<effect>" [along <path>] [to <colour>] [from <dir>] [out]
        [stagger <d>] [for <dur>] [at <t> | after <place>] [delay <d>] [repeat <n>] [yoyo] [ease "<name>"]
```

```pic
.PS
margin = 0.12
B1: box "load" fit
arrow
B2: box "run" fit
animate B1 with "pop" for 0.4
animate 1st arrow with "draw"
animate B2 with "fade" after 1st arrow delay 0.2
.PE
```

- **Target** is any native pic reference (label, ordinal such as `1st arrow` or
  `last box`, or `previous`) — the same delegation contract as `class`, riding
  the same `s<N>` ids. It must be a drawn shape; a bare point is an error.
- **Effects**: `fade` (opacity), `pop` (scale, overshoots), `draw` (strokes
  trace themselves), `move` (travel along another object's path — see below),
  `highlight` (emphasis — see below), `slide` (translate in from `from <dir>`).
  Any other string is accepted but flagged with an `unknown_animation_effect`
  warning and renders nothing.
- **Direction / exit**: `slide` enters from a compass direction (`from
  up`/`down`/`left`/`right` — required; `from` elsewhere warns
  `from_without_slide`). The `out` modifier reverses **any** effect into an exit
  (fade away, pop out, retract a `draw`, slide off), for two-beat build-up /
  tear-down. Both ride the manifest (`"out":true`, `"from":"left"`) only when set.
- **Motion along a path** (`move`): `along <path>` names a drawn `line`/`arrow`/
  `spline` whose geometry the target follows (GSAP MotionPathPlugin); the target
  is typically a `dot` at the path's start. The manifest entry gains a `path`
  key (`"path":"s1"`). `move` without `along` is an error; `along` on any other
  effect is ignored with an `along_without_move` warning.
- **Emphasis** (`highlight`): `to <colour>` (any rpic colour form) tweens the
  object's outline to that colour; without a colour it's a colour-free scale
  pulse. One-directional — add `repeat 1 yoyo` for a flash-and-return or
  `repeat -1 yoyo` for a continuous pulse. The colour rides the manifest as a
  `color` key; `to` on any non-`highlight` effect warns `to_without_highlight`.
- **Staggering a group** (`stagger <d>`): point `animate` at a `[ … ]` block to
  fan the effect across its **visible** children (invisible `move`/`invis`
  helpers are skipped), each starting `d` seconds after the previous, in source
  order. It expands to one ordinary manifest entry per child (no new key), and
  the sequence resumes after the last child. `stagger` on a non-block target is
  ignored with a `stagger_without_block` warning.
- **Duration** (`for`) defaults to `0.6` seconds.
- **Timing** is one of: *sequential* (default — start when the previously
  declared animation ends), *absolute* (`at <t>`), or *relative* (`after
  <place>` — start when that object's animation ends). `at` and `after` share a
  slot, so the last one given wins. `delay <d>` then offsets the resolved start.
- **Looping / easing** (GSAP passthrough): `repeat <n>` replays the effect (`-1`
  loops forever, `0`/absent plays once); `yoyo` reverses each pass (and warns
  with `yoyo_without_repeat` if used alone); `ease "<name>"` overrides the
  effect's default easing with any GSAP ease (e.g. `"elastic.out(1, 0.3)"`). An
  infinite `repeat` does not stall the sequence — sequential/`after` timing
  tracks only the first pass.
- The manifest is a flat array of `{ id, effect, start, duration }` (plus
  `repeat`/`yoyo`/`ease` only when set) with **absolute** start times in
  seconds — readable without a player.

Pop/draw overshoot can escape the canvas; reserve room with
[`margin`](#canvas-margins). See also [Class Hooks](#class-hooks) — both share
the `s<N>` id contract, so one shape can carry both a CSS hook and an animation.

## Closed Line Paths

`close` is an rpic-only attribute for turning a multi-segment `line` into a
polygon. It is inspired by Pikchr's filled-polygon idiom, but remains outside
dpic compatibility: dpic 2025.08.01 treats `close` as an unresolved variable.

```pic
.PS
line right 1 then up 0.7 close shaded "gold" outlined "black" "triangle"
.PE
```

`close` is contextual, so `close = 1` remains an ordinary variable assignment.
The attribute must appear after at least three vertices have been established,
and it ends the route: adding `then`, `to`, `by`, directions, or bare distances
after `close` is an error. This mirrors Pikchr's "polygon is closed" behavior
and avoids ambiguous current-point semantics.

For a closed line:

- rpic appends the first point to the path when needed and renders SVG with
  `<polygon>`;
- the current point and `.end` become the first point, as if the final segment
  explicitly returned there;
- `.c` / `.center` and attached labels use the polygon's bounding-box center,
  not the midpoint between `.start` and `.end`;
- ordinary line styling still applies, including `shaded`, `outlined`,
  `colored`, `fill`, `opacity`, `hatch`, `crosshatch`, `dashed`, `dotted`, and
  `thick`.

When byte-for-byte dpic parity is the goal, close the path in classic pic style
with an explicit final segment instead:

```pic
line from A to B then to C then to A shaded "gold"
```

## Fitted Text Objects

`fit` is an rpic-only attribute inspired by Pikchr. It sizes a closed object to
the visible text already declared on that object, while keeping classic pic
input unchanged unless the author opts in.

```pic
.PS
box "long label" fit
move right 1
ellipse "two" "lines" fit
.PE
```

The first implementation applies to `box`, `ellipse`, and `circle`:

- only text that appears before `fit` contributes to the fitted size;
- later text remains attached to the object, but does not change its geometry;
- explicit dimensions still win: `wid`/`ht` keep their values on boxes and
  ellipses, while `rad`/`diam` keep their values on circles;
- `scaled` is applied after fitting, just as it is for explicit dimensions;
- using `fit` without visible preceding text is an error.

The text metrics are the same approximate metrics rpic already uses for
rendered text bboxes. That keeps the feature practical and backend-stable
inside rpic, but it also means `fit` is not a dpic oracle feature. Programs that
do not use `fit` keep their dpic-compatible dimensions and placement.

## Font Attributes

`bold`, `italic`, `mono`, `font "<family>"`, `fontsize <points>` and the
pikchr size words `big`/`small` style each text string individually (inspired
by Pikchr's `bold`/`italic`). They bind like `ljust`/`rjust`: to the string
they follow, or — written before any string — to the next one. `big`/`small`
are sugar over `fontsize` (1.5× / 0.7× of the classic 11 pt).

```pic
.PS
box "Heading" bold "subtitle" italic fontsize 9
box "code" mono fit
"caption" font "Georgia" fontsize 14
.PE
```

- Emission is per `<text>`: `font-weight="bold"`, `font-style="italic"`,
  `font-family="monospace"` / the given family, `font-size="<n>pt"`. Unstyled
  lines keep the classic output byte-for-byte.
- Measurement follows the style: `fit`, text bboxes and standalone default
  heights scale with `fontsize` (and bold's ~5% wider advance), so styled
  labels stay inside their boxes.
- PNG/PDF embed the Go family faces (bold/italic/bold-italic/mono/mono-bold),
  keeping raster output machine-independent; arbitrary `font "…"` families
  resolve in the viewer for SVG and fall back to the embedded face in raster.
- Math (`$…$` under `texlabels`) is typeset by the math renderer and ignores
  these attributes.
- dpic rejects these words as syntax errors, so no valid dpic input changes
  meaning; `fontsize <= 0` is an eval error.

## Rotated Text

`rotated <degrees>` is a per-string text attribute (binding like `ljust`):
angles are CCW (pic convention) and the rotation pivots on the text's SVG
anchor (`transform="rotate(-a x y)"` — negated because SVG's screen space is
y-down). `fit` and standalone-text bounds cover the rotated extent (an
axis-aligned cover of the rotated line box, padded for the anchor-vs-center
offset). Attached labels keep the classic dpic behaviour of overflowing the
canvas; `margin` gives them room. Math lines ignore it. dpic rejects the word
(oracle-checked), and `rotated` stays usable as a variable.

```pic
arrow right 2 "gradient ascent" rotated 20 above
"y axis" rotated 90
```

### `aligned` (pikchr)

`aligned` is a per-string text attribute that rotates the label to the host
segment's angle — the pikchr spelling, reusing the `rotated` machinery. Only
linear objects have a segment (line/arrow/spline/move); elsewhere it is inert.
The angle is `atan2(end − start)`, normalized to keep the text upright (a
leftward or downward segment reads horizontally, never upside down; a nearly
horizontal segment leaves the label unrotated, byte-identical to a plain one).

```pic
arrow from (0,0) to (2,1) "gradient ascent" aligned above
```

## Colour Literals

The colour grammar (`outlined`/`shaded`/`color`/`hatchcolor`/`gradient`)
accepts two native literal forms besides names and quoted strings:

```pic
box shaded rgb(27,94,32)
circle outlined 0xb3261e
```

- `rgb(r,g,b)` takes full expressions; components 0–255, out-of-range is an
  eval error. Evaluates to `#rrggbb`.
- `0xRRGGBB` is pikchr's numeric-colour spelling; `0x…` is a general hex
  number literal (usable anywhere a number is). Numeric colours range
  0–0xFFFFFF.
- Bare `#hex` is impossible in pic — `#` starts a comment — but the quoted
  `shaded "#1b5e20"` form has always worked and still does. `rgb` stays
  usable as a macro/variable name (only `rgb(` in colour position triggers).

## Pikchr Positioning Niceties

Two small pikchr conveniences that dpic lacks:

- **`previous`** is a synonym for `last` (the immediately preceding object):
  `previous`, `previous box`, `2nd previous`, `previous.e` all work. Like
  `last`, it is a reserved word.
- **`.start` / `.end` as `with` anchors on closed objects.** rpic already reads
  `box.start` / `box.end` (the entry/exit edge for the current direction — for
  a rightward box, `.w` and `.e`); now they also work as placement anchors, so
  `B: box with .start at A.end` edge-aligns `B` against `A` instead of centring
  it. Compass corners and `.c` are unchanged.

  ```pic
  A: box "in"
  B: box "out" with .start at A.end
  ```

## Fill Opacity

`opacity <expr>` is an rpic-only attribute for making filled regions partially
transparent. It maps to SVG `fill-opacity`, so outlines, arrowheads, brace
strokes, and labels remain crisp while shaded, filled, or hatched areas fade.

```pic
.PS
box fill 0.8 opacity 0.45 "fill only"
circle shaded "gold" opacity 0.35 outlined "black"
line right then up then left then down crosshatch opacity 0.4 "label stays solid"
.PE
```

The value must be between `0` and `1`, where `0` is fully transparent and `1`
is fully opaque. The default is no explicit opacity, so classic pic input keeps
its dpic-compatible SVG when the attribute is absent. `opacity` has no visible
effect on an object that has no `fill`, `shaded`, `hatch`, or `crosshatch`
region, and standalone text rejects it explicitly.

For `[ ... ]` blocks, opacity multiplies into each contained fill. For example,
`[ box fill 0.8 opacity 0.5 ] opacity 0.5` renders the inner box fill with
effective opacity `0.25`, while its outline and labels stay opaque.

This first surface intentionally avoids separate stroke, text, or whole-object
opacity controls. Those remain possible future style refinements under the
broader SVG/CSS styling work tracked in #116. PNG and PDF output inherit the
behavior because those backends are rendered from rpic's SVG.

Additional runnable examples are in `examples/opacity.pic`.

## TeX Math Labels

`texlabels` is an rpic-only extension that typesets label strings written as
inline TeX math. It is off by default; classic input — including the corpus
`$G(s)$`-style labels that dpic's SVG mode also prints literally — is
untouched unless the author opts in:

```pic
.PS
texlabels = 1
box "$-\frac{T}{2}$"
circle "$\beta$"
line right "$F_1(\omega)$" above
.PE
```

Rules:

- Only label strings **fully delimited** as `$…$` (after trimming spaces) are
  typeset; everything else renders as plain text exactly as before. A string
  with interior `$` characters is left alone.
- Rendering is a pure library call (RaTeX, a Rust port of KaTeX) — no
  process execution, so the #129 raw-backend policy is not implicated.
  KaTeX-grade coverage: fractions, radicals, sub/superscripts, operators,
  Greek, `\mathbb`, matrices, and so on.
- The formula becomes a self-contained group of glyph paths in the SVG
  (KaTeX fonts embedded as outlines), so PNG/PDF rasterize it identically
  with no font installed — and the label uses **exact metrics**
  (width/height/depth from the layout engine), so bboxes, anchors, `fit`,
  and baseline alignment are more precise than the classic estimate.
- If a formula fails to parse, the label falls back to the literal text and
  a `print`-style diagnostic is emitted — a bad formula never fails the
  picture.
- The wasm build ships without the math renderer (size budget): labels fall
  back to literal there, with a diagnostic. Browser playgrounds can typeset
  via host-page KaTeX instead (the class-hooks delegation pattern).

For classic sources you do not want to edit (e.g. an existing dpic corpus),
the activation can also come from the invocation as a convenience
initializer — `rpic -t`/`--texlabels`, `rpic.render_svg(src, texlabels=True)`
in Python, `compile(src, { texlabels: true })` in JS — all equivalent to
prepending `texlabels = 1`. The source stays sovereign: the canonical switch
is the variable (it affects geometry, so the picture should describe itself),
and a `texlabels = 0` in the source overrides the flag.

The renderer sits behind a neutral hook in the core
(`set_math_renderer`), so the backend is replaceable: **Typst + mitex is the
documented alternative** should RaTeX ever stall — see the candidate matrix
in [`docs/tex-labels.md`](tex-labels.md). dpic compatibility is unaffected:
in the dpic ecosystem LaTeX typesets labels in the TeX backends, and dpic's
own SVG mode prints `$…$` literally, exactly like rpic with `texlabels = 0`.

## Linear Gradients

`gradient` is an rpic-only fill extension, inspired by PSTricks'
`fillstyle=gradient` (`gradbegin`/`gradend`/`gradangle`), with pic-style
attributes instead of TeX option lists:

```pic
.PS
box gradient "steelblue" "white"
circle gradient "gold" "orangered" gradientangle 45
.PE
```

- `gradient "<from>" "<to>"` — two color stops, accepting the same quoted or
  bare color names as `outlined`/`shaded`/`hatchcolor`;
- `gradientangle <expr>` — degrees in pic coordinates, defaulting to `0`
  (left to right); `90` runs bottom to top, matching how `hatchangle`
  measures. `gradientangle` alone creates a default black-to-white gradient.

The SVG backend emits a native `<linearGradient>` in `<defs>` with
`objectBoundingBox` units, so the gradient follows each shape's own bounds
and rasterizes identically in PNG/PDF (resvg/svg2pdf support it natively).

Composition follows the fill slot:

- `gradient` takes precedence over `fill`/`shaded` on the same object;
- combined with `hatch`/`crosshatch`, the gradient becomes the pattern
  background behind the hatch lines;
- `opacity` applies to the composed fill as usual.

Deferred extensions (each additive later): multi-stop lists,
`radialgradient`, per-stop opacity, and user-space coordinates. Classic pic
input remains dpic-compatible when these attributes are not used.

## Hatch Fills

`hatch` and `crosshatch` are rpic-only fill extensions for closed regions. They
are inspired by PSTricks' hatch fill styles, but keep pic-style object
attributes instead of TeX option lists.

```pic
.PS
box fill 0.92 hatch hatchangle 30 hatchsep 0.05 hatchwid 1
circle crosshatch hatchcolor red
.PE
```

The supported surface is intentionally small:

- `hatch` draws one family of parallel hatch lines;
- `crosshatch` draws two perpendicular families;
- `hatchangle <expr>` sets the line angle in degrees, measured in pic
  coordinates, defaulting to `45`;
- `hatchsep <expr>` sets the spacing between hatch lines in pic units,
  defaulting to `0.08`;
- `hatchwid <expr>` or `hatchwidth <expr>` sets the hatch line width in points,
  defaulting to `0.8`, matching PSTricks' documented default;
- `hatchcolor <color>` accepts the same quoted or bare color names as
  `outlined`/`shaded`, defaulting to black;
- when combined with `fill` or `shaded`, the existing fill becomes the pattern
  background; without a fill, the hatch background is transparent.

As with other pic attributes that accept optional expressions, ordering matters:
put `hatch`/`crosshatch` before trailing attributes such as `dashed`, `dotted`,
or `invis` when there is no explicit numeric argument between them.

The SVG backend emits native `<pattern>` fills, so clipping is handled by SVG
for boxes, circles, ellipses, and filled open paths/splines/arcs. PNG/PDF paths
that rasterize from SVG inherit the same appearance; future native non-SVG
backends should either materialize clipped hatch lines or document a fallback.

### Keeping labels legible

A label sits on top of the pattern, so the glyphs are never crossed by hatch
lines — but the lines still run up to the text. rpic does **not** auto-mask a
gap around labels: in the spirit of pic, geometry is composed explicitly rather
than by a hidden render effect. When you want a clear label over a busy pattern,
place an opaque swatch behind it with the primitives pic already gives you:

```pic
.PS
B: box wid 1.7 ht 1 crosshatch hatchsep 0.05
box fill 1 wid 0.95 ht 0.34 at B.c "crosshatch"   # framed plaque

C: circle diam 1.1 hatch hatchcolor blue at B.e + (1.4,0)
box fill 1 invis wid 0.5 ht 0.5 at C.c "one" "two" # borderless mask
.PE
```

`fill 1` is opaque white (pic's `0 = black … 1 = white` fill); add `invis` to
drop the border and keep just the masked area. You size and color the swatch,
so single- or multi-line labels, framed or borderless, are all under your
control — no white is hardcoded and nothing is masked that you did not ask for.

Classic pic input remains dpic-compatible when these attributes are not used.
Additional runnable examples are in `examples/hatch.pic`.

## Curly Brace Annotations

`brace` is an rpic-only object for grouping or annotating a span between two
points. It is inspired by PSTricks' `\psbrace`, but uses pic-style object syntax
instead of TeX option lists.

```pic
.PS
A: box "parse"
move right 1.2
B: box "render"
brace from A.nw to B.ne up "pipeline" wid 0.18
.PE
```

The first implementation keeps the surface small:

- `brace` is contextual, so `brace = 1` remains an ordinary variable
  assignment;
- `from`/`to` set the brace endpoints, and `last brace.start` /
  `last brace.end` are available like other open-object anchors;
- `last brace.c` / `.center` resolve to the brace cusp, so `bracepos` moves
  the object's logical center; compass anchors such as `.nw` and `.ne` are
  convenience anchors on the brace curve's bounding box, not semantic curl
  points;
- `up`, `down`, `left`, and `right` choose the absolute side where the brace
  opens when explicit endpoints are present;
- `wid` controls brace depth, defaulting to `0.18`;
- `bracepos <expr>` moves the cusp along the segment and must be between 0 and
  1, defaulting to `0.5`;
- `labeloffset <expr>` adds local outward spacing between the brace cusp and
  attached text;
- ordinary line styling applies, including `thick`, `dashed`, `dotted`,
  `outlined`, `colored`, `invis`, and the global `linethick`;
- attached text is placed outside the brace on the chosen side.

To leave whitespace between a brace and the annotated objects, shift the
endpoints with ordinary pic coordinate arithmetic:

```pic
gap = 0.16
brace from A.nw + (0,gap) to B.ne + (0,gap) up "pipeline"
```

This is a native object rather than a macro because the renderer must know its
bbox, anchors, label position, and SVG cubic path. Classic pic input remains
dpic-compatible when `brace` is not used. Additional runnable examples are in
`examples/brace_labeloffset.pic`, `examples/brace_pos.pic`,
`examples/brace_sides.pic`, `examples/brace_style.pic`, and
`examples/brace_width.pic`.

## In-Source Circuit Library Loading

`copy "circuits"` is a reserved include target that loads the embedded native
circuit-element library — the in-source spelling of the `-c` flag, mirroring
how `texlabels = 1` is the in-source spelling of `-t`. A figure can declare
its own dependency instead of relying on every consumer to pass the flag:

```pic
.PS
copy "circuits"
A:(0,0); B:(2,0)
resistor(A,B)
.PE
```

Rules:

- The name resolves **before** any filesystem lookup, so it works even where
  file includes are unavailable (the wasm build, `compile_json` with no base
  directory). A real file literally named `circuits` (no extension) is
  shadowed; rename it or add an extension.
- Loading is idempotent: with `-c` (or `circuits: true` in the bindings) plus
  an explicit `copy "circuits"`, the second load is skipped — output is
  byte-identical to the flag alone.
- Like any `copy`, it splices macro definitions where it appears: put it
  before the first use of a library element.
- Classic input is untouched — the target only triggers on the exact string
  `"circuits"`, which previously always failed (no such file).
