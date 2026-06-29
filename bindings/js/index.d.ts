// Type definitions for @milkway/rpic

export interface Anim {
  id: string;
  effect: string;
  /** absolute start time in seconds */
  start: number;
  duration: number;
}

export interface Bundle {
  svg: string;
  animations: Anim[];
}

export interface CompileOptions {
  /** prepend the native circuit-element library (resistor, and_gate, …) */
  circuits?: boolean;
}

/**
 * Initialize the WebAssembly module. In the browser call with no argument; in
 * Node pass the `.wasm` bytes or a file URL.
 */
export function ready(wasmInput?: BufferSource | URL | string): Promise<void>;

/** Compile pic source into `{ svg, animations }`. Throws on a pic error. */
export function compile(src: string, opts?: CompileOptions): Bundle;

/** Compile and return only the SVG string. */
export function renderSvg(src: string, opts?: CompileOptions): string;

/**
 * Build and play a GSAP timeline from an animation manifest on the SVG inside
 * `root`. Browser-only.
 */
export function animate(root: Element, animations: Anim[], gsap: unknown): unknown;
