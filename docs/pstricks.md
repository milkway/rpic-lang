# PSTricks Research Notes

Status: research for rpic PSTricks-inspired extensions, including
[#113](https://github.com/milkway/rpic-lang/issues/113) and
[#114](https://github.com/milkway/rpic-lang/issues/114).
Date: 2026-07-01.

These notes summarize PSTricks passes for explicit rpic extensions. The rpic
stance remains **Kernighan-first, dpic as practical oracle**: PSTricks is a
design reference for opt-in rpic extensions, not a replacement semantic oracle
for classic pic.

Sources inspected:

- CTAN package metadata for `pstricks-add`:
  <https://ctan.org/pkg/pstricks-add>
- CTAN package metadata for `pst-node`: <https://ctan.org/pkg/pst-node>
- Downloaded `pstricks-add` source and docs under `/private/tmp`, especially
  `tex/pstricks-add.tex` and `doc/pstricks-add-doc.tex`.
- TUG India PSTricks "Colors and Fillstyle" chapter:
  <https://tug.org/PSTricks/doc/sarovar/chap2.pdf>

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

## PSTricks Hatch Model

PSTricks exposes hatch fills through `fillstyle` values such as `vlines`,
`hlines`, and `crosshatch`. The inspected TUG India PSTricks chapter documents
the important hatch controls:

- `hatchcolor` controls the color of hatch lines;
- `hatchwidth` controls the line width, defaulting to `0.8pt`;
- `hatchangle` controls the line angle in degrees, defaulting to `45`;
- starred fill styles combine a hatch pattern with a `fillcolor` background.

## rpic Hatch Decision

For rpic, the first hatch surface keeps the extension explicit and pic-like:

```pic
box hatch hatchangle 30 hatchsep 0.05 hatchwid 1 hatchcolor red
box fill 0.9 crosshatch
```

The SVG backend uses native `<pattern>` fills. That keeps clipping delegated to
SVG for the shapes rpic already knows how to fill, and it leaves classic
`fill`/`shaded` behavior unchanged when no hatch attribute is present.
