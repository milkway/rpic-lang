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
