# Maps — a pic drawing placed on the Earth (geopic)

Give the plane a registration and the same drawing becomes geographic data: a GeoJSON
`FeatureCollection` in WGS84 (`rpic --geojson`), an `sf` data frame in R, a MapLibre
layer set with sizes true to the ground. Docs: https://rpic.dev/docs/maps and the
engine's `docs/geopic.md`.

## Registration (pic variables, or arguments of the tool)

| pic variable | meaning |
|---|---|
| `geo_lon`, `geo_lat` | where the pic origin `(0,0)` is |
| `geo_scale` | **metres per inch** of the drawing (default 100) |
| `geo_bearing` | azimuth of the drawing's +y axis, degrees clockwise from north |
| `geo_frame` | `"enu"` (local tangent plane, true distances; default) or `"mercator"` |

Or **control points**: a place `A: (x,y)` with `A_lon`/`A_lat`. One anchors; two fix
scale and bearing exactly; three or more are fitted (residual reported). Set `geo_*`
at top level, not inside `[ ]` (block-local assignments do not register).

Three scales meet here: pic `scale = 20` (pic units per inch, author in metres),
`geo_scale = 20` (metres per inch → pic units are metres), and text in points
(a label is `font_size/72 × geo_scale` metres tall on the ground — interactive maps
offer `text_scale` when labels come out small against a city).

## Recipes

A callout pinned to a place (one anchor, annotation kept at screen size with `class "screen"`):

```pic
geo_lon = -34.8711; geo_lat = -8.0631; geo_scale = 400
dot at (0,0) rad 0.04
B: box "Marco Zero" "Recife" wid 1.3 ht 0.5 at (1.4, 0.9) fill 0.95 class "screen"
arrow from B.sw to (0.06, 0.06) thick 1.2
```

A geodesic radius (every vertex is 2 000 m from the centre along the ellipsoid):

```pic
geo_lon = -34.8711; geo_lat = -8.0631; geo_scale = 1000
C: circle rad 2 at (0,0) dashed fill 0.97
line from C.c to C.e thin
"2 km" above at 0.5 <C.c, C.e>
```

A site plan in metres, registered by two surveyed corners (scale and bearing come out of
the fit, so the plan lands rotated to true north):

```pic
scale = 10; margin = 2.5
A: (0,0);   A_lon = -34.8716;  A_lat = -8.0608
B: (40,25); B_lon = -34.87155; B_lat = -8.06030
Yard: box wid 40 ht 25 with .sw at A
Shed: box "shed" wid 12 ht 8 with .sw at A + (3,3) fill 0.85
Tank: circle "tank" rad 4 at A + (30,17) fill 0.9
dot at A rad 0.4; dot at B rad 0.4
```

A drawing oriented with `geo_bearing` (a pitch in metres, laid at 55°):

```pic
geo_lon = -34.8947; geo_lat = -8.0636; geo_scale = 20; geo_bearing = 55
scale = 20; margin = 3
Field: box wid 105 ht 68 shaded "#e4efe4" outlined "#2e7d32"
line from Field.n to Field.s outlined "#2e7d32"
circle rad 9.15 at Field.c outlined "#2e7d32"
"105 m" below at Field.s + (0, -2)
```

## Getting it onto a map

| Tool | Call |
|---|---|
| CLI | `rpic --geojson fig.pic` → GeoJSON; `--maplibre` adds the layer specs (`--text-scale 1.5`); `--geo-anchor LON,LAT --geo-scale M --geo-bearing DEG --geo-feature Name=LON,LAT` override the source |
| R (`rpic` on CRAN) | `pic(src) \|> rpic_sf()` (sf data frame, snake_case columns `geopic_kind`, `stroke`, `fill`, `geopic_text`, `geopic_anchor` …); `ggplot() + rpic_layers(x)`; `pic(src) \|> autoplot()`; interactive: `maplibre(style = carto_style("positron")) \|> add_rpic(x, before = "labels", text_scale = 1.5)` (mapgl) |
| JS | `compileGeoJSON(src, { layers: true })` from `@strategicprojects/rpic`; `attach(map, fc, { before: 'labels', font: 'style' })` from `@strategicprojects/rpic/maplibre` |
| Python | `rpic.geo.to_geojson(src)`, `to_geodataframe(src)`, `maplibre_layers(fc)` |
| Studio | a source with `geo_*` or control points opens a **Map** tab (MapLibre, live), with GeoJSON download |

In R, a `.pic` file is read with `pic(paste(readLines("fig.pic"), collapse = "\n"))`;
`rpic_geojson()` returns the GeoJSON text (save it with `writeLines(gj, "fig.geojson")`).

Features can come from data instead of the source: R `rpic_sf(src, features = sf_object,
name_col = "NAME")` places `Wake`, `Mecklenburg` … as named points (centroids) with
bounding boxes (`Wake.n`), so `arrow from Mecklenburg to Wake` draws between counties.
Feature names must be pic labels (Capitalised identifier).

## What GeoJSON carries

Boxes/circles → polygons; lines/arrows → line strings (arrowheads separate features);
dots and texts → points; a shape's own label → a `title` point at its centre. Style
rides in simplestyle properties (`stroke`, `stroke-width` pt, `fill`) plus `geopic:*`
(`kind`, `id`, `text`, `anchor` compass, `rotation`, `font_size`, `placement`
= `screen` for annotations). What cannot be carried warns (`unsupported_style` for a
gradient), never silently.
