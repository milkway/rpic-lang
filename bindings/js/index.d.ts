// Type definitions for rpic

export interface Anim {
  id: string;
  effect: string;
  /** absolute start time in seconds */
  start: number;
  duration: number;
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

export interface Bundle {
  svg: string;
  animations: Anim[];
  /** lines emitted by pic `print` statements */
  diagnostics: string[];
  /** non-fatal compiler warnings for accepted but suspicious input */
  warnings: Diagnostic[];
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

/** Compile pic source into `{ svg, animations, diagnostics, warnings }`. Throws on a pic error. */
export function compile(src: string, opts?: CompileOptions): Bundle;

/** Compile and return only the SVG string. */
export function renderSvg(src: string, opts?: CompileOptions): string;

/**
 * Build and play a GSAP timeline from an animation manifest on the SVG inside
 * `root`. Browser-only.
 */
export function animate(root: Element, animations: Anim[], gsap: unknown): unknown;
