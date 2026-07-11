// rpic — JS/TS bindings for the rpic pic graphics language (WASM).
//
// Browser:  await ready();                         // wasm fetched automatically
// Node:     await ready(fs.readFileSync(url));      // pass the .wasm bytes/URL
// Math:     await ready(undefined, { math: true }); // texlabels typeset in-browser
import * as leanModule from './pkg/rpic_wasm.js';

let initPromise = null;
let wasm = null; // the initialized glue module (lean or math)
let initMath = null;

/**
 * Initialize the WebAssembly module (idempotent; concurrent calls share one
 * init). In the browser, call with no argument (the .wasm is fetched relative
 * to the module). In Node, pass the wasm bytes or a file URL.
 *
 * Pass `{ math: true }` to load the math-enabled build instead: it bundles
 * the RaTeX renderer so `texlabels` sources typeset `$…$` labels exactly like
 * the native CLI. The math glue + wasm are only fetched when requested, so
 * the lean fast path stays untouched. The choice is fixed by the first
 * successful (or in-flight) call; later calls asking for the other build
 * reject. A *failed* init (bad bytes, fetch error) clears the slot so
 * `ready()` can be retried. In Node, pass the bytes of
 * `pkg/rpic_wasm_math_bg.wasm` along with it.
 */
export function ready(wasmInput, opts = {}) {
  const wantsMath = !!opts.math;
  if (initPromise) {
    if (initMath !== wantsMath) {
      const current = initMath ? 'math-enabled' : 'lean';
      const requested = wantsMath ? 'math-enabled' : 'lean';
      return Promise.reject(
        new Error(`rpic: ready() already initialized the ${current} build; cannot switch to ${requested}`)
      );
    }
    return initPromise;
  }
  initMath = wantsMath;
  const attempt = (
    wantsMath ? import('./pkg/rpic_wasm_math.js') : Promise.resolve(leanModule)
  )
    .then(async (mod) => {
      await mod.default(wasmInput === undefined ? undefined : { module_or_path: wasmInput });
      wasm = mod;
    })
    .catch((e) => {
      // a settled rejection must not poison future calls — clear the slot
      // (only if it is still ours) so the next ready() starts fresh
      if (initPromise === attempt) {
        initPromise = null;
        initMath = null;
      }
      throw e;
    });
  initPromise = attempt;
  return initPromise;
}

function ensure() {
  if (!wasm) {
    throw new Error('rpic: call `await ready()` before compiling');
  }
}

/**
 * Compile pic source into `{ svg, animations, diagnostics, warnings, objects }`
 * (throws on a pic error). A top-level `scroll: true` is present when the source
 * used `animate scroll` — a hint to scrub the timeline on scroll (see `animate`).
 * @param {string} src
 * @param {{circuits?: boolean, texlabels?: boolean}} [opts]
 */
export function compile(src, opts = {}) {
  ensure();
  const json = opts.texlabels
    ? wasm.compile_with(src, !!opts.circuits, true)
    : opts.circuits
      ? wasm.compile_circuits(src)
      : wasm.compile(src);
  const out = JSON.parse(json);
  if (out.error) {
    const err = new Error(out.error);
    err.errorInfo = out.error_info;
    err.warnings = out.warnings ?? [];
    throw err;
  }
  return out;
}

/** Compile and return just the SVG string. */
export function renderSvg(src, opts) {
  return compile(src, opts).svg;
}

// The GSAP player lives in player.js — a zero-import module, so consumers
// with pre-rendered SVG can `import { animate } from '…/player.js'` and pull
// in nothing wasm-related. Re-exported here to keep the one-stop API.
export { animate, interactive } from './player.js';
