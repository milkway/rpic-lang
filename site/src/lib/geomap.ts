// A pic drawing on a real map, client side: MapLibre GL JS with the
// OpenFreeMap `positron` basemap, styled from a geopic FeatureCollection's
// simplestyle + `geopic:*` properties — every shape as map geometry and every
// size (strokes, dots, fonts) true to the ground via the collection's
// `geopic.registration`, so the map keeps the SVG's proportions at any zoom.
// Shared by the build-time maps (GeoMap.astro) and the live playground
// (GeoPlayground.astro), which feeds it a fresh collection on every edit.
// (`geopic:placement = "screen"` is a hint for consumers that implement
// pixel-anchored callouts; this module does not, on purpose: a fixed-size
// marker made the arrow look detached.)
import { Map as MapLibreMap, NavigationControl, Popup, setWorkerUrl } from 'maplibre-gl';
import type { GeoJSONSource, MapLayerMouseEvent } from 'maplibre-gl';
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url';
import 'maplibre-gl/dist/maplibre-gl.css';

// the bundler cannot inline MapLibre's worker; serve the file and point at it
setWorkerUrl(workerUrl);

type Feature = {
  geometry: { type: string; coordinates: unknown } | null;
  properties: Record<string, unknown>;
};
export type GeoFC = {
  type: string;
  features: Feature[];
  geopic?: { registration?: { frame?: string; scale_m_per_in?: number; anchor?: { lat?: number } } };
};

export interface GeoMapHandle {
  /** replace the drawing (sizes re-derived from its registration) */
  setGeoJSON(fc: GeoFC): void;
  /** frame the drawing's extent */
  fitTo(fc: GeoFC): void;
  destroy(): void;
}

const EMPTY: GeoFC = { type: 'FeatureCollection', features: [] };

/** pic compass anchor → MapLibre text-anchor (the reference point sits at
 *  that side of the text: `w` = text extends right of it, `n` = below). */
const ANCHORS: Record<string, string> = {
  c: 'center', n: 'top', s: 'bottom', e: 'right', w: 'left',
  ne: 'top-right', nw: 'top-left', se: 'bottom-right', sw: 'bottom-left',
};

function bounds(fc: GeoFC): [[number, number], [number, number]] | null {
  let w = Infinity, s = Infinity, e = -Infinity, n = -Infinity;
  const take = (p: unknown) => {
    const [x, y] = p as [number, number];
    w = Math.min(w, x); e = Math.max(e, x); s = Math.min(s, y); n = Math.max(n, y);
  };
  for (const f of fc.features) {
    const g = f.geometry;
    if (!g) continue;
    if (g.type === 'Point') take(g.coordinates);
    else if (g.type === 'LineString') (g.coordinates as unknown[]).forEach(take);
    else if (g.type === 'Polygon') (g.coordinates as unknown[][])[0].forEach(take);
  }
  return Number.isFinite(w) ? [[w, s], [e, n]] : null;
}

/**
 * Sizes true to the ground: the collection's `geopic.registration` gives
 * metres per inch, so a stroke width (points), a dot radius (inches) or a
 * font size (points) becomes metres, and metres become pixels at zoom z as
 * m / (78271.517 · cos(lat) / 2^z) — MapLibre's 512 px tiles put the whole
 * equator on 512 px at zoom 0. MapLibre only lets `zoom` drive an
 * interpolate at the top of an expression; an exponential (base 2)
 * interpolation between two zoom stops reproduces the 2^z law exactly.
 * Under the `mercator` frame the scale is in EPSG:3857 metres, which are
 * already the map's own (inflated by 1/cos φ), so the cos(lat) factor drops
 * out. Everything here depends on the collection, so it is recomputed on
 * every `setGeoJSON`.
 */
function sizing(fc: GeoFC) {
  const reg = fc.geopic?.registration;
  const mPerIn = reg?.scale_m_per_in ?? 100;
  const lat = reg?.frame === 'mercator' ? 0 : (reg?.anchor?.lat ?? 0);
  const pxPerMetreAtZ0 = 1 / ((78271.517 * Math.cos((lat * Math.PI) / 180)) || 1);
  const groundPx = (metres: unknown[], floorPx: number): unknown[] => [
    'interpolate', ['exponential', 2], ['zoom'],
    0, ['max', floorPx, ['*', metres, pxPerMetreAtZ0]],
    24, ['max', floorPx, ['*', metres, pxPerMetreAtZ0 * 2 ** 24]],
  ];
  // Symbol sizes are packed into 16-bit vertex attributes (≈ 255 px max), so
  // a far zoom stop with thousands of pixels overflows and the text came out
  // tiny. For text the upper stop is the last zoom at which this map's
  // largest font still fits in 255 px: exact 2^z scaling up to there, a
  // frozen size beyond (far past the fitted zoom).
  const maxFontM = Math.max(
    1e-6,
    ...fc.features.map((f) => (Number(f.properties['geopic:font_size']) || 0) * (mPerIn / 72)),
  );
  const textZHi = Math.min(24, Math.max(1, Math.floor(Math.log2(255 / (maxFontM * pxPerMetreAtZ0)))));
  const groundTextPx = (metres: unknown[]): unknown[] => [
    'interpolate', ['exponential', 2], ['zoom'],
    0, ['max', 1, ['*', metres, pxPerMetreAtZ0]],
    textZHi, ['min', 255, ['max', 1, ['*', metres, pxPerMetreAtZ0 * 2 ** textZHi]]],
  ];
  const ptToM = (expr: unknown[]): unknown[] => ['*', expr, mPerIn / 72];
  const inToM = (expr: unknown[]): unknown[] => ['*', expr, mPerIn];
  return {
    // pic strokes are hairlines: ground-true width, never thinner than 1 px
    lineWidth: groundPx(ptToM(['coalesce', ['get', 'stroke-width'], 0.8]), 1),
    dotRadius: groundPx(inToM(['coalesce', ['get', 'geopic:radius_in'], 0.04]), 2),
    // the pic font size in points → metres on the ground → px per zoom
    textSize: groundTextPx(ptToM(['coalesce', ['get', 'geopic:font_size'], 11])),
  };
}

const STROKE_LAYERS = ['pic-stroke', 'pic-stroke-dashed'] as const;
const TEXT_LAYERS = ['pic-text', 'pic-title'] as const;

export function createGeoMap(container: HTMLElement): GeoMapHandle {
  const map = new MapLibreMap({
    container,
    style: 'https://tiles.openfreemap.org/styles/positron',
    attributionControl: { compact: true },
    cooperativeGestures: true,
  });
  map.addControl(new NavigationControl({ showCompass: false }), 'top-right');
  // expose the maps for debugging / QA scripts
  const registry = ((window as unknown as { __rpicGeoMaps?: unknown[] }).__rpicGeoMaps ??= []);
  registry.push(map);

  let loaded = false;
  let current: GeoFC = EMPTY;
  let pendingFit: GeoFC | null = null;

  // any: the style-spec expression types are far stricter than the arrays
  // built above, and every expression here is checked by MapLibre at runtime
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const expr = (e: unknown) => e as any;

  function applySizes(fc: GeoFC) {
    const s = sizing(fc);
    for (const id of STROKE_LAYERS) map.setPaintProperty(id, 'line-width', expr(s.lineWidth));
    map.setPaintProperty('pic-dot', 'circle-radius', expr(s.dotRadius));
    for (const id of TEXT_LAYERS) map.setLayoutProperty(id, 'text-size', expr(s.textSize));
  }

  function fitNow(fc: GeoFC) {
    const b = bounds(fc);
    if (b) map.fitBounds(b, { padding: 48, maxZoom: 19, duration: 0 });
  }

  map.on('load', () => {
    const s = sizing(current);
    map.addSource('pic', { type: 'geojson', data: expr(current) });
    const isPolygon = ['==', ['geometry-type'], 'Polygon'];
    const isLine = ['==', ['geometry-type'], 'LineString'];
    const kind = ['get', 'geopic:kind'];
    map.addLayer({
      id: 'pic-fill', type: 'fill', source: 'pic', filter: expr(isPolygon),
      paint: {
        'fill-color': expr(['coalesce', ['get', 'fill'], '#000000']),
        'fill-opacity': expr(['case', ['has', 'fill'], ['coalesce', ['get', 'fill-opacity'], 0.55], 0]),
      },
    });
    for (const id of STROKE_LAYERS) {
      const dashed = id === 'pic-stroke-dashed';
      map.addLayer({
        id, type: 'line', source: 'pic',
        filter: expr(['all', ['any', isPolygon, isLine], ['has', 'stroke'], dashed ? ['has', 'geopic:dash'] : ['!', ['has', 'geopic:dash']]]),
        layout: { 'line-cap': 'round', 'line-join': 'round' },
        paint: {
          'line-color': expr(['get', 'stroke']),
          'line-width': expr(s.lineWidth),
          'line-opacity': expr(['coalesce', ['get', 'stroke-opacity'], 1]),
          ...(dashed ? { 'line-dasharray': [3, 2] } : {}),
        },
      });
    }
    map.addLayer({
      id: 'pic-dot', type: 'circle', source: 'pic', filter: expr(['==', kind, 'dot']),
      paint: {
        'circle-radius': expr(s.dotRadius),
        'circle-color': expr(['coalesce', ['get', 'fill'], '#000000']),
      },
    });
    map.addLayer({
      id: 'pic-text', type: 'symbol', source: 'pic', filter: expr(['==', kind, 'text']),
      layout: {
        'text-field': expr(['get', 'geopic:text']),
        'text-font': ['Noto Sans Regular'],
        'text-size': expr(s.textSize),
        'text-anchor': expr(['match', ['get', 'geopic:anchor'], ...Object.entries(ANCHORS).flat(), 'center']),
        // geopic:rotation is CCW from east; MapLibre rotates clockwise
        'text-rotate': expr(['*', -1, ['coalesce', ['get', 'geopic:rotation'], 0]]),
        'text-rotation-alignment': 'map',
        'text-pitch-alignment': 'map',
        'text-allow-overlap': true,
        'text-ignore-placement': true,
      },
      paint: { 'text-color': '#0f172a', 'text-halo-color': '#ffffff', 'text-halo-width': 1.5 },
    });
    // text attached to a shape (a box's label) sits at the shape's centre
    map.addLayer({
      id: 'pic-title', type: 'symbol', source: 'pic',
      filter: expr(['all', isPolygon, ['has', 'title'], ['!=', kind, 'arrowhead']]),
      layout: {
        'text-field': expr(['get', 'title']),
        'text-font': ['Noto Sans Regular'],
        'text-size': expr(s.textSize),
        'text-rotation-alignment': 'map',
        'text-pitch-alignment': 'map',
        'text-allow-overlap': true,
        'text-ignore-placement': true,
      },
      paint: { 'text-color': '#0f172a', 'text-halo-color': '#ffffff', 'text-halo-width': 1.5 },
    });

    map.on('click', ['pic-fill', 'pic-stroke', 'pic-stroke-dashed', 'pic-dot'], (ev: MapLayerMouseEvent) => {
      const f = ev.features?.[0];
      if (!f) return;
      const p = f.properties as Record<string, unknown>;
      const title = p['title'] ? `<b>${String(p['title']).replace(/</g, '&lt;')}</b><br>` : '';
      new Popup({ closeButton: false })
        .setLngLat(ev.lngLat)
        .setHTML(`${title}<code>${p['geopic:kind']}</code> · <code>${p['geopic:id']}</code>`)
        .addTo(map);
    });
    for (const id of ['pic-fill', 'pic-stroke', 'pic-dot']) {
      map.on('mouseenter', id, () => (map.getCanvas().style.cursor = 'pointer'));
      map.on('mouseleave', id, () => (map.getCanvas().style.cursor = ''));
    }

    loaded = true;
    if (pendingFit) {
      fitNow(pendingFit);
      pendingFit = null;
    }
  });

  return {
    setGeoJSON(fc) {
      current = fc;
      if (!loaded) return; // the load handler picks `current` up
      (map.getSource('pic') as GeoJSONSource).setData(expr(fc));
      applySizes(fc);
    },
    fitTo(fc) {
      if (loaded) fitNow(fc);
      else pendingFit = fc;
    },
    destroy() {
      const i = registry.indexOf(map);
      if (i >= 0) registry.splice(i, 1);
      map.remove();
    },
  };
}
