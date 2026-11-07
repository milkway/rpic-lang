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
const { geojson, warnings, registration, canvas } = compileGeoJSON(src, {
  anchor: [-34.8711, -8.0631], scale: 1000, bearing: 0, frame: 'enu',
  features: { Recife: [-34.8711, -8.0631], Olinda: [-34.8553, -8.0089] },
});
```

Both bindings inject host `features` the source does not define in two
passes (placed by the first pass's registration, as places or — with a
`bbox` — invisible boxes), validate feature names against the pic-label
pattern before anything compiles, and report errors structured (`GeoError`
with `info = {kind, message, hint}`). The wasm exports underneath are
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
| `title` | the item's text lines joined by `\n` |
| `geopic:schema` | `1` — the version of this vocabulary |
| `geopic:id`, `geopic:kind`, `geopic:z`, `geopic:label`, `geopic:block`, `geopic:class`, `geopic:link` | the scene item's fields |
| `geopic:text`, `geopic:rotation`, `geopic:anchor` | text content and placement |
| `geopic:dash`, `geopic:hatch` | the scene's descriptors, verbatim (not geometry) |
| `geopic:placement` | `"screen"` for `text`, `dot` and items whose `class` has a word in `screen_classes` (default `screen`); `"geo"` otherwise |
| `geopic:head`, `geopic:filled` | arrowhead features: which end, filled or open |
| `geopic:part` | path index when an item yields several features |
| `geopic:role` | `"fill"` on the polygon that carries an open stroke's fill |
| `geopic:radius_in` | `dot` radius, canvas inches |

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

Errors (`GeoError {kind, message, hint?}`): `no_registration`, `invalid_scale`
(`geo_scale ≤ 0`, non-finite bearing, effective scale — declared or fitted —
above 10⁹ m/in), `invalid_anchor` (`|lat| ≥ 90`, `|lon| > 180`, non-finite),
`invalid_flatten` (tolerance not a positive number), `view_scale_required`
(`geo_px = 1` without `view_scale`, only when the scale is not fitted),
`degenerate_control_points` (coincident points), `invalid_scene`. A label
that is redefined resolves to its **latest** definition, a top-level object
winning over a block-local one of the same name.

## 6. Versioning

`geopic:schema` is a single integer, **1**. Adding a property or a kind keeps
it; renaming, removing or changing the meaning or unit of a property bumps it
(release-noted; the old value is not emitted anymore). Consumers must ignore
properties they do not know. The input contract is the scene format, versioned
separately (`scene`).
