// A pic drawing on a real map, client side: MapLibre GL JS with the
// OpenFreeMap `positron` basemap. The map itself — source `pic`, the six
// layers, every size (strokes, dots, fonts) true to the ground from the
// collection's `geopic.registration` — comes from the package module
// `@strategicprojects/rpic/maplibre` (docs/geopic.md § 6, DECISIONS G9),
// whose layers are emitted by the engine: build-time maps (GeoMap.astro) pass
// `rpic --geojson --maplibre`'s layers, the live playground
// (GeoPlayground.astro) `compileGeoJSON(src, { layers: true }).layers`.
// What stays here is the site's own glue: the worker URL, the CSS, popups
// and the hover cursor, and the `window.__rpicGeoMaps` registry for QA.
// (`geopic:placement = "screen"` is a hint for consumers that implement
// pixel-anchored callouts; this module does not, on purpose: a fixed-size
// marker made the arrow look detached.)
import { Map as MapLibreMap, NavigationControl, Popup, setWorkerUrl } from 'maplibre-gl';
import type { MapLayerMouseEvent } from 'maplibre-gl';
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url';
import 'maplibre-gl/dist/maplibre-gl.css';
import { createGeoMap as createPicMap } from '@strategicprojects/rpic/maplibre';
import type { GeoPicCollection, MapLibreLayer } from '@strategicprojects/rpic/maplibre';

// the bundler cannot inline MapLibre's worker; serve the file and point at it
setWorkerUrl(workerUrl);

export type GeoFC = GeoPicCollection;
export type { MapLibreLayer };

export interface GeoMapHandle {
  /** replace the drawing; `layers` are the engine's (default: the package's port) */
  setGeoJSON(fc: GeoFC, layers?: MapLibreLayer[]): void;
  /** frame the drawing's extent */
  fitTo(fc: GeoFC): void;
  destroy(): void;
}

/** Layers whose shapes answer clicks with a popup. */
const CLICKABLE = ['pic-fill', 'pic-stroke', 'pic-stroke-dashed', 'pic-dot'];

export function createGeoMap(container: HTMLElement): GeoMapHandle {
  const pic = createPicMap<MapLibreMap>({ Map: MapLibreMap, NavigationControl }, container);
  const map = pic.map;
  // expose the maps for debugging / QA scripts
  const registry = ((window as unknown as { __rpicGeoMaps?: unknown[] }).__rpicGeoMaps ??= []);
  registry.push(map);

  // registered after the package's own `load` handler, so the layers exist
  map.once('load', () => {
    // the properties come from user-typed source in the playground: escape
    const esc = (v: unknown) =>
      String(v ?? '').replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
    map.on('click', CLICKABLE, (ev: MapLayerMouseEvent) => {
      const f = ev.features?.[0];
      if (!f) return;
      const p = f.properties as Record<string, unknown>;
      const title = p['title'] ? `<b>${esc(p['title'])}</b><br>` : '';
      new Popup({ closeButton: false })
        .setLngLat(ev.lngLat)
        .setHTML(`${title}<code>${esc(p['geopic:kind'])}</code> · <code>${esc(p['geopic:id'])}</code>`)
        .addTo(map);
    });
    for (const id of CLICKABLE) {
      map.on('mouseenter', id, () => (map.getCanvas().style.cursor = 'pointer'));
      map.on('mouseleave', id, () => (map.getCanvas().style.cursor = ''));
    }
  });

  return {
    setGeoJSON: (fc, layers) => pic.setGeoJSON(fc, layers),
    fitTo: (fc) => pic.fitTo(fc),
    destroy() {
      const i = registry.indexOf(map);
      if (i >= 0) registry.splice(i, 1);
      pic.destroy();
    },
  };
}
