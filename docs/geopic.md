# geopic — pic drawings as GeoJSON

`geopic-core` reads an rpic **scene** ([docs/scene.md](scene.md)) and emits a
GeoJSON `FeatureCollection` (RFC 7946, WGS84). This document is the contract:
how the drawing is registered on the Earth, what each scene kind becomes, the
property vocabulary, the warnings, and how the format is versioned.

## 0. Invocation

```sh
rpic --geojson map.pic                      # FeatureCollection on stdout, warnings on stderr
rpic --geojson --geo-anchor -34.87,-8.06 --geo-scale 500 --geo-bearing 30 map.pic
rpic --geojson --geo-frame mercator --flatten 1e-4 map.pic
rpic --geojson --geo-feature Recife=-34.8711,-8.0631 --geo-feature Olinda=-34.8553,-8.0089 map.pic
```

`--geo-anchor LON,LAT`, `--geo-scale M`, `--geo-bearing DEG`, `--geo-frame
enu|mercator` and `--geo-view-scale PX` override the source's variables;
`--geo-feature NAME=LON,LAT` (repeatable) names a place or object of the
drawing as a control point; `--flatten` is the curve tolerance. The library
(`geopic_core::georeference` with `GeoOptions`) additionally takes
`screen_classes` and a feature `bbox`, which the CLI does not expose; the
Python binding (`rpic.geo.compile` / `to_geojson` / `to_geodataframe`) and
the JS/TS package mirror the library. In JS, geopic lives in a third,
lazily fetched wasm build (DECISIONS G8):

```js
import { ready, compileGeoJSON, geoToCanvas } from '@strategicprojects/rpic';
await ready(undefined, { geo: true });   // fetches pkg/rpic_wasm_geo_bg.wasm
const { geojson, warnings, diagnostics, registration, canvas } = compileGeoJSON(src, {
  anchor: [-34.8711, -8.0631], scale: 1000, bearing: 0, frame: 'enu',
  features: { Recife: [-34.8711, -8.0631], Olinda: [-34.8553, -8.0089] },
});
```

Both bindings inject host `features` the source does not define in two
passes (placed by the first pass's registration, as places or — with a
`bbox` — invisible boxes), validate feature names against the pic-label
pattern before anything compiles, report the engine's warnings and `print`
lines (`diagnostics`) next to the geo warnings with the caller's line numbers
(a position inside the injected lines carries `file: "features"`), and
report errors structured (`GeoError` with `info = {kind, message, hint}`). The wasm exports underneath are
`compile_geo(src, circuits, texlabels, opts_json)` and
`geo_to_canvas(registration_json, points_json)`.
Examples with their expected output live in `examples/geo/*.pic` +
`*.geojson`, guarded by `crates/cli/tests/geo.rs`.

## 1. Registration

A **similarity** with four parameters maps canvas inches to a plane in metres,
and a **frame** carries plane points to the ellipsoid.

| Source | Meaning |
|---|---|
| `geo_lon`, `geo_lat` | WGS84 position of the pic origin `(0, 0)` |
| `geo_scale` | metres per inch of the drawing *as authored* (default 100). The scene's `rescale` (`.PS <width>`, `maxpswid`) is folded in automatically |
| `geo_bearing` | azimuth of the canvas +y axis, degrees clockwise from north (default 0) |
| `geo_frame` | `0` = `enu` (default), `1` = `mercator` |
| `geo_px` | `1` = lengths are pixels at the caller's `view_scale` (px per inch); `geo_scale` is then metres per pixel |
| `<Label>_lon`, `<Label>_lat` | a **control point**: the place `Label: …` (or the object labelled `Label`) sits at that position |

These are read from the scene's top-level `vars`, so assign them **at the top
level** of the source. As in dpic, a `=` inside `[ … ]` creates a variable local
to the block: `[ geo_lon = -34.87; geo_lat = -8.06; box ]` registers nothing
(error `no_registration`, whose hint says so). `:=` inside a block assigns an
existing top-level variable and does propagate (`geo_lon = 0` … `[ geo_lon :=
-34.87 ]`). `if`/`for` bodies are not scopes; assignments there count
(DECISIONS S14).

Caller options (`GeoOptions`; `anchor=`, `scale=`, `bearing=`, `frame=`, …)
override the variables, like `-D` in troff. Host-provided **features**
(`name, lon, lat[, bbox]`) whose name matches a label are control points too,
and win over a source `<Label>_lon`/`<Label>_lat` of the same name (warning
`control_point_overridden`).

Registration method, by number of resolved control points:

| Control points | Method | Scale and bearing |
|---|---|---|
| 0 | `anchor` — needs `geo_lon`/`geo_lat` (else error `no_registration`) | declared |
| 1 | `anchor` at that point (a declared anchor is ignored, warning `ignored_with_control_points`) | declared |
| 2 | `two_points` — closed form (exact) | fitted; declared ones ignored with a warning |
| ≥ 3 | `helmert` — least squares; `rms_m` reported; warning `registration_residual` when the RMS exceeds 1 % of the drawing's extent | fitted |

Frames:

- **`enu`** — local tangent plane at the anchor. Plane offsets go to the
  ellipsoid by the geodesic direct problem (Karney, via `geographiclib-rs`),
  so a straight segment through the anchor is a geodesic and distances are
  true metres. Bearings are relative to the anchor's meridian.
- **`mercator`** — a similarity in EPSG:3857 metres and the Web Mercator
  inverse: straight lines of the drawing stay straight on a web map; lengths
  are Mercator metres (true at the equator, inflated by `1/cos φ`).

`registration` in the result: `{frame, anchor:{lon,lat}, origin:[x,y],
scale_m_per_in, bearing_deg, method, control_points, rms_m?}`.

## 2. Densification

Before de-projection every straight segment is subdivided in the plane so no
step exceeds 100 m (`⌈L / 100 m⌉` pieces — a 100 m chord lies 0.2 mm off its
geodesic); cubic curves are flattened to the `flatten` tolerance (inches,
default `1e-3`) first; circles and ellipses get
enough vertices that the sagitta stays within `flatten` **and** no chord
exceeds 100 m in the plane (at least 32, at most 4096; rounded box corners a
quarter of that each). A 20 km line therefore stays within
1 m of the geodesic it represents (test A4), whereas its two endpoints joined
straight in longitude/latitude would not.

Resource caps (a hostile scene cannot amplify the output): at most 100 000
vertices per segment and **2 000 000 per scene across every producer** —
curve flattening, circle/box rings and segment densification draw from one
budget; beyond it, curves keep their chords, rings their minimal polygon and
segments their endpoints, and the warning `densification_budget` is raised.
An effective scale above 10⁹ m per inch is the error `invalid_scale`. Output
size is otherwise proportional to the input (its own vertices are never
multiplied beyond the budget).

## 3. Features by kind

| Scene kind | Geometry | Notes |
|---|---|---|
| `box` | `Polygon` | corner radius honoured (8 segments per corner) |
| `circle`, `ellipse` | `Polygon` | |
| `line`, `arrow` | `LineString`; `Polygon` when `closed` | arrowheads are **separate** features, `geopic:kind = "arrowhead"`, same `geopic:id`; filled heads are polygons in the stroke color, open heads line strings. An **open stroke with a fill** (the SVG paints the implicitly closed area) yields an extra `Polygon` with `geopic:role = "fill"` — from the scene's untrimmed `fillPoints`/`fillPaths` when present — and the LineString keeps the stroke properties only |
| `spline`, `arc`, `brace` | `LineString`; `Polygon` when the path is closed | one feature per path (`geopic:part` when more than one) |
| `dot` | `Point` | `geopic:radius_in` |
| `text` | `Point` at the object's anchor | `geopic:text`, `geopic:rotation` (degrees **counter-clockwise from the map's east**: the canvas rotation minus the bearing), `geopic:anchor` |
| shape text | `Point`, `geopic:kind = "title"`, right after the shape's own features (arrowheads included) | every non-`text` item that carries text lines (a box's `"label"`, a line's) also yields this point, at the scene's `text_center` — the centre the SVG lays the label out around (an older scene without it: the item's `at`, else no point). `geopic:id` = `<shape id>:title`, `geopic:for` = the shape's id; `geopic:text`, `geopic:anchor` (the first line's compass: `above` → `s`, `ljust` → `w`), `geopic:font_size`, `geopic:font_weight`/`font_style`/`font_family`, `geopic:rotation` as a `text` item; `geopic:z`/`label`/`block`/`class`/`link`/`placement` as the shape's; no stroke/fill. The shape keeps its `title`; arrowheads carry no label keys (DECISIONS G10) |
| `block` | — | no feature; children carry `geopic:block` (the block path) |
| hidden items | — | paint nothing, produce nothing (no warning) |

Coordinates are `[lon, lat]`, rounded to 9 decimals. Reproducibility across platforms is at the millimetre, not the byte: the geodesic code uses the platform's `sin`/`cos`/`atan2`, whose last bits differ between macOS, Linux and Windows, and a control-point fit can carry that into the 9th decimal — the example guard therefore compares within 1e-8°.

## 4. Properties

simplestyle-spec keys, then the `geopic:` namespace:

| Property | Source |
|---|---|
| `stroke`, `stroke-width` | `stroke.color` (hex), `stroke.width` (points) |
| `fill` | `fill.color`; for a gradient, the first stop's color (warning `unsupported_style`) |
| `stroke-opacity`, `fill-opacity` | the item's `opacity` |
| `title` | the item's **visible** text lines joined by `\n` (blank lines of the stack left out — a popup has no use for them) |
| `geopic:schema` | `1` — the version of this vocabulary |
| `geopic:id`, `geopic:kind`, `geopic:z`, `geopic:label`, `geopic:block`, `geopic:class`, `geopic:link` | the scene item's fields |
| `geopic:text`, `geopic:rotation`, `geopic:anchor` | text content and placement. `geopic:text` holds **all rows of the author's stack**, joined by `\n`, blank ones as empty strings (`box "" "below"` → `"\nbelow"`, `box "a" "" "c"` → `"a\n\nc"`), rebuilt from the scene's `row`/`text_rows` (scene DECISIONS S13), so a renderer that centres the block on the point draws each line on its SVG row; a scene without them joins the visible lines |
| `geopic:font_size` | the first text line's font size, **points**; with the collection's `geopic.registration.scale_m_per_in`, `font_size / 72 × scale_m_per_in` is the letter height in metres — size labels true to the ground, in proportion to the shapes, at any zoom |
| `geopic:font_weight`, `geopic:font_style`, `geopic:font_family` | the first text line's face, each **present only when set**: `"bold"` for `bold`, `"italic"` for `italic`, and the family (`"monospace"` for `mono`, the name given to `font "…"`). As with the size, the first line speaks for the stack. A renderer picks a face of its own from them (§ 6, DECISIONS G12) |
| `geopic:dash`, `geopic:hatch` | the scene's descriptors, verbatim (not geometry) |
| `geopic:placement` | `"screen"` for `text`, `dot` and items whose `class` has a word in `screen_classes` (default `screen`); `"geo"` otherwise |
| `geopic:for` | `title` points: the `geopic:id` of the shape whose text they carry |
| `geopic:head`, `geopic:filled` | arrowhead features: which end, filled or open |
| `geopic:part` | path index when an item yields several features |
| `geopic:role` | `"fill"` on the polygon that carries an open stroke's fill |
| `geopic:radius_in` | `dot` radius, canvas inches |

The `FeatureCollection` itself carries a foreign member (RFC 7946 §6.1)
`"geopic": {"schema": 1, "registration": {frame, anchor, origin,
scale_m_per_in, bearing_deg, method, control_points, rms_m?}}` — the
similarity that placed the drawing, so a consumer can convert any pic
length (stroke widths in points, dot radii in inches, font sizes) into
metres without re-deriving it.

The machine-readable contract is [`geopic.schema.json`](geopic.schema.json).

## 5. Warnings and errors

Warnings (`{kind, message, id?, span?}`), never silent:

| kind | when |
|---|---|
| `unsupported_style` | gradient fill (first stop used) |
| `ignored` | math label kept as its TeX literal; an item without the geometry its kind requires; an arrowhead without geometry; `view_scale` given without `geo_px = 1` |
| `unsupported_kind` | a scene kind this version does not map (the exhaustiveness test keeps this empty for format 1) |
| `ignored_with_control_points` | declared anchor/scale/bearing superseded by control points |
| `control_point_unresolved` | `<L>_lon` without `<L>_lat`, or no place/object labelled `L`; a lone `geo_lon`/`geo_lat`; a host feature naming no label, or given twice |
| `control_point_overridden` | the source's `<L>_lon`/`<L>_lat` lost to the host's feature of the same name (the caller wins) |
| `registration_residual` | Helmert RMS above 1 % of the drawing's extent |
| `invalid_frame` | `geo_frame` not 0/1 (ENU used) |
| `densification_budget` | the 2 000 000-vertex budget ran out; later segments carry endpoints only |
| `scene_inconsistent` | an item's text `row`/`text_rows` (scene S13) are present but contradict each other (a row outside the stack, out of order, a line without `row`, `row` without `text_rows`): `geopic:text` falls back to the visible lines joined. A scene with neither key (an older engine) is not warned |

Errors (`GeoError {kind, message, hint?}`): `no_registration`, `invalid_scale`
(`geo_scale ≤ 0`, non-finite bearing, effective scale — declared or fitted —
above 10⁹ m/in), `invalid_anchor` (`|lat| ≥ 90`, `|lon| > 180`, non-finite),
`invalid_flatten` (tolerance not a positive number), `view_scale_required`
(`geo_px = 1` without `view_scale`, only when the scale is not fitted),
`degenerate_control_points` (coincident points), `invalid_scene`. A label
that is redefined resolves to its **latest** definition, a top-level object
winning over a block-local one of the same name.

## 6. Rendering on MapLibre

`geopic_core::maplibre::layers(&fc)` turns a collection into the
[MapLibre style-spec](https://maplibre.org/maplibre-style-spec/layers/)
layers that draw it — styling as data, emitted once by the engine so the
rpic.dev Maps page, the Studio, other sites and R/mapgl draw the same map
(DECISIONS G9). Every layer reads one GeoJSON source whose id is always
**`pic`**: add the collection as that source, then add the layers as they
are, in order.

| id | type | filter | styling |
|---|---|---|---|
| `pic-fill` | `fill` | `Polygon` | `fill-color` = `fill` (default `#000000`); `fill-opacity` = `fill-opacity` (default 0.55) when the feature has `fill`, else 0 |
| `pic-stroke` | `line` | `Polygon` or `LineString`, has `stroke`, no `geopic:dash` | `line-color` = `stroke`, `line-opacity` = `stroke-opacity` (default 1), `line-width` (below), round cap and join |
| `pic-stroke-dashed` | `line` | same, has `geopic:dash` | as `pic-stroke` + `line-dasharray [3, 2]` |
| `pic-dot` | `circle` | `geopic:kind == "dot"` | `circle-color` = `fill` (default `#000000`), `circle-radius` (below) |
| `pic-text` | `symbol` | `geopic:kind == "text"` | `text-field` = `geopic:text`, a trailing `\n` padded with a no-break space (`["case", ["==", ["slice", T, ["max", 0, ["-", ["length", T], 1]]], "\n"], ["concat", T, "\u00a0"], T]`, `T` = `["get", "geopic:text"]` — MapLibre's shaper drops a final newline, the padding keeps the last blank row; the shaper trims the space, so the row stays empty), `text-font` = a `case` on the label's face — `geopic:font_weight == "bold"` and `geopic:font_style == "italic"` → `["Noto Sans Bold"]`, bold → `["Noto Sans Bold"]`, italic → `["Noto Sans Italic"]`, else `["Noto Sans Regular"]` (each a `["literal", […]]`; see *Font* below), `text-size` (below), `text-line-height 1` (pic's line skip is the font size; MapLibre's default 1.2 em would push each row 20 % further from the point), `text-anchor` from `geopic:anchor` (`c` center, `n` top, `s` bottom, `e` right, `w` left, `ne` top-right, `nw` top-left, `se` bottom-right, `sw` bottom-left; default center), `text-rotate` = −`geopic:rotation`, rotation and pitch alignment `map`, overlap allowed, placement ignored; text `#0f172a`, halo `#ffffff` 1.5 px |
| `pic-title` | `symbol` | `geopic:kind == "title"` (the shapes' title points) | exactly as `pic-text`. A symbol on the polygon itself would be placed once per vector tile the polygon is clipped into — a box straddling a tile boundary showed its title twice (#435) — so the label is a point of its own |

**Sizes are true to the ground.** From the collection's
`geopic.registration` take `m` = `scale_m_per_in` (100 when absent) and
`φ` = `anchor.lat` under `enu`, **0 under `mercator`** (and when absent):
Mercator metres are already the map's own, inflated by 1/cos φ, so the cosine
drops out. With MapLibre's 512 px tiles the equator spans 512 px at zoom 0,
so a ground length of `L` metres is, at zoom `z`,

    px(L, z) = L · p₀ · 2^z,    p₀ = 1 / (78271.517 · cos φ)

A pic length becomes metres as `pt · m / 72` (stroke widths, font sizes, in
points) or `in · m` (dot radii, in inches). MapLibre lets `zoom` drive only a
top-level `interpolate`, so each size is `["interpolate", ["exponential", 2],
["zoom"], z₀, px(L, z₀), z₁, px(L, z₁)]`, which reproduces the 2^z law
exactly between the stops:

| property | metres `L` (feature default) | stops | clamp |
|---|---|---|---|
| `line-width` | `stroke-width` pt (0.8) · m / 72 | z 0 and 24 | ≥ 1 px |
| `circle-radius` | `geopic:radius_in` in (0.04) · m | z 0 and 24 | ≥ 2 px |
| `text-size` | `geopic:font_size` pt (11) · m / 72 | z 0 and `z_text` | ≥ 1 px; ≤ 255 px at `z_text` |

Symbol sizes are packed into 16-bit vertex attributes (≈ 255 px), so a far
zoom stop with thousands of pixels overflows and the text comes out tiny. The
upper text stop is therefore per collection: with `F` the largest
`geopic:font_size · m / 72` over the features (at least 10⁻⁶ m),

    z_text = clamp(⌊log₂(255 / (F · p₀))⌋, 1, 24)

— exact 2^z scaling up to the last zoom at which the largest label still fits,
a frozen size beyond (far past the fitted zoom). Example: 400 m/in at
φ = −8.0631° gives p₀ = 1.2903602 × 10⁻⁵ px/m; an 11 pt label is 61.1 m
and `z_text` = 18. Integer constants are emitted as JSON integers.

**Options** (`geopic_core::maplibre::LayerOptions`, `layers_with(&fc,
&opts)`; DECISIONS G11). The default is the ground-true output above, to the
bit.

| option | type, range (default) | effect |
|---|---|---|
| `text_scale` | finite number in (0, 16] (1) | multiplies every font size: the `text-size` metres become `geopic:font_size · m / 72 · text_scale` (the factor is folded into the constant), and `F` in `z_text` is taken over the *scaled* sizes — so at ×2 the upper stop is one zoom lower and the largest label still fits in 255 px. Strokes and dots are unchanged |

Out of range, not a number, or an unknown key: a `GeoError` of kind
`invalid_options`. The JSON entry point `layers_json_with(fc_json,
opts_json)` takes `{"text_scale": 1.5}` (every key optional; an empty string
is the default); `bundle_json_with` is the CLI's bundle under options.

*Placement is not an option of the specs.* Where the layers go in the
consumer's style — `map.addLayer(spec, beforeId)` — belongs to the map, not
to the layer objects: the JS `attach(map, fc, { before })` takes a layer id
or `'labels'` — the style's layer whose id is `labels` when there is one,
otherwise the start of the current style's *trailing* block of `symbol`
layers: the first layer after the last non-`symbol` layer, ours ignored
(resolved when the layers are added; on top when no symbol layer follows
the last geometry layer). Not the first `symbol` layer: styles interleave
some labels with the geometry — CARTO's positron has `waterway_label` at
index 13 and its roads, buildings and boundaries up to 67, so the first
symbol would put the shapes under the roads and buildings. Only the
geometry layers (`pic-fill`, `pic-stroke`, `pic-stroke-dashed`, `pic-dot`)
go there, so the basemap's place names stay above the shapes; the
drawing's own labels (`pic-text`, `pic-title`) are always appended on top —
a basemap "Olinda" must not cover the drawing's "Olinda". rpic.dev uses
`before: 'labels'`.

*Font.* The specs' `text-font` names are placeholders, not a requirement: the
glyphs come from the consumer style's server, and the same name can carry
other vertical metrics there — on CARTO's positron (MapLibre 5.22) a Noto
label's box sat centred on its anchor, a `pic-text` label on the line it
annotates; the style's own Montserrat put it beside, with the right gap
(OpenFreeMap's Noto behaves, which hid it on rpic.dev). Consumers should
follow the style's font. `attach(map, fc, { font })` does: `'style'`
(default) = the most common `text-font` stack among the style's own symbol
layers whose first face is Regular or Medium (not Italic) — a basemap's most
common stack is often its Bold place-name stack, and the drawing's labels must
not come out bold (stacks compared whole, `["literal", […]]` unwrapped, other
expressions skipped; with no plain stack, the most common of all; the same
rule rpic-r's `add_rpic(font = "style")` applies), set on copies of
`pic-text`/`pic-title` when they are added — the spec's when the style has
no such layer; `'spec'` keeps the placeholders; a stack is used as given, for
every label.

*Faces (DECISIONS G12).* A label written `bold` or `italic` in the pic source
carries `geopic:font_weight` / `geopic:font_style`, and `text-font` chooses a
stack from them, feature by feature (only the stacks a drawing uses are
fetched). The specs name `Noto Sans Bold` and `Noto Sans Italic`, faces of
the OpenMapTiles font set OpenFreeMap serves; that set has no bold italic, so
a bold italic label asks for the Bold face — a spec must not name a font its
reference glyph server answers with 404, which fails the whole tile's labels.
Under `font: 'style'`, `attach` rebuilds the expression with the style's own
faces: among the symbol layers' stacks whose first face is of the regular
stack's family (the name minus its trailing weight and slant words), bold =
`Bold`, else a `SemiBold`/`DemiBold`, else a heavier face; italic = a slanted
face of plain weight (`Italic`, `Medium Italic`; not `Light Italic`); bold
italic = a face with both. The most common stack of each wins; a face the
style does not use falls back to the regular stack (bold italic: to the bold,
then the italic) — a font the style never asks for may not be on its glyph
server. When every face resolves to the regular stack, `text-font` is that
plain stack. `font: { regular, bold?, italic?, boldItalic? }` names the
stacks outright (same fallbacks), for a glyph server known to have them.
`geopic:font_family` is carried for consumers that can honour it (a web
renderer with its own fonts, a GIS); the MapLibre layers do not read it —
basemap glyph servers rarely have a monospace face or an arbitrary family,
so `mono` and `font "…"` labels come out in the regular family. The JS port
exports `FONTS` and `textFont(faces)`; the engine, `maplibre::FONTS` and
`maplibre::text_font`.

Exposed everywhere the collection is:

```sh
rpic --geojson --maplibre map.pic     # {"geojson": FeatureCollection, "layers": [...]}
rpic --geojson --maplibre --text-scale 1.5 map.pic   # labels 1.5× larger
```

```js
const { geojson, layers } = compileGeoJSON(src, { layers: true });   // or { layers: { textScale: 1.5 } }
import { attach } from '@strategicprojects/rpic/maplibre';
attach(map, geojson, { layers, fit: true, before: 'labels' });   // source "pic" + the layers, under the labels
maplibreLayers(fc, { textScale: 1.5 });       // from any geopic collection
```

```python
from rpic import geo
out = geo.compile(src, layers=True)           # out["layers"]; text_scale=1.5 for larger labels
layers = geo.maplibre_layers(fc, text_scale=1.0)   # from any geopic collection
```

`maplibre_layers` / `layers_json` read only the `geopic.registration`
foreign member and the features' `geopic:font_size`. The zero-dependency JS
module `@strategicprojects/rpic/maplibre` also carries a port of the same
function (`layers(fc, { textScale })`, and `sizing`), used when no engine
output is at hand; a test pins it to the engine at scales 1 and 1.5. The snapshots in `examples/geo/maplibre/*.layers.json` (the
four examples, the callout under `mercator`, an empty collection) are the
layer specs the rpic.dev maps drew before the styling moved here, and every
implementation is checked against them.

## 7. Versioning

`geopic:schema` is a single integer, **1**. Adding a property or a kind keeps
it; renaming, removing or changing the meaning or unit of a property bumps it
(release-noted; the old value is not emitted anymore). Consumers must ignore
properties they do not know. The input contract is the scene format, versioned
separately (`scene`).
