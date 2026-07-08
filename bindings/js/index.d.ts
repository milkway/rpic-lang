// Type definitions for rpic

export interface Anim {
  id: string;
  /** "fade" | "pop" | "draw" | "slide" | "move" | "highlight" | "morph" */
  effect: string;
  /** absolute start time in seconds */
  start: number;
  duration: number;
  /** replay count; -1 loops forever. Present only when set. */
  repeat?: number;
  /** reverse on each repeat. Present only when set. */
  yoyo?: boolean;
  /** GSAP easing name overriding the effect's default. Present only when set. */
  ease?: string;
  /** id of the object whose path a `move` follows. Present only for `move`. */
  path?: string;
  /** target colour for `highlight` (`#rrggbb` or a name). Present only when set. */
  color?: string;
  /** play the effect as an exit (reverse) instead of an entrance. */
  out?: boolean;
  /** entry direction for `slide` ("up" | "down" | "left" | "right"). */
  from?: string;
  /** id of the shape a `morph` tweens into. Present only for `morph`. */
  morph?: string;
}

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
  /** 1-based source position of the statement that drew it, when known */
  line?: number;
  col?: number;
  /** exclusive end column of the statement's leading token */
  end_col?: number;
  /** absent = your own input; a `copy` include name or `"circuits"` otherwise */
  file?: string;
}

export interface Bundle {
  svg: string;
  animations: Anim[];
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

/**
 * Build and play a GSAP timeline from an animation manifest on the SVG inside
 * `root`. Browser-only.
 */
export function animate(root: Element, animations: Anim[], gsap: unknown): unknown;
