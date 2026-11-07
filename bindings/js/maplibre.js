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

/** pic compass anchor → MapLibre text-anchor (the reference point sits at
 *  that side of the text: `w` = text extends right of it, `n` = below). */
const ANCHORS = {
  c: 'center', n: 'top', s: 'bottom', e: 'right', w: 'left',
  ne: 'top-right', nw: 'top-left', se: 'bottom-right', sw: 'bottom-left',
};

/**
 * The numbers the size expressions are built from: metres per inch and the
 * latitude from the collection's `geopic.registration` (cos φ drops out under
 * `mercator`), pixels per ground metre at zoom 0 (MapLibre's 512 px tiles:
 * 1 / (78271.517 · cos φ)), and the upper zoom stop of `text-size` — the last
 * zoom at which the largest font still fits in MapLibre's 255 px symbol limit.
 */
export function sizing(fc) {
  const reg = fc?.geopic?.registration;
  const mPerIn = reg?.scale_m_per_in ?? 100;
  const lat = reg?.frame === 'mercator' ? 0 : (reg?.anchor?.lat ?? 0);
  const pxPerMetreAtZ0 = 1 / ((78271.517 * Math.cos((lat * Math.PI) / 180)) || 1);
  let maxFontM = 1e-6; // a loop, not a spread: collections can hold 10^5+ features
  for (const f of fc?.features ?? []) {
    maxFontM = Math.max(maxFontM, (Number(f.properties?.['geopic:font_size']) || 0) * (mPerIn / 72));
  }
  const z = Math.floor(Math.log2(255 / (maxFontM * pxPerMetreAtZ0)));
  const textZHi = Number.isNaN(z) ? 1 : Math.min(24, Math.max(1, z));
  return { mPerIn, lat, pxPerMetreAtZ0, textZHi };
}

/**
 * The six MapLibre style-spec layers (source `"pic"`) that draw `fc` with
 * sizes true to the ground, in draw order: pic-fill, pic-stroke,
 * pic-stroke-dashed, pic-dot, pic-text, pic-title. Same output as the
 * engine's `geopic_core::maplibre::layers`.
 */
export function layers(fc) {
  const { mPerIn, pxPerMetreAtZ0, textZHi } = sizing(fc);
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
  const inToM = (expr) => ['*', expr, mPerIn];
  // pic strokes are hairlines: ground-true width, never thinner than 1 px
  const lineWidth = () => groundPx(ptToM(['coalesce', ['get', 'stroke-width'], 0.8]), 1);
  const dotRadius = groundPx(inToM(['coalesce', ['get', 'geopic:radius_in'], 0.04]), 2);
  const textSize = () => groundTextPx(ptToM(['coalesce', ['get', 'geopic:font_size'], 11]));

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
 * Draw `fc` on a MapLibre `map`: adds (or updates) the GeoJSON source `"pic"`
 * and the six layers. `layers` are the engine's (`compileGeoJSON(src,
 * { layers: true }).layers`); omitted, they are computed by `layers(fc)`.
 * `fit: true` frames the drawing. Waits for the map's `load` when the style is
 * not loaded yet; calls made meanwhile are applied then.
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
    for (const spec of specs) {
      if (!map.getLayer(spec.id)) {
        map.addLayer(spec);
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
 * page scrolls past the map), `navigation` (zoom buttons, default true).
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
  const handle = attach(map, EMPTY);
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
