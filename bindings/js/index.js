// @milkway/rpic — JS/TS bindings for the rpic pic graphics language (WASM).
//
// Browser:  await ready();                         // wasm fetched automatically
// Node:     await ready(fs.readFileSync(url));      // pass the .wasm bytes/URL
import initWasm, {
  compile as wasmCompile,
  compile_circuits as wasmCompileCircuits,
} from './pkg/rpic_wasm.js';

let initialized = false;

/**
 * Initialize the WebAssembly module. In the browser, call with no argument
 * (the .wasm is fetched relative to the module). In Node, pass the wasm bytes
 * or a file URL.
 */
export async function ready(wasmInput) {
  if (initialized) return;
  await initWasm(wasmInput === undefined ? undefined : { module_or_path: wasmInput });
  initialized = true;
}

function ensure() {
  if (!initialized) {
    throw new Error('rpic: call `await ready()` before compiling');
  }
}

/**
 * Compile pic source into `{ svg, animations }` (throws on a pic error).
 * @param {string} src
 * @param {{circuits?: boolean}} [opts]
 */
export function compile(src, opts = {}) {
  ensure();
  const json = opts.circuits ? wasmCompileCircuits(src) : wasmCompile(src);
  const out = JSON.parse(json);
  if (out.error) throw new Error(out.error);
  return out;
}

/** Compile and return just the SVG string. */
export function renderSvg(src, opts) {
  return compile(src, opts).svg;
}

/**
 * Build a GSAP timeline from a drawing's animation manifest and play it on the
 * SVG inside `root`. Browser-only (needs the DOM and a GSAP instance).
 * @param {Element} root container holding the injected SVG
 * @param {Array<{id:string,effect:string,start:number,duration:number}>} animations
 * @param {*} gsap the GSAP instance
 * @returns the GSAP timeline
 */
export function animate(root, animations, gsap) {
  const tl = gsap.timeline();
  for (const a of animations) {
    const el = root.querySelector('#' + (window.CSS ? CSS.escape(a.id) : a.id));
    if (!el) continue;
    switch (a.effect) {
      case 'fade':
        tl.from(el, { opacity: 0, duration: a.duration, ease: 'power1.out' }, a.start);
        break;
      case 'pop':
        tl.from(
          el,
          { scale: 0, transformOrigin: '50% 50%', duration: a.duration, ease: 'back.out(1.7)' },
          a.start
        );
        break;
      case 'draw':
        drawOn(el, a, tl);
        break;
      default:
        tl.from(el, { opacity: 0, duration: a.duration }, a.start);
    }
  }
  return tl;
}

function drawOn(group, a, tl) {
  const els = group.querySelectorAll('path, polyline, line, rect, circle, ellipse, polygon');
  els.forEach((el) => {
    let len = 0;
    try {
      len = el.getTotalLength();
    } catch {
      len = 0;
    }
    if (len > 0) {
      tl.fromTo(
        el,
        { strokeDasharray: len, strokeDashoffset: len },
        { strokeDashoffset: 0, duration: a.duration, ease: 'none' },
        a.start
      );
    } else {
      tl.from(el, { opacity: 0, duration: a.duration }, a.start);
    }
  });
  const texts = group.querySelectorAll('text');
  if (texts.length) {
    tl.from(texts, { opacity: 0, duration: a.duration * 0.6 }, a.start + a.duration * 0.4);
  }
}
