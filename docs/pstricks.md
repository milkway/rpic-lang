# PSTricks Research Notes

Status: research for [#113](https://github.com/milkway/rpic-lang/issues/113).
Date: 2026-07-01.

These notes summarize the PSTricks pass for curly brace annotations. The rpic
stance remains **Kernighan-first, dpic as practical oracle**: PSTricks is a
design reference for explicit rpic extensions, not a replacement semantic
oracle for classic pic.

Sources inspected:

- CTAN package metadata for `pstricks-add`:
  <https://ctan.org/pkg/pstricks-add>
- CTAN package metadata for `pst-node`: <https://ctan.org/pkg/pst-node>
- Downloaded `pstricks-add` source and docs under `/private/tmp`, especially
  `tex/pstricks-add.tex` and `doc/pstricks-add-doc.tex`.

## PSTricks Brace Model

`pstricks-add` provides `\psbrace*[options](A)(B){text}`. The package describes
braces as an add-on and as a node-connection/linestyle feature. Its important
brace-specific options include:

- `braceWidth`: brace body width/depth;
- `braceWidthInner` and `braceWidthOuter`: inner/outer curl radii;
- `bracePos`: relative position of the cusp along the segment;
- `nodesepA`, `nodesepB`, and `nodesep`: label offsets;
- `rot` and `ref`: label rotation and reference point;
- `singleline`: stroke-only mode, where dashed/dotted line styles make sense.

The source computes the angle from A to B, transforms into that local coordinate
system, draws the brace with PostScript arc segments, and places the text near
the cusp with an outward offset.

## rpic Decision

For rpic, the smallest natural form is a native object:

```pic
brace from A to B down "label" wid 0.18 bracepos 0.5
```

This is preferable to a macro because a macro cannot give the evaluator stable
knowledge of the brace bbox, anchors, label position, and SVG cubic path.

The initial rpic surface intentionally adopts only the parts that fit pic well:

- endpoints through existing `from`/`to`;
- side through existing `up`, `down`, `left`, and `right` direction words;
- depth through existing `wid`;
- cusp position through contextual `bracepos`;
- text through normal attached strings and text-position handling later if
  needed.

Deferred PSTricks ideas include separate inner/outer radii, star fill behavior,
label rotation/reference-point controls, and node-connection aliases. Those
should wait until the basic object proves useful in examples.
