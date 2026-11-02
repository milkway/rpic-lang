# The scene format (`rpic --scene`)

The **scene** is rpic's device-independent output: the resolved draw list the
SVG backend consumes, serialized as versioned JSON. It is the analogue of
classic pic emitting troff `\D` commands — rpic acts as a *frontend* (parser +
layout) and the scene lets anyone build their own *device*: a GeoJSON exporter
(geopic), a TikZ backend, a Canvas 2D player, a visual editor.

The scene describes **what would be drawn**, never **how to paint it**: final
geometry in inches (y-up), style as descriptors, text as text.

- Opt-in and inert: `rpic file.pic` and `rpic --json file.pic` are
  byte-identical with the scene feature compiled in. Only `--scene`,
  `--json=all` and the explicit binding entry points produce a scene.
- Everything the SVG paints that the scene cannot represent raises a
  structured `scene_unsupported` warning (with the shape's span) — never
  silence. The catalogue is in [§9](#9-unrepresented-svg-features-warned).
- `animate`/`draggable` stay in their own `animations`/`interactions`
  manifests; they are not scene material.

## 1. Invocation

```sh
rpic --scene file.pic                 # the scene JSON alone
rpic --scene --flatten 1e-3 file.pic  # curves pre-flattened to polylines (tolerance in inches)
rpic --json=all file.pic              # the compile bundle + a "scene" key
rpic --json file.pic                  # unchanged — no scene key, byte-identical to before
```

Library/bindings:

| Surface | Entry point |
|---|---|
| Rust | `rpic_core::scene_json(&drawing, &SceneOptions)` / `compile_json_all_with_options` |
| Python | `rpic.scene(src, flatten=None, …)`; `rpic.compile(src, scene=True)["scene"]` |
| JS/WASM | `compile(src, {scene: true, flatten})` → `.scene` |
| C | `rpic_compile_json_opts(...)` with the scene flag (see `rpic.h`) |
| R | `rpic_scene()` — see [§11](#11-binding-status) for current status |

## 2. Top level

```json
{
  "scene": 1,
  "units": "in",
  "y_up": true,
  "canvas": { "w": 2.52, "h": 1.02,
              "margin": {"top": 0, "right": 0, "bottom": 0, "left": 0},
              "origin": [0.01, 0.01] },
  "source": "figure.pic",
  "vars": { "geo_lon": -34.8811, "geo_lat": -8.0539, "scale": 1,
            "boxwid": 0.75, "boxht": 0.5, "linethick": 0.8, "...": 0 },
  "items": [ ... ]
}
```

| Field | Meaning |
|---|---|
| `scene` | integer format version, this document describes **1** |
| `units` | always `"in"` (inches) — every length in the scene unless noted |
| `y_up` | always `true`: pic model coordinates, y grows upward |
| `canvas.w`, `canvas.h` | full page size in inches, margins and stroke padding included — the SVG root is exactly `w×96 × h×96` px |
| `canvas.margin` | the `margin*` extension values per side, inches |
| `canvas.origin` | `[ox, oy]`: where the pic origin `(0,0)` sits, measured from the canvas **lower-left** corner |
| `source` | the input's base name, when the caller provided one (the CLI does); absent otherwise |
| `vars` | every user variable and every pic built-in (by its pic name: `boxwid`, `scale`, …) with its **final** value, full `f64` precision |
| `items` | the draw list, in draw order ([§3](#3-items)) |

**Mapping scene → SVG pixel space** (for consumers that want to overlay on the
official SVG, and for the fidelity tests): a scene point `(x, y)` lands at

```
svg_px = ( (x + origin[0]) · 96 ,  (canvas.h − (y + origin[1])) · 96 )
```

**`vars` caveat:** values are the end-of-evaluation ones. The whole-picture
rescale that `.PS <w> <h>` / `maxpswid`/`maxpsht` may apply afterwards scales
the geometry but — as in classic pic — not the variables.

## 3. Items

`items` is in **draw order**: the same stable sort by (`behind` layer, source
index) the SVG emitter uses, so array order equals SVG document order of the
`<g id="sN">` groups. Fields common to all items:

| Field | Presence | Meaning |
|---|---|---|
| `id` | always | the stable SVG id: `"s<N>"` (N = source index) for shapes, `"blk<N>"` for block items |
| `kind` | always | `box` `circle` `ellipse` `arc` `line` `arrow` `spline` `text` `dot` `brace` `block` |
| `z` | shape items | index in draw order, dense, 0-based (hidden items included; block items carry none) |
| `label` | when labelled | the pic label the statement was given (`L: box` → `"L"`) |
| `block` | inside a block | path of enclosing block item ids, outermost first (`["blk0","blk1"]`) |
| `class` | when set | the `class` extension string |
| `link` | when set | the `link` extension URL (already validated) |
| `hidden` | when true | the item paints nothing (`move`, invisible helper spines). Ids must align with the SVG's groups, which exist for every shape, so hidden items stay in the list |

Notes on kinds: `arrow` is a `line` whose statement carried arrowheads; `dot`
is the `dot` primitive (a filled circle in the SVG). The `objects` array of the
compile bundle keeps its own, older kind vocabulary (`path`, `circle`) — the
two are intentionally not unified (the `objects` schema is frozen).

### 3.1 Geometry per kind

All coordinates are absolute model inches, y-up. `fit`, `rotated`, `close`,
`.PS`/`maxps` scaling are already applied — this is final geometry.

| Kind | Fields |
|---|---|
| `box` | `at` (center `[x,y]`), `w`, `h`, `rad` (corner radius, present when > 0) |
| `circle`, `dot` | `at` (center), `r` |
| `ellipse` | `at` (center), `w`, `h` |
| `line`, `arrow` | `points` (`[[x,y],…]`), `closed` (present when true); for arrows the decorated endpoints are already pulled back so the stroke does not poke through the head — the true tip is in `heads` |
| `spline`, `arc`, `brace` | `paths` ([§3.2](#32-paths-curves)) |
| `text` | `at` (the object's anchor point) + `text` ([§5](#5-text)) |
| `block` | `items`: the child item ids, in id order; no geometry (a block is a grouping, its frame — if drawn — is a child `box`) |

Any kind may carry `text` (attached labels), `stroke`, `fill`, `opacity`
([§4](#4-style)) and — for `line`/`arrow`/`spline`/`arc` — `heads`
([§3.3](#33-arrowheads)).

### 3.2 Paths (curves)

Curves are **cubic Béziers**; a consumer needs only moveto / cubic / lineto /
close. A path object is one of:

```json
{ "closed": false, "start": [x0,y0],
  "cubics": [ [[c1x,c1y],[c2x,c2y],[px,py]], ... ] }

{ "closed": true, "points": [[x,y], ...] }
```

- `cubics` is the exact segment list the SVG `d` attribute carries (splines use
  dpic's classic / tensioned constructions; arcs are converted with ≤ 90°
  segments) — round-tripping through the scene→SVG mapping reproduces the
  official `d` to serializer precision.
- With `--flatten <tol>` every `cubics` path is delivered as a `points`
  polyline instead, adaptively subdivided so the polyline stays within `tol`
  inches of the curve.
- In the rare combination *open fill + arrowheads* (the stroke is trimmed, the
  filled area is not) an item additionally carries `fillPaths` /
  `fillPoints` with the untrimmed outline; otherwise the fill uses the same
  geometry as the stroke.

### 3.3 Arrowheads

`heads` is an array of up to two entries:

```json
{ "at": "end", "filled": true,  "points": [[lx,ly],[tipx,tipy],[rx,ry]] }
{ "at": "start", "filled": false, "points": [[lx,ly],[px,py],[rx,ry]] }
{ "at": "end", "filled": true,  "path": { "closed": true, "start": [...], "cubics": [...] } }
```

- `filled: true` + `points`: a solid triangle (`arrowhead=2`, the default),
  painted in the stroke color, no outline.
- `filled: false` + `points`: an open head — a polyline stroked exactly like
  the shaft (`arrowhead=0`).
- `path`: arc arrowheads (both styles) are curved wedges; they come as a
  closed cubic path filled with the stroke color.

## 4. Style

Descriptors, never paint instructions. Absent = not painted.

```json
"stroke": { "color": "#000000", "width": 0.8,
            "dash": { "kind": "dashed", "len": 0.05 } },
"fill":   { "color": "#90ee90",
            "gradient": { "angle": 90, "stops": [
                {"offset": 0, "color": "#ff0000"},
                {"offset": 1, "color": "#0000ff"} ] },
            "hatch": { "angle": 45, "spacing": 0.1, "width": 0.4,
                       "color": "#000000", "cross": false } },
"opacity": 0.5
```

- `stroke.color`: resolved `#rrggbb` ([§8](#8-color-resolution)). `stroke.width`
  in **points** (pic's native stroke unit; the SVG uses `width·96/72` px).
- `stroke.dash`: `{"kind":"dashed","len":<in>}` (SVG realisation:
  `dasharray = 7·len/6, 5·len/6`, in the same units as the stroke) or
  `{"kind":"dotted","gap":<in>}` with round caps (`dasharray = 0.5px, gap`);
  a dotted stroke without explicit spacing omits `gap` — the default is
  `5 × stroke width` (device px in the SVG). Solid strokes carry no `dash`.
- `fill` carries any subset of `color` / `gradient` / `hatch`, layered
  bottom-up in that order (a gradient under hatch lines spans the whole
  object, matching the SVG's underlay trick; a solid color under hatch is the
  hatch tile background). pic gray shades (`fill 0.3`) arrive as hex
  (`0 = black … 1 = white`).
- `gradient.angle`, `hatch.angle` in degrees, pic convention (0 = left→right,
  90 = bottom→top). `hatch.spacing` inches, `hatch.width` points.
- `opacity` is the fill opacity (`0..1`), present only when the source set it.
  rpic has no stroke opacity.

## 5. Text

Text stays text. Each line of an item's `text` array:

```json
{ "text": "label", "at": [x, y], "anchor": "c",
  "font": { "size": 11, "family": "monospace", "bold": true, "italic": true },
  "rotation": 30 }
```

- `at` is the line's **baseline anchor point** — exactly the point the SVG
  backend places the glyph run at (pre-flip), so an SVG-like consumer needs no
  font metrics at all: put the baseline at `at`, justify per the `anchor`'s
  horizontal component. `anchor` is the pic compass of the justification —
  `c n s e w ne nw se sw` (`w` = the text extends right of the reference,
  `s` = it sits above it, …): semantic information for consumers like TikZ
  that think in box anchors.
- `font.size` is always present, in points, fully resolved (classic 11 pt for
  attached labels, height-derived for standalone text objects, or the explicit
  `fontsize`). `family` appears for `mono`/`font "…"`; `bold`/`italic` only
  when set.
- `rotation` in degrees CCW (from `rotated`/`aligned`, already resolved),
  applied about the line's baseline anchor point (see below).
- Math lines (`texlabels`) carry the original literal in `text` plus
  `"math": {"width", "height", "depth"}` (inches, ink box about the baseline).
  Their glyphs are **not** in the scene unless the `outline` option is set, in
  which case `"outline"` holds the self-contained `<svg>` glyph fragment the
  SVG backend embeds. Without `outline`, a math line raises
  `scene_unsupported` ([§9](#9-unrepresented-svg-features-warned)).

**Rendering recipe** (what the A2 reference renderer does): emit the glyph run
with its baseline at `at`, horizontal justification `start`/`middle`/`end` for
a `w`/center/`e` anchor; `rotation` rotates about `at`. Multi-line labels come
pre-stacked: each line already carries its own `at` on the classic line grid,
so no line-skip computation is needed downstream. Consumers that need the ink
*box* (e.g. TikZ node anchors) can estimate it with the engine's own metric
model: `em = size/72` in, x-height `0.66·em`, average glyph advance `0.6·em`
(bold ×1.05) — the box's bottom edge sits at the baseline and its top at
baseline + x-height for anchor purposes.

## 6. A complete example

```pic
geo_lon = -34.8811
B: box "hub" fit shaded "lightgreen"
arrow right 0.6 from B.e
C: circle rad 0.18 at B.e + (0.8, 0) dashed
```

```json
{
  "scene": 1, "units": "in", "y_up": true,
  "canvas": { "w": 1.993, "h": 0.511,
              "margin": {"top":0,"right":0,"bottom":0,"left":0},
              "origin": [0.571, 0.267] },
  "vars": { "geo_lon": -34.8811, "boxwid": 0.75, "boxht": 0.5, "...": 0 },
  "items": [
    { "id": "s0", "kind": "box", "z": 0, "label": "B",
      "at": [0, 0], "w": 0.357, "h": 0.218,
      "fill": { "color": "#90ee90" },
      "stroke": { "color": "#000000", "width": 0.8 },
      "text": [ { "text": "hub", "at": [0, -0.0504], "anchor": "c",
                  "font": { "size": 11 } } ] },
    { "id": "s1", "kind": "arrow", "z": 1,
      "points": [[0.178, 0], [0.693, 0]],
      "stroke": { "color": "#000000", "width": 0.8 },
      "heads": [ { "at": "end", "filled": true,
                   "points": [[0.678,0.025],[0.778,0],[0.678,-0.025]] } ] },
    { "id": "s2", "kind": "circle", "z": 2, "label": "C",
      "at": [0.978, 0], "r": 0.18,
      "stroke": { "color": "#000000", "width": 0.8,
                  "dash": { "kind": "dashed", "len": 0.05 } } }
  ]
}
```

(Numbers illustrative; the committed tests pin the real ones.)

## 7. Versioning and compatibility policy

- `scene` is a single integer, starting at **1**.
- **Adding** an optional field (or a new `kind`, or a new warning) keeps the
  version. Consumers MUST ignore fields and kinds they do not know.
- **Renaming or removing** a field, or changing the meaning or unit of an
  existing one, bumps the version. Version bumps are release-noted and the old
  version is not emitted anymore (the version field is how consumers detect it).
- The emitter never relies on key order; consumers must not either.
- Enforcement: `crates/core` carries a field-manifest test — the set of key
  names emitted over a corpus-wide sweep is compared against a committed
  golden list, so a silent rename/removal fails CI until the golden file *and*
  the version are updated together.
- The machine-readable contract is [`docs/scene.schema.json`](scene.schema.json)
  (JSON Schema 2020-12); a test validates every corpus scene against it.

## 8. Color resolution

All scene colors are `#rrggbb` (lowercase). The emitter resolves, in order:
values already in hex (as produced by `rgb()`, `0xRRGGBB`, color expressions
and the xcolor/dvips names, which eval maps at parse time); pic gray shades;
the 148 CSS named colors via a static table. A color that cannot be reduced
(e.g. `currentColor`, a raw `hsl(…)` string, or an alpha form like
`#rrggbbaa`) is passed through verbatim and raises `scene_unsupported` —
consumers get the SVG's exact value, flagged.

## 9. Unrepresented SVG features (warned)

Each of these raises `Diagnostic { kind: "scene_unsupported", … }` with the
shape's source span when a scene is requested (and only then — the default
compile paths are untouched):

1. **TeX label glyphs without `outline`** — the line carries the literal and
   the ink metrics, not the typeset paths.
2. **Non-hex-resolvable colors** — passed through verbatim (§8).

Deliberately out of scope, documented rather than warned: the `animations` /
`interactions` manifests (own channel), the `type`-effect `<tspan>` splitting
(animation plumbing), and the SVG coordinate transform itself (the header
carries its inputs — §2).

## 10. Phase 2: parity by construction (not in this delivery)

Today the scene is a *second reader* of the same `ir::Drawing` the SVG backend
consumes; fidelity is enforced by tests (corpus coverage, raster round-trip,
id/order equality). The next step is inverting the dependency: make the SVG
backend itself consume the scene (scene → SVG serializer), so parity holds by
construction. What it would take:

- Port `svg.rs`'s emission order and defs machinery (hatch/gradient ids) to
  read scene descriptors — mostly mechanical; the geometry work (arrowheads,
  splines, arcs, text baselines) is already shared with the scene builder.
- A byte-compat shim for the serializer (`num` formatting, attribute order) to
  keep the corpus byte-identical — the risky part; needs the corpus
  byte-check at every step.
- The raster backends are downstream of the SVG string and come for free.

Estimated as a focused multi-PR effort comparable to the diagnostics refactor
(#191–#217): the blast radius is one file (`svg.rs`), but every byte of the
corpus pins it.

## 11. Binding status

| Binding | Status |
|---|---|
| Rust / CLI | this delivery |
| Python | this delivery (`rpic.scene`, `compile(..., scene=True)`) |
| JS/WASM | this delivery (`compile(src, {scene, flatten})`) |
| C | this delivery (flagged `rpic_compile_json` variant) |
| R | **gap**: rpic-r pins the published crates.io core (0.11.x) and only picks this up at its next engine bump; `rpic_scene()` lands there then. Tracked in rpic-r. |
