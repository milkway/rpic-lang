// Type definitions for rpic

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
  }[];
  /** block items: the child item ids */
  items?: string[];
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
  /** final values of every variable (user + pic built-ins) */
  vars: Record<string, number>;
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
