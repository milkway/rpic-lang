// rpic on MapLibre — draw a geopic FeatureCollection on a MapLibre GL map.
//
// This module has ZERO imports: it never touches the wasm compiler nor
// maplibre-gl itself (pass your `maplibregl` namespace to `createGeoMap`, or
// your own map to `attach`), so it adds nothing to a bundle that already has
// MapLibre. The layers normally come from the engine — `compileGeoJSON(src,
// { layers: true }).layers`, `rpic --geojson --maplibre`, Python
// `rpic.geo.maplibre_layers(fc)` — and `layers(fc)` below is a line-for-line
// port of `geopic_core::maplibre` (docs/geopic.md § 6, DECISIONS G9) for
// callers that have only the collection; a test pins the two together.

/** The GeoJSON source id every layer reads. */
export const SOURCE = 'pic';

/** The layer ids, in draw order. */
export const LAYER_IDS = ['pic-fill', 'pic-stroke', 'pic-stroke-dashed', 'pic-dot', 'pic-text', 'pic-title'];

/** The drawing's own label layers: `before` never buries them (they stay on top). */
const TEXT_LAYER_IDS = ['pic-text', 'pic-title'];

/** pic compass anchor → MapLibre text-anchor (the reference point sits at
 *  that side of the text: `w` = text extends right of it, `n` = below). */
const ANCHORS = {
  c: 'center', n: 'top', s: 'bottom', e: 'right', w: 'left',
  ne: 'top-right', nw: 'top-left', se: 'bottom-right', sw: 'bottom-left',
};

/** The largest accepted `textScale` (the engine's `LayerOptions::MAX_TEXT_SCALE`). */
export const MAX_TEXT_SCALE = 16;

/** `opts.textScale` (default 1), validated as the engine does (DECISIONS G11). */
function textScaleOf(opts) {
  const k = opts?.textScale ?? 1;
  if (!(typeof k === 'number' && Number.isFinite(k) && k > 0 && k <= MAX_TEXT_SCALE)) {
    throw new Error(`rpic: textScale must be a finite number in (0, ${MAX_TEXT_SCALE}], got ${k}`);
  }
  return k;
}

/**
 * The numbers the size expressions are built from: metres per inch and the
 * latitude from the collection's `geopic.registration` (cos φ drops out under
 * `mercator`), pixels per ground metre at zoom 0 (MapLibre's 512 px tiles:
 * 1 / (78271.517 · cos φ)), and the upper zoom stop of `text-size` — the last
 * zoom at which the largest font, multiplied by `opts.textScale` (default 1),
 * still fits in MapLibre's 255 px symbol limit.
 */
export function sizing(fc, opts = {}) {
  const textScale = textScaleOf(opts);
  const reg = fc?.geopic?.registration;
  const mPerIn = reg?.scale_m_per_in ?? 100;
  const lat = reg?.frame === 'mercator' ? 0 : (reg?.anchor?.lat ?? 0);
  const pxPerMetreAtZ0 = 1 / ((78271.517 * Math.cos((lat * Math.PI) / 180)) || 1);
  let maxFontM = 1e-6; // a loop, not a spread: collections can hold 10^5+ features
  for (const f of fc?.features ?? []) {
    maxFontM = Math.max(maxFontM, (Number(f.properties?.['geopic:font_size']) || 0) * (mPerIn / 72) * textScale);
  }
  const z = Math.floor(Math.log2(255 / (maxFontM * pxPerMetreAtZ0)));
  const textZHi = Number.isNaN(z) ? 1 : Math.min(24, Math.max(1, z));
  return { mPerIn, lat, pxPerMetreAtZ0, textZHi, textScale };
}

/**
 * The six MapLibre style-spec layers (source `"pic"`) that draw `fc` with
 * sizes true to the ground, in draw order: pic-fill, pic-stroke,
 * pic-stroke-dashed, pic-dot, pic-text, pic-title. `opts.textScale`
 * multiplies every label's font size. Same output as the engine's
 * `geopic_core::maplibre::layers_with`.
 */
export function layers(fc, opts = {}) {
  const { mPerIn, pxPerMetreAtZ0, textZHi, textScale } = sizing(fc, opts);
  // only a top-level interpolate may read zoom; exponential base 2 between
  // two stops reproduces the 2^z law exactly
  const groundPx = (metres, floorPx) => [
    'interpolate', ['exponential', 2], ['zoom'],
    0, ['max', floorPx, ['*', metres, pxPerMetreAtZ0]],
    24, ['max', floorPx, ['*', metres, pxPerMetreAtZ0 * 2 ** 24]],
  ];
  const groundTextPx = (metres) => [
    'interpolate', ['exponential', 2], ['zoom'],
    0, ['max', 1, ['*', metres, pxPerMetreAtZ0]],
    textZHi, ['min', 255, ['max', 1, ['*', metres, pxPerMetreAtZ0 * 2 ** textZHi]]],
  ];
  const ptToM = (expr) => ['*', expr, mPerIn / 72];
  // font points → metres, × textScale folded into the constant (exact at 1)
  const fontToM = (expr) => ['*', expr, (mPerIn / 72) * textScale];
  const inToM = (expr) => ['*', expr, mPerIn];
  // pic strokes are hairlines: ground-true width, never thinner than 1 px
  const lineWidth = () => groundPx(ptToM(['coalesce', ['get', 'stroke-width'], 0.8]), 1);
  const dotRadius = groundPx(inToM(['coalesce', ['get', 'geopic:radius_in'], 0.04]), 2);
  const textSize = () => groundTextPx(fontToM(['coalesce', ['get', 'geopic:font_size'], 11]));

  const isPolygon = () => ['==', ['geometry-type'], 'Polygon'];
  const isLine = ['==', ['geometry-type'], 'LineString'];
  const kind = () => ['get', 'geopic:kind'];
  const textPaint = () => ({ 'text-color': '#0f172a', 'text-halo-color': '#ffffff', 'text-halo-width': 1.5 });
  const stroke = (id, dashed) => ({
    id, type: 'line', source: SOURCE,
    filter: ['all', ['any', isPolygon(), isLine], ['has', 'stroke'], dashed ? ['has', 'geopic:dash'] : ['!', ['has', 'geopic:dash']]],
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: {
      'line-color': ['get', 'stroke'],
      'line-width': lineWidth(),
      'line-opacity': ['coalesce', ['get', 'stroke-opacity'], 1],
      ...(dashed ? { 'line-dasharray': [3, 2] } : {}),
    },
  });
  // geopic:text keeps the author's blank rows (G10 addendum), but MapLibre's
  // shaper only breaks *before* a following character, so a final "\n" is
  // dropped and `box "above" ""` would render centred: a trailing newline is
  // padded with a no-break space (trimmed by the shaper, so the row stays
  // empty but is kept)
  const textField = () => {
    const t = ['get', 'geopic:text'];
    return ['case', ['==', ['slice', t, ['max', 0, ['-', ['length', t], 1]]], '\n'], ['concat', t, '\u00a0'], t];
  };
  const symbol = (id, k) => ({
    id, type: 'symbol', source: SOURCE, filter: ['==', kind(), k],
    layout: {
      'text-field': textField(),
      'text-font': ['Noto Sans Regular'],
      'text-size': textSize(),
      // pic's line skip is the font size; MapLibre's default 1.2 em would
      // push every row 20 % off text_center
      'text-line-height': 1,
      'text-anchor': ['match', ['get', 'geopic:anchor'], ...Object.entries(ANCHORS).flat(), 'center'],
      // geopic:rotation is CCW from east; MapLibre rotates clockwise
      'text-rotate': ['*', -1, ['coalesce', ['get', 'geopic:rotation'], 0]],
      'text-rotation-alignment': 'map',
      'text-pitch-alignment': 'map',
      'text-allow-overlap': true,
      'text-ignore-placement': true,
    },
    paint: textPaint(),
  });
  return [
    {
      id: 'pic-fill', type: 'fill', source: SOURCE, filter: isPolygon(),
      paint: {
        'fill-color': ['coalesce', ['get', 'fill'], '#000000'],
        'fill-opacity': ['case', ['has', 'fill'], ['coalesce', ['get', 'fill-opacity'], 0.55], 0],
      },
    },
    stroke('pic-stroke', false),
    stroke('pic-stroke-dashed', true),
    {
      id: 'pic-dot', type: 'circle', source: SOURCE, filter: ['==', kind(), 'dot'],
      paint: { 'circle-radius': dotRadius, 'circle-color': ['coalesce', ['get', 'fill'], '#000000'] },
    },
    symbol('pic-text', 'text'),
    // text attached to a shape (a box's label): its own Point at the shape's
    // centre (DECISIONS G10), styled like free text
    symbol('pic-title', 'title'),
  ];
}

/** `[[west, south], [east, north]]` of the collection, or null when empty. */
export function bounds(fc) {
  let w = Infinity, s = Infinity, e = -Infinity, n = -Infinity;
  const take = ([x, y]) => {
    w = Math.min(w, x); e = Math.max(e, x); s = Math.min(s, y); n = Math.max(n, y);
  };
  for (const f of fc?.features ?? []) {
    const g = f.geometry;
    if (!g) continue;
    if (g.type === 'Point') take(g.coordinates);
    else if (g.type === 'LineString') g.coordinates.forEach(take);
    else if (g.type === 'Polygon') g.coordinates[0].forEach(take);
  }
  return Number.isFinite(w) ? [[w, s], [e, n]] : null;
}

const EMPTY = { type: 'FeatureCollection', features: [] };

/**
 * The layer id the pic geometry layers are inserted before, resolved now from
 * the map's current style: `'labels'` = a layer whose id really is `labels`
 * when the style has one, else the first `symbol` layer that is not one of
 * ours (the basemap's place names, road labels…); any other string = that
 * layer, when the map has it. `undefined` (append on top) otherwise.
 */
function beforeId(map, before) {
  if (before == null) return undefined;
  if (before === 'labels') {
    if (map.getLayer('labels')) return 'labels'; // a style with a real `labels` layer
    const style = typeof map.getStyle === 'function' ? map.getStyle() : null;
    const first = (style?.layers ?? []).find((l) => l.type === 'symbol' && !LAYER_IDS.includes(l.id));
    return first?.id;
  }
  if (map.getLayer(before)) return before;
  console.warn(`rpic: attach: no layer "${before}" on the map; the pic layers go on top`);
  return undefined;
}

/**
 * Draw `fc` on a MapLibre `map`: adds (or updates) the GeoJSON source `"pic"`
 * and the six layers. `layers` are the engine's (`compileGeoJSON(src,
 * { layers: true }).layers`); omitted, they are computed by `layers(fc)`.
 * `fit: true` frames the drawing. `before` places the *geometry* layers
 * (`pic-fill`, `pic-stroke`, `pic-stroke-dashed`, `pic-dot`) under another
 * one: a layer id, or `'labels'` for the first `symbol` layer of the map's
 * style (so the basemap's labels stay above the shapes); resolved each time
 * the layers are added, on top when there is no such layer. The drawing's
 * own labels (`pic-text`, `pic-title`) are always appended on top, so a
 * basemap label never covers them. Waits for the map's
 * `load` when the style is not loaded yet; calls made meanwhile are applied
 * then.
 * @returns {{ setGeoJSON(fc, layers?): void, fitTo(fc?): void, detach(): void }}
 */
export function attach(map, fc = EMPTY, opts = {}) {
  let current = fc ?? EMPTY;
  let currentLayers = opts.layers ?? null;
  let pendingFit = opts.fit ? true : null; // true = frame `current`, or a collection
  let ready = false;
  let detached = false;

  // the only properties that depend on the collection (its registration and
  // largest font): everything else in a spec is constant across collections
  const SIZE_PAINT = ['line-width', 'circle-radius'];
  const SIZE_LAYOUT = ['text-size'];
  function applyLayers(specs) {
    let below; // resolved once per pass, only when a layer is added
    for (const spec of specs) {
      if (!map.getLayer(spec.id)) {
        if (TEXT_LAYER_IDS.includes(spec.id)) {
          map.addLayer(spec); // the drawing's labels: on top, never buried
          continue;
        }
        if (below === undefined) below = { id: beforeId(map, opts.before) };
        map.addLayer(spec, below.id);
        continue;
      }
      for (const k of SIZE_PAINT) if (spec.paint && k in spec.paint) map.setPaintProperty(spec.id, k, spec.paint[k]);
      for (const k of SIZE_LAYOUT) if (spec.layout && k in spec.layout) map.setLayoutProperty(spec.id, k, spec.layout[k]);
    }
  }

  function fitNow(target) {
    const b = bounds(target);
    if (b) map.fitBounds(b, { padding: 48, maxZoom: 19, duration: 0 });
  }

  function apply() {
    const src = map.getSource(SOURCE);
    if (src) src.setData(current);
    else map.addSource(SOURCE, { type: 'geojson', data: current });
    applyLayers(currentLayers ?? layers(current));
  }

  function start() {
    if (detached || ready) return;
    ready = true;
    apply();
    if (pendingFit) {
      const target = pendingFit === true ? current : pendingFit;
      pendingFit = null;
      fitNow(target);
    }
  }

  // `isStyleLoaded()` is also false while any tile is still loading, and
  // `load` fires only once per map — so on a map that is already up, `idle`
  // (which needs the style loaded) is what brings us in. `start` runs once.
  const loaded = typeof map.isStyleLoaded === 'function' ? map.isStyleLoaded() : map.loaded?.();
  if (loaded) start();
  else {
    map.once('load', start);
    map.once('idle', start);
  }

  return {
    setGeoJSON(next, nextLayers) {
      current = next ?? EMPTY;
      currentLayers = nextLayers ?? null;
      if (ready && !detached) apply();
    },
    fitTo(target) {
      if (ready) fitNow(target ?? current);
      else pendingFit = target ?? true; // framed once the style has loaded
    },
    detach() {
      if (detached) return;
      detached = true;
      if (!ready) {
        map.off?.('load', start);
        map.off?.('idle', start);
        return;
      }
      for (const id of [...LAYER_IDS].reverse()) if (map.getLayer(id)) map.removeLayer(id);
      if (map.getSource(SOURCE)) map.removeSource(SOURCE);
    },
  };
}

/** The OpenFreeMap `positron` basemap (no key; the rpic.dev default). */
export const DEFAULT_STYLE = 'https://tiles.openfreemap.org/styles/positron';

/**
 * A MapLibre map ready for a pic drawing: `maplibregl` is your maplibre-gl
 * namespace (`import * as maplibregl from 'maplibre-gl'`; set its worker URL
 * yourself if your bundler needs it). Options: `style` (default the
 * OpenFreeMap positron basemap), `cooperativeGestures` (default true: the
 * page scrolls past the map), `navigation` (zoom buttons, default true),
 * `before` (as in `attach`: `'labels'` keeps the basemap's labels on top).
 * Returns the `attach` handle plus `map` and `destroy()`.
 */
export function createGeoMap(maplibregl, container, opts = {}) {
  const map = new maplibregl.Map({
    container,
    style: opts.style ?? DEFAULT_STYLE,
    attributionControl: { compact: true },
    cooperativeGestures: opts.cooperativeGestures ?? true,
  });
  if (opts.navigation ?? true) map.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'top-right');
  const handle = attach(map, EMPTY, { before: opts.before });
  return {
    map,
    setGeoJSON: handle.setGeoJSON,
    fitTo: handle.fitTo,
    detach: handle.detach,
    destroy() {
      handle.detach();
      map.remove();
    },
  };
}
