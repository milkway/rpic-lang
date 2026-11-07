// Type definitions for the rpic MapLibre module (maplibre.js — zero imports,
// no wasm, no maplibre-gl dependency). See docs/geopic.md § 6.

/** A MapLibre style-spec layer object (kept structural: no maplibre-gl types). */
export interface MapLibreLayer {
  id: string;
  type: 'fill' | 'line' | 'circle' | 'symbol';
  source: 'pic';
  filter?: unknown;
  layout?: Record<string, unknown>;
  paint?: Record<string, unknown>;
}

/** A geopic FeatureCollection (only what this module reads). */
export interface GeoPicCollection {
  type: string;
  features: { geometry: { type: string; coordinates: unknown } | null; properties: Record<string, unknown> }[];
  geopic?: { registration?: { frame?: string; scale_m_per_in?: number; anchor?: { lat?: number } } };
}

/** The GeoJSON source id every layer reads: `"pic"`. */
export const SOURCE: 'pic';
/** `pic-fill`, `pic-stroke`, `pic-stroke-dashed`, `pic-dot`, `pic-text`, `pic-title`. */
export const LAYER_IDS: readonly string[];
/** The OpenFreeMap positron basemap URL. */
export const DEFAULT_STYLE: string;

export interface Sizing {
  /** `registration.scale_m_per_in` (100 when absent) */
  mPerIn: number;
  /** the anchor latitude under `enu`, 0 under `mercator` */
  lat: number;
  /** 1 / (78271.517 · cos lat) */
  pxPerMetreAtZ0: number;
  /** upper zoom stop of `text-size` (1..24), from the scaled fonts */
  textZHi: number;
  /** the `textScale` the sizes were built with (default 1) */
  textScale: number;
}

/** Options of the layers (the engine's `LayerOptions`, DECISIONS G11). */
export interface LayerOptions {
  /**
   * multiplies every label's font size; finite, in (0, 16]; default 1 (true
   * to the ground). The upper `text-size` zoom stop follows the scaled font.
   */
  textScale?: number;
}

/** The largest accepted `textScale`: 16. */
export const MAX_TEXT_SCALE: number;

/**
 * Validate layer options as the engine does (object, no unknown keys,
 * `textScale` finite in (0, 16]); returns them with the default filled.
 * Throws an `Error` otherwise.
 */
export function checkLayerOptions(opts?: LayerOptions | null): Required<LayerOptions>;

/** The numbers behind the ground-true size expressions (throws on a bad `textScale`). */
export function sizing(fc: GeoPicCollection, opts?: LayerOptions): Sizing;
/** The six layers that draw `fc` — a port of the engine's output (throws on a bad `textScale`). */
export function layers(fc: GeoPicCollection, opts?: LayerOptions): MapLibreLayer[];
/** `[[west, south], [east, north]]`, or null for an empty collection. */
export function bounds(fc: GeoPicCollection): [[number, number], [number, number]] | null;

/** The minimal map surface `attach` uses (a maplibre-gl `Map` satisfies it). */
export interface MapLike {
  addSource(id: string, source: unknown): unknown;
  getSource(id: string): { setData(data: unknown): unknown } | undefined | null;
  removeSource(id: string): unknown;
  addLayer(layer: unknown, beforeId?: string): unknown;
  getLayer(id: string): unknown;
  /** read for `before: 'labels'` (the start of the trailing `symbol` block) and `font: 'style'` */
  getStyle?(): { layers?: { id: string; type: string; layout?: Record<string, unknown> }[] } | undefined | null;
  removeLayer(id: string): unknown;
  setFilter(id: string, filter: unknown): unknown;
  setPaintProperty(id: string, name: string, value: unknown): unknown;
  setLayoutProperty(id: string, name: string, value: unknown): unknown;
  fitBounds(bounds: [[number, number], [number, number]], opts?: unknown): unknown;
  isStyleLoaded?(): boolean | void;
  loaded?(): boolean;
  once(type: 'load', listener: () => void): unknown;
  off?(type: 'load', listener: () => void): unknown;
}

export interface AttachOptions {
  /** the engine's layers (`compileGeoJSON(src, { layers: true }).layers`); default `layers(fc)` */
  layers?: MapLibreLayer[] | unknown[];
  /** frame the drawing once drawn */
  fit?: boolean;
  /**
   * add the geometry layers (`pic-fill`, `pic-stroke`, `pic-stroke-dashed`,
   * `pic-dot`) under this one: a layer id, or `'labels'` = the style's layer
   * with id `labels` if it has one, else the start of its trailing block of
   * `symbol` layers — the first layer after the last non-`symbol` one (the
   * basemap's labels stay above the shapes, its roads and buildings below).
   * Resolved when the layers are added; on top when there is none.
   * The drawing's own labels (`pic-text`, `pic-title`) always go on top.
   * A placement on the consumer's map, not part of the layer specs.
   */
  before?: 'labels' | (string & {});
  /**
   * the `text-font` of the drawing's labels (`pic-text`, `pic-title`):
   * `'style'` (default) = the most common stack among the style's own
   * symbol layers (ties: a Regular/Medium face), the spec's when the style
   * has none — the glyph server serves the style's fonts, and one it serves
   * with other metrics (CARTO's Noto) centres a label on its anchor;
   * `'spec'` = the engine's `["Noto Sans Regular"]` (a placeholder); or an
   * explicit stack. Applied to a copy when the layers are added.
   */
  font?: 'style' | 'spec' | string[];
}

export interface GeoMapAttachment {
  /** replace the drawing; layers default to `layers(fc)` */
  setGeoJSON(fc: GeoPicCollection, layers?: MapLibreLayer[] | unknown[]): void;
  /** frame `fc` (default the current drawing) */
  fitTo(fc?: GeoPicCollection): void;
  /** remove the layers and the source */
  detach(): void;
}

/** Add (or update) source `"pic"` and the layers on `map`. */
export function attach(map: MapLike, fc?: GeoPicCollection, opts?: AttachOptions): GeoMapAttachment;

export interface CreateGeoMapOptions {
  /** basemap style URL or object; default the OpenFreeMap positron */
  style?: unknown;
  /** default true */
  cooperativeGestures?: boolean;
  /** zoom buttons; default true */
  navigation?: boolean;
  /** as in `attach`: a layer id, or `'labels'` to keep the basemap's labels on top */
  before?: 'labels' | (string & {});
  /** as in `attach`: `'style'` (default), `'spec'` or a font stack */
  font?: 'style' | 'spec' | string[];
}

export interface GeoMap<M = unknown> extends GeoMapAttachment {
  map: M;
  /** detach and remove the map */
  destroy(): void;
}

/** A map ready for a pic drawing; `maplibregl` is the maplibre-gl namespace. */
export function createGeoMap<M = unknown>(
  maplibregl: { Map: new (opts: any) => M; NavigationControl: new (opts?: any) => unknown },
  container: HTMLElement | string,
  opts?: CreateGeoMapOptions,
): GeoMap<M>;
