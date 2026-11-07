# rpic

JS/TS bindings for [rpic](https://github.com/milkway/rpic-lang) — the pic
graphics language compiled to **SVG** with animation and diagnostic manifests,
via WebAssembly. Works in the browser and in Node, ships TypeScript types.

```js
import * as rpic from '@strategicprojects/rpic';

await rpic.ready();                       // browser: wasm fetched automatically

const { svg, animations, diagnostics, warnings, objects } = rpic.compile('box "A"; arrow; box "B"');
document.querySelector('#stage').innerHTML = svg;

// animate with GSAP:
import { gsap } from 'gsap';
rpic.animate(document.querySelector('#stage'), animations, gsap);

// pre-rendered SVG (e.g. from `rpic --json` on the CLI)? Import just the
// player — a zero-import module that never touches the wasm compiler:
// import { animate } from '@strategicprojects/rpic/player';

// circuit library:
rpic.renderSvg('A:(0,0); B:(2,0)\nresistor(A,B)', { circuits: true });
```

### No bundler? Plain HTML via CDN

The package works straight from a CDN — compile in the page, or pre-render
with the CLI (`rpic --json fig.pic`) and import only the player (`animate()`
never touches the wasm):

```html
<div id="stage"></div>
<script type="module">
  import { ready, compile, animate } from
    'https://cdn.jsdelivr.net/npm/@strategicprojects/rpic@0.11.0/index.js';
  import { gsap } from 'https://cdn.jsdelivr.net/npm/gsap@3.13.0/+esm';
  await ready();                    // the .wasm is fetched from the CDN
  const { svg, animations } = compile('box "A"; arrow; box "B"\nanimate last box with "pop"');
  const stage = document.querySelector('#stage');
  stage.innerHTML = svg;
  animate(stage, animations, gsap);
</script>
```

The [animate docs](https://rpic.dev/docs/extensions/animate#quick-start--plain-html-no-build-step)
carry the full recipe, including the pre-rendered variant with classic
`<script>` tags (with SRI hashes) and the plugin files `move`/`morph`/
`scramble`/`wiggle` need.

### Node

```js
import { readFileSync } from 'node:fs';
import * as rpic from '@strategicprojects/rpic';
await rpic.ready(readFileSync(new URL('./node_modules/@strategicprojects/rpic/pkg/rpic_wasm_bg.wasm', import.meta.url)));
console.log(rpic.renderSvg('box "hi"'));
```

### TeX math labels (`texlabels`)

The default wasm build is lean and renders `$…$` labels as literal text (plus
a diagnostic). To typeset them exactly like the native CLI, opt into the
math-enabled build — a second, heavier `.wasm` (RaTeX + embedded KaTeX glyph
data) that is only fetched when you ask for it:

```js
await rpic.ready(undefined, { math: true }); // browser: fetches pkg/rpic_wasm_math_bg.wasm
rpic.renderSvg('box "$-\\frac{T}{2}$" fit', { texlabels: true });
```

| Build | `.wasm` | raw | gzip -9 |
| --- | --- | --- | --- |
| lean (default) | `pkg/rpic_wasm_bg.wasm` | 495 KB | 200 KB |
| math | `pkg/rpic_wasm_math_bg.wasm` | 3.05 MB | 1.15 MB |
| geo | `pkg/rpic_wasm_geo_bg.wasm` | 716 KB | 296 KB |

(v0.16.1; the math build is +2.55 MB raw, +0.95 MB gzipped, almost all of it
RaTeX and the KaTeX glyph outlines.)

**One build per module instance.** The first `ready()` call fixes the build.
A later call for the same build returns the same promise. A call for a
different build rejects (`already initialized the lean build; cannot switch to
math-enabled`) instead of silently reusing the first one, and
`{ math: true, geo: true }` always rejects. Only a *failed* init clears the
slot. So decide before the first call: lazy-load math when the source has a
`$…$` label, and keep the lean fast path otherwise. Switching builds later
takes a fresh module instance (a page reload). In Node, pass the math
artifact's bytes:
`ready(readFileSync(new URL('…/pkg/rpic_wasm_math_bg.wasm', import.meta.url)), { math: true })`.

Without the renderer (lean/geo builds), a `$…$` label is drawn literally and
`diagnostics` gains `texlabels: no math renderer in this build; … kept
literal`. If you build your own wasm on `rpic-core`, enable `rpic-render`'s
`math` feature and call `rpic_core::set_math_renderer` at module init. The
recipe and full rules are in
[docs/tex-labels.md § On wasm](https://github.com/milkway/rpic-lang/blob/main/docs/tex-labels.md#on-wasm).

### Maps (geopic: drawings as GeoJSON)

A third, opt-in build georeferences a drawing in the browser: the result is an
RFC 7946 `FeatureCollection` (WGS84) for Leaflet, MapLibre, OpenLayers…
The drawing is placed by `geo_lon`/`geo_lat`/`geo_scale`/`geo_bearing` in the
source, by control points, or by the options below (which override them) —
the contract is [docs/geopic.md](https://github.com/milkway/rpic-lang/blob/main/docs/geopic.md).

```js
await rpic.ready(undefined, { geo: true }); // browser: fetches pkg/rpic_wasm_geo_bg.wasm

const { geojson, warnings, registration } = rpic.compileGeoJSON(
  'geo_scale = 1000\narrow from Recife to Olinda\n"Recife" at Recife below',
  {
    anchor: [-34.8711, -8.0631],
    features: { Recife: [-34.8711, -8.0631], Olinda: [-34.8553, -8.0089] },
  }
);
L.geoJSON(geojson).addTo(map); // e.g. Leaflet
```

`features` names points the source may use without defining them: names the
source defines are control points of its own geometry, the others are
injected as places (or invisible boxes, given a `bbox: [w, s, e, n]`, so
`Zone.n` follows the extent). Names must be pic labels (`/^[A-Z][A-Za-z0-9_]*$/`)
and are validated before anything compiles. Options: `frame` (`'enu'` |
`'mercator'`), `anchor`, `scale` (m per inch), `bearing` (° clockwise from
north), `features`, `flatten`, `viewScale`, `screenClasses`, `circuits`,
`texlabels`. A georeferencing failure throws a `GeoError` whose `info` is
`{ kind, message, hint }` (`no_registration`, `invalid_anchor`, …); pic errors
throw the usual error with `errorInfo`. Nothing is dropped silently: every loss
(an unsupported style, ignored `animate` timelines…) is in `warnings`, next to
the engine's own warnings; pic `print` lines are in `diagnostics`. Numeric
options must be finite (`scale`/`flatten`/`viewScale` > 0) or the call throws.
`geoToCanvas(registration, [[lon, lat], …])` maps points back onto the canvas.
In Node, pass the bytes of `pkg/rpic_wasm_geo_bg.wasm` to `ready(…, { geo: true })`.

#### On a MapLibre map

`layers: true` adds the MapLibre style-spec layers that draw the collection
the way rpic.dev does — every stroke, dot and label sized **true to the
ground** from the collection's registration (source id `"pic"`;
[docs/geopic.md § 6](https://github.com/milkway/rpic-lang/blob/main/docs/geopic.md#6-rendering-on-maplibre)).
The subpath module `@strategicprojects/rpic/maplibre` has zero imports (no
wasm, no maplibre-gl dependency) and puts them on your map:

```js
import maplibregl from 'maplibre-gl';
import { attach, createGeoMap } from '@strategicprojects/rpic/maplibre';

const { geojson, layers } = rpic.compileGeoJSON(src, { layers: true });

// your own map: adds/updates source "pic" + the six layers (waits for `load`);
// before: 'labels' puts the shapes under the basemap's labels (the style's
// trailing block of symbol layers, above its roads and buildings), or pass a
// layer id; the drawing's own labels always stay on top, in the style's own
// font (font: 'style', the default: its most common text-font stack — or
// 'spec' for the engine's Noto placeholder, or an explicit stack)
const pic = attach(map, geojson, { layers, fit: true, before: 'labels' });
pic.setGeoJSON(next.geojson, next.layers); // live edits
pic.fitTo();                                // re-frame
pic.detach();                               // remove layers + source

// …or a ready map (OpenFreeMap positron basemap, zoom buttons, cooperative
// gestures); the maplibre-gl namespace is a parameter, not a dependency
const g = createGeoMap(maplibregl, 'map', { style, cooperativeGestures, navigation, before, font });
g.setGeoJSON(geojson, layers);
g.fitTo();
```

Larger labels: `compileGeoJSON(src, { layers: { textScale: 1.5 } })` (or
`maplibreLayers(fc, { textScale: 1.5 })`) multiplies every font size —
finite, in (0, 16], default 1 — and moves the upper zoom stop of
`text-size` so the largest label still fits MapLibre's 255 px limit;
strokes and dots stay true to the ground.

Without the geo build (a collection from the CLI or Python), omit `layers`:
the module computes the same ones with `layers(fc)` (or `layers(fc,
{ textScale })`), a JS port of the engine's function pinned to it by the
test suite. The CLI prints both at once: `rpic --geojson --maplibre map.pic`
→ `{ "geojson": …, "layers": […] }` (add `--text-scale 1.5` for larger
labels).

The specs' `text-font` (`["Noto Sans Regular"]`) is a placeholder: a glyph
server may serve a Noto with other vertical metrics (CARTO's does, and its
labels sat centred on their lines instead of beside them). `attach` therefore
gives `pic-text`/`pic-title` the style's own font by default; if you add the
layers yourself, set `layout['text-font']` to a stack your style uses.

## API

| Function | Description |
|----------|-------------|
| `ready(wasmInput?, {math?, geo?})` | Initialize WASM. Browser: no arg. Node: pass `.wasm` bytes/URL. `math: true` loads the math-enabled build, `geo: true` the geo-enabled one; conflicting later calls reject. |
| `compile(src, {circuits?, texlabels?, scene?, flatten?, outline?})` | → `{ svg, animations, diagnostics, warnings, objects, places?, passthrough?, scene? }` (throws on a pic error with `errorInfo`; `places` = named `Label: position` points in SVG units (with `block`, the scene's block path, when defined inside `[ … ]`), `passthrough` = troff `.` lines, each present only when the source has some). `scene: true` adds the device-independent resolved draw list ([docs/scene.md](https://github.com/milkway/rpic-lang/blob/main/docs/scene.md)); `flatten` (inches) delivers its curves as polylines. |
| `renderSvg(src, {circuits?, texlabels?})` | → SVG string. |
| `compileGeoJSON(src, {anchor?, scale?, bearing?, frame?, features?, layers?, …})` | Geo build only. → `{ geojson, warnings, diagnostics, registration, canvas, layers? }`; `layers: true` adds the MapLibre layers. Throws `GeoError` (`info.kind`) or a pic error. |
| `maplibreLayers(fc, {textScale?})` | Geo build only. The engine's MapLibre layers for a geopic FeatureCollection; `textScale` multiplies every label's font size. |
| `attach(map, fc, {layers?, fit?, before?, font?})` | `@strategicprojects/rpic/maplibre`. Source `"pic"` + layers on a MapLibre map (shapes under layer `before`, or under the basemap's labels with `'labels'`; the drawing's own labels stay on top, in the style's font unless `font: 'spec'` or a stack) → `{ setGeoJSON, fitTo, detach }`. Also `createGeoMap(maplibregl, container, opts)`, `layers(fc, {textScale?})`, `sizing(fc, {textScale?})`, `bounds(fc)`. |
| `geoToCanvas(registration, points)` | Geo build only. WGS84 `[lon, lat]` points → canvas `[x, y]` inches. |
| `animate(root, animations, gsap)` | Build/play a GSAP timeline (`draw`/`fade`/`pop`). Browser only. |

Compile errors keep a readable `message` and expose structured data for editors:

```js
try {
  rpic.compile('bxo');
} catch (err) {
  console.log(err.errorInfo); // { message, line, col, end_col, file, kind, found, expected, hint }
}
```

Positions are always relative to your own source — with `circuits: true` an
error on your line 1 reports `line: 1`, and a problem inside a `copy` include
carries the include's name in `file` (`null` means your own input).

PNG/PDF are available via the CLI, the Python package, or the R package (the
WASM core renders SVG; rasterization isn't bundled here).

## Rebuild

```sh
npm run build:wasm
npm test
```
