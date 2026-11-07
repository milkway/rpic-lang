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
  /** upper zoom stop of `text-size` (1..24) */
  textZHi: number;
}

/** The numbers behind the ground-true size expressions. */
export function sizing(fc: GeoPicCollection): Sizing;
/** The six layers that draw `fc` — a port of the engine's output. */
export function layers(fc: GeoPicCollection): MapLibreLayer[];
/** `[[west, south], [east, north]]`, or null for an empty collection. */
export function bounds(fc: GeoPicCollection): [[number, number], [number, number]] | null;

/** The minimal map surface `attach` uses (a maplibre-gl `Map` satisfies it). */
export interface MapLike {
  addSource(id: string, source: unknown): unknown;
  getSource(id: string): { setData(data: unknown): unknown } | undefined | null;
  removeSource(id: string): unknown;
  addLayer(layer: unknown): unknown;
  getLayer(id: string): unknown;
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
