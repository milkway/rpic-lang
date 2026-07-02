// rpic — JS/TS bindings for the rpic pic graphics language (WASM).
//
// Browser:  await ready();                         // wasm fetched automatically
// Node:     await ready(fs.readFileSync(url));      // pass the .wasm bytes/URL
import initWasm, {
  compile as wasmCompile,
  compile_circuits as wasmCompileCircuits,
  compile_with as wasmCompileWith,
} from './pkg/rpic_wasm.js';

let initPromise = null;
let initialized = false;

/**
 * Initialize the WebAssembly module (idempotent; concurrent calls share one
 * init). In the browser, call with no argument (the .wasm is fetched relative
 * to the module). In Node, pass the wasm bytes or a file URL.
 */
export function ready(wasmInput) {
  if (!initPromise) {
    initPromise = initWasm(
      wasmInput === undefined ? undefined : { module_or_path: wasmInput }
    ).then(() => {
      initialized = true;
    });
  }
  return initPromise;
}

function ensure() {
  if (!initialized) {
    throw new Error('rpic: call `await ready()` before compiling');
  }
}

/**
 * Compile pic source into `{ svg, animations, diagnostics }` (throws on a pic error).
 * @param {string} src
 * @param {{circuits?: boolean, texlabels?: boolean}} [opts]
 */
export function compile(src, opts = {}) {
  ensure();
  const json = opts.texlabels
    ? wasmCompileWith(src, !!opts.circuits, true)
    : opts.circuits
      ? wasmCompileCircuits(src)
      : wasmCompile(src);
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
    const sel =
      typeof CSS !== 'undefined' && CSS.escape ? '#' + CSS.escape(a.id) : `[id="${a.id}"]`;
    const el = root.querySelector(sel);
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
    // Filled, unstroked elements (arrowheads) can't be dash-traced — pop
    // them in as the shaft reaches the tip instead.
    const fill = el.getAttribute('fill');
    if (el.getAttribute('stroke-width') === '0' || (fill && fill !== 'none' && !el.getAttribute('stroke'))) {
      tl.from(
        el,
        { opacity: 0, scale: 0, transformOrigin: '50% 50%', duration: Math.min(0.2, a.duration * 0.4), ease: 'back.out(1.7)' },
        a.start + a.duration * 0.8
      );
      return;
    }
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
