// Type definitions for rpic
import type { LayerOptions, MapLibreLayer } from './maplibre';

// The player (animate/interactive + Anim/Interaction) is typed in player.d.ts
// — its module has no wasm dependency and is importable on its own via the
// `./player` subpath. Re-exported here for the one-stop API.
import type { Anim, Interaction } from './player';
export type { Anim, Interaction } from './player';
export { animate, interactive } from './player';

export interface Diagnostic {
  message: string;
  line: number | null;
  col: number | null;
  end_col: number | null;
  /**
   * Which source the position refers to: `null` is your own input; a string
   * names a `copy` include (as written) or a loaded library (`"circuits"`).
   * Positions are always relative to that source — with `circuits: true` an
   * error on your line 1 reports line 1.
   */
  file: string | null;
  kind: string;
  found: string | null;
  expected: string | null;
  hint: string | null;
}

export interface ObjectGeometry {
  /** matches the shape's `<g id="sN">` group in the SVG */
  id: string;
  /** "box" | "circle" | "ellipse" | "path" | "spline" | "arc" | "brace" | "text" */
  kind: string;
  /** bounds in SVG user units (the viewBox space); `null` for invisible shapes */
  bbox: { x: number; y: number; w: number; h: number } | null;
  /** hyperlink URL from the `link` extension. Present only when set. */
  link?: string;
  /** 1-based source position of the statement that drew it, when known */
  line?: number;
  col?: number;
  /** exclusive end column of the statement's leading token */
  end_col?: number;
  /** absent = your own input; a `copy` include name or `"circuits"` otherwise */
  file?: string;
}

/** One subpath of a scene item: a cubic Bézier chain or a polyline. */
export interface ScenePath {
  closed?: true;
  /** cubic form: start point + `[c1, c2, end]` triples */
  start?: [number, number];
  cubics?: [[number, number], [number, number], [number, number]][];
  /** polyline form (inherently straight, or `flatten`ed) */
  points?: [number, number][];
}

export interface SceneItem {
  /** `"s<N>"` — matches the SVG's `<g id>` — or `"blk<N>"` for blocks */
  id: string;
  kind:
    | 'box' | 'circle' | 'ellipse' | 'arc' | 'line' | 'arrow'
    | 'spline' | 'text' | 'dot' | 'brace' | 'block';
  /** draw-order index (dense over shape items; absent on blocks) */
  z?: number;
  label?: string;
  /** enclosing block item ids, outermost first */
  block?: string[];
  class?: string;
  link?: string;
  /** the item paints nothing (`move`, invisible helpers) */
  hidden?: true;
  at?: [number, number];
  w?: number;
  h?: number;
  r?: number;
  rad?: number;
  points?: [number, number][];
  closed?: true;
  paths?: ScenePath[];
  fillPaths?: ScenePath[];
  fillPoints?: [number, number][];
  heads?: {
    at: 'start' | 'end';
    filled: boolean;
    points?: [number, number][];
    path?: ScenePath;
  }[];
  stroke?: {
    color: string;
    /** points */
    width: number;
    dash?: { kind: 'dashed' | 'dotted'; len?: number; gap?: number };
  };
  fill?: {
    color?: string;
    gradient?: { angle: number; stops: { offset: number; color: string }[] };
    hatch?: { angle: number; spacing: number; width: number; color: string; cross: boolean };
  };
  opacity?: number;
  text?: {
    text: string;
    /** the line's baseline anchor point (see docs/scene.md §5) */
    at: [number, number];
    anchor: 'c' | 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';
    font: { size: number; family?: string; bold?: true; italic?: true };
    rotation?: number;
    math?: { width: number; height: number; depth: number };
    outline?: string;
    /** 0-based row of this line in the author's stack, blank rows included (docs/scene.md §5) */
    row?: number;
  }[];
  /** present with `text`: the centre the lines are laid out around (docs/scene.md §5) */
  text_center?: [number, number];
  /** present with `text`: the stack's height in rows, blank ones included (docs/scene.md §5) */
  text_rows?: number;
  /** block items: the child item ids */
  items?: string[];
  /**
   * block items: where the block came from — the outermost macro call that
   * expanded to it, or its opening `[` when written inline
   */
  span?: SourceSpan;
  /** block items made by a macro: that outermost call */
  call?: MacroCall;
}

/**
 * A macro call (scene block items). `args` is rpic's normalized text;
 * `raw_args` the same arguments exactly as written, index-aligned.
 */
export interface MacroCall {
  name: string;
  args: string[];
  raw_args: RawArg[];
}

/**
 * One macro-call argument as written: the source text from its first token
 * to its last (spacing, `\` continuations, quotes kept), from
 * `line`/`col` to `end_line`/`end_col` (exclusive), relative to its own
 * source unit (`file` absent = the user's input, like diagnostics).
 */
export interface RawArg {
  text: string;
  line: number;
  col: number;
  end_line: number;
  end_col: number;
  file?: string;
}

/** A source position, relative to its own source unit (like diagnostics). */
export interface SourceSpan {
  line: number;
  col: number;
  end_col: number;
  /** absent = the user's input; `"circuits"` = the `-c` library; else the `copy` name */
  file?: string;
}

/** A named place (`Label: position`) in the scene. */
export interface ScenePlace {
  /** the resolved label, subscripts included (`R[2]`) */
  label: string;
  /** scene units, the same frame as the items */
  at: [number, number];
  /** enclosing block item ids, outermost first */
  block?: string[];
  span?: SourceSpan;
}

/** A named place in the compile bundle: SVG user units, the `objects` frame. */
export interface Place {
  label: string;
  x: number;
  y: number;
  line?: number;
  col?: number;
  end_col?: number;
  file?: string;
}

/** A troff request line (`.` in column 1) set aside before lexing. */
export interface Passthrough {
  /**
   * the line as written, leading `.` included; a request continued by
   * column-1 `+` lines holds all of them, joined by `\n`
   */
  text: string;
  line: number;
  col: number;
  /** end of the entry's last line */
  end_col: number;
  /** the entry's last line; present only when `+` continuation lines follow it */
  end_line?: number;
  file?: string;
}

/**
 * The device-independent resolved draw list (docs/scene.md). Coordinates in
 * inches, y-up; curves as cubic Béziers; style as descriptors.
 */
export interface Scene {
  /** format version (currently 1) */
  scene: number;
  units: 'in';
  y_up: true;
  canvas: {
    w: number;
    h: number;
    margin: { top: number; right: number; bottom: number; left: number };
    /** where the pic origin sits, from the canvas lower-left corner */
    origin: [number, number];
  };
  source?: string;
  /**
   * the uniform factor the whole-picture rescale (`.PS <w>`, `maxpswid`/`maxpsht`)
   * applied to the geometry; 1 when none acted. `vars` are pre-rescale.
   */
  rescale: number;
  /** final values of every variable (user + pic built-ins) */
  vars: Record<string, number>;
  /** named places (`Label: position`), evaluation order; empty when none */
  places: ScenePlace[];
  items: SceneItem[];
}

export interface Bundle {
  svg: string;
  animations: Anim[];
  /** `draggable` targets for the host to wire GSAP Draggable. Present only when non-empty. */
  interactions?: Interaction[];
  /** lines emitted by pic `print` statements */
  diagnostics: string[];
  /** non-fatal compiler warnings for accepted but suspicious input */
  warnings: Diagnostic[];
  /** per-object geometry, index-aligned with the `<g id="sN">` groups */
  objects: ObjectGeometry[];
  /** named places (`Label: position`) in SVG user units. Present only when the source defines some. */
  places?: Place[];
  /** troff request lines set aside before lexing. Present only when the source has some. */
  passthrough?: Passthrough[];
  /**
   * `true` when the source used `animate scroll`, hinting the host to scrub
   * the timeline on scroll rather than autoplay. Present only when set.
   */
  scroll?: boolean;
  /** the resolved draw list; present only when compiled with `scene: true` */
  scene?: Scene;
}

export interface CompileError extends Error {
  /** structured compiler error, when available */
  errorInfo?: Diagnostic;
  /** warnings emitted before the fatal error, currently usually empty */
  warnings?: Diagnostic[];
}

export interface CompileOptions {
  /** prepend the native circuit-element library (resistor, and_gate, …) */
  circuits?: boolean;
  /**
   * set `texlabels = 1`, typesetting fully `$…$`-delimited labels as TeX
   * math. Requires the math-enabled build (`ready(…, { math: true })`); the
   * default lean build ships without the math renderer (size budget), so
   * labels fall back to literal text plus a diagnostic.
   */
  texlabels?: boolean;
  /** also compute the device-independent `scene` draw list (docs/scene.md) */
  scene?: boolean;
  /** scene: deliver curves as polylines within this tolerance (inches); implies `scene` */
  flatten?: number;
  /** scene: embed typeset math as SVG glyph fragments (`outline` keys) */
  outline?: boolean;
  /**
   * Named source units compiled before `src` (after `circuits`/`texlabels`,
   * in order): positions inside one carry `file: name`; the caller's own
   * positions never shift. Names must be non-empty, unique, free of `"` and
   * line breaks, and not `circuits`/`<texlabels>`/`features` (else an
   * `invalid_options` error).
   */
  preludes?: Prelude[];
}

/** A named source unit for `CompileOptions.preludes`. */
export interface Prelude {
  name: string;
  text: string;
}

export interface ReadyOptions {
  /**
   * Load the math-enabled wasm build (`pkg/rpic_wasm_math_bg.wasm`), which
   * bundles the RaTeX renderer so `texlabels` sources typeset `$…$` labels.
   * The heavier artifact is only fetched when requested; the choice is fixed
   * by the first `ready()` call, and conflicting later calls reject. In Node,
   * pass the math wasm bytes as `wasmInput`.
   */
  math?: boolean;
  /**
   * Load the geo-enabled wasm build (`pkg/rpic_wasm_geo_bg.wasm`), which adds
   * `compileGeoJSON` / `geoToCanvas` (geopic: drawings as GeoJSON). Fetched
   * only when requested; cannot be combined with `math`. In Node, pass the
   * geo wasm bytes as `wasmInput`.
   */
  geo?: boolean;
}

/**
 * Initialize the WebAssembly module. In the browser call with no argument; in
 * Node pass the `.wasm` bytes or a file URL. Idempotent while pending or
 * after success; a *failed* init clears the slot so `ready()` can be retried.
 */
export function ready(
  wasmInput?: BufferSource | URL | string,
  opts?: ReadyOptions
): Promise<void>;

/** Compile pic source into `{ svg, animations, diagnostics, warnings, objects }`. Throws on a pic error. */
export function compile(src: string, opts?: CompileOptions): Bundle;

/** Compile and return only the SVG string. */
export function renderSvg(src: string, opts?: CompileOptions): string;

// ---------------------------------------------------------------------------
// geopic: pic drawings as GeoJSON (docs/geopic.md). Needs the geo build:
// `await ready(undefined, { geo: true })`.
// ---------------------------------------------------------------------------

/** `[west, south, east, north]`, degrees. */
export type GeoBBox = [number, number, number, number];

/**
 * A named geographic feature the host knows about. The name must be a pic
 * label (`/^[A-Z][A-Za-z0-9_]*$/`); anything else is rejected before
 * compiling. With a `bbox` it is injected as an invisible box, so
 * `Name.n`/`Name.e`/… follow its extent (compass corners are canvas
 * directions).
 */
export interface GeoFeatureInput {
  name: string;
  lon: number;
  lat: number;
  bbox?: GeoBBox | null;
}

/**
 * `features` in any of its accepted forms: `{ Name: [lon, lat] }`,
 * `{ Name: [lon, lat, bbox] }`, `{ Name: { lon, lat, bbox? } }`, or an array
 * of `{ name, lon, lat, bbox? }` objects / `[name, lon, lat, bbox?]` tuples.
 */
export type GeoFeatures =
  | Record<string, [number, number] | [number, number, GeoBBox | null] | { lon: number; lat: number; bbox?: GeoBBox | null }>
  | Array<GeoFeatureInput | [string, number, number] | [string, number, number, GeoBBox | null]>;

export interface GeoOptions {
  /** load the native circuit-element library */
  circuits?: boolean;
  /** set `texlabels = 1` (the geo build has no math renderer: literal labels + a diagnostic) */
  texlabels?: boolean;
  /** named source units compiled before the source, as in `CompileOptions.preludes` */
  preludes?: Prelude[];
  /** projection frame; overrides `geo_frame` */
  frame?: 'enu' | 'mercator';
  /** `[lon, lat]` of the pic origin; overrides `geo_lon`/`geo_lat` */
  anchor?: [number, number];
  /** metres per authored inch; overrides `geo_scale` (default 100) */
  scale?: number;
  /** azimuth of the canvas +y axis, degrees clockwise from north; overrides `geo_bearing` */
  bearing?: number;
  /**
   * Named features: those the source defines (places or labelled objects)
   * are control points of its geometry; the others are injected as places
   * (two-pass), so `arrow from Recife to Olinda` works without the source
   * defining them.
   */
  features?: GeoFeatures;
  /** flattening tolerance for curves, inches (default `1e-3`) */
  flatten?: number;
  /** pixels per inch when the source sets `geo_px = 1` */
  viewScale?: number;
  /** `class` words marking screen-anchored items (default `['screen']`) */
  screenClasses?: string[];
  /**
   * add `layers`: the MapLibre layers that draw the collection (docs/geopic.md § 6);
   * `{ textScale }` multiplies every label's font size (in (0, 16], default 1)
   */
  layers?: boolean | LayerOptions;
}

/** A non-fatal loss or substitution while georeferencing (never silent). */
export interface GeoWarning {
  /**
   * `unsupported_style`, `ignored`, `registration_residual`,
   * `ignored_with_control_points`, `control_point_unresolved`,
   * `control_point_overridden`, `unsupported_kind`, `densification_budget`,
   * `scene_inconsistent`,
   * …; engine `scene_unsupported` diagnostics ride here too (with the
   * `Diagnostic` keys instead of `id`/`span`).
   */
  kind: string;
  message: string;
  /** the scene item concerned (`s<N>`), when there is one */
  id?: string;
  /**
   * its source span, relative to your own source; `file: 'features'` marks a
   * position inside the lines `compileGeoJSON` injected for `features`
   */
  span?: { line: number; col: number; end_col: number; file?: string };
}

/** The similarity canvas (inches) → Earth. */
export interface GeoRegistration {
  frame: 'enu' | 'mercator';
  /** geographic position of `origin` */
  anchor: { lon: number; lat: number };
  /** the canvas point (inches) that sits at `anchor` */
  origin: [number, number];
  /** metres per inch of the emitted geometry */
  scale_m_per_in: number;
  /** azimuth of the canvas +y axis, degrees clockwise from north */
  bearing_deg: number;
  method: 'anchor' | 'two_points' | 'helmert';
  /** control points used by the fit (0 for a declared anchor) */
  control_points: number;
  /** RMS residual of the fit, metres (fits only) */
  rms_m?: number;
}

export interface GeoJSONGeometry {
  type: 'Point' | 'LineString' | 'Polygon' | 'MultiPoint' | 'MultiLineString' | 'MultiPolygon';
  coordinates: unknown;
}

export interface GeoJSONFeature {
  type: 'Feature';
  geometry: GeoJSONGeometry | null;
  /** simplestyle keys (`stroke`, `fill`, …) and `geopic:*` (docs/geopic.md) */
  properties: Record<string, unknown>;
}

export interface GeoJSONFeatureCollection {
  type: 'FeatureCollection';
  features: GeoJSONFeature[];
}

export interface GeoResult {
  /** RFC 7946 FeatureCollection, WGS84 */
  geojson: GeoJSONFeatureCollection;
  /** engine warnings, then scene, then geopic ones — never silent */
  warnings: (GeoWarning | Diagnostic)[];
  /** lines emitted by pic `print` statements, plus engine notices (as in `Bundle`) */
  diagnostics: string[];
  registration: GeoRegistration;
  /** what a host needs to write canvas positions back as source coordinates */
  canvas: {
    /** the whole-picture rescale (`.PS <width>`, `maxpswid`); 1 when none */
    rescale: number;
    /** the pic `scale` variable (source units per inch) */
    scale: number;
  };
  /**
   * With `layers: true` only: MapLibre style-spec layers (source `"pic"`),
   * sizes true to the ground — pass them to `attach` from
   * `@strategicprojects/rpic/maplibre`.
   */
  layers?: MapLibreLayer[];
}

/** A georeferencing error; `info.kind` is machine-readable. */
export class GeoError extends Error {
  constructor(message: string, info: { kind: string; message: string; hint?: string });
  info: {
    /**
     * `no_registration`, `invalid_scale`, `invalid_anchor`, `invalid_frame`,
     * `invalid_scene`, `invalid_flatten`, `view_scale_required`,
     * `degenerate_control_points`, `invalid_options` (option JSON of the
     * wrong shape).
     */
    kind: string;
    message: string;
    hint?: string;
  };
}

/**
 * Georeference pic source as a GeoJSON FeatureCollection (WGS84). Needs the
 * geo build (`ready(…, { geo: true })`). Throws a `CompileError` on a pic
 * error, a `GeoError` on a georeferencing error, and a plain `Error` for an
 * invalid feature name/shape (checked before anything compiles).
 */
export function compileGeoJSON(src: string, opts?: GeoOptions): GeoResult;

/**
 * Canvas positions (inches) of WGS84 `[lon, lat]` points under a
 * registration (as `compileGeoJSON` returned it). Needs the geo build.
 */
export function geoToCanvas(
  registration: GeoRegistration | string,
  points: [number, number][]
): [number, number][];

export type { LayerOptions, MapLibreLayer };

/**
 * The engine's MapLibre layers for a geopic FeatureCollection (or its JSON
 * text). Needs the geo build.
 */
export function maplibreLayers(fc: GeoJSONFeatureCollection | string, opts?: LayerOptions): MapLibreLayer[];
