# rpic Extensions

rpic's default language target remains **Kernighan-first, dpic as practical
oracle**. The features in this document are explicit rpic extensions: they are
available to authors who opt into them, but classic pic input should keep its
dpic-compatible meaning when the extension is not used.

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
