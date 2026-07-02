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
