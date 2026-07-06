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
 * Compile pic source into `{ svg, animations, diagnostics, warnings, objects }` (throws on a pic error).
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

/**
 * Build a GSAP timeline from a drawing's animation manifest and play it on the
 * SVG inside `root`. Browser-only (needs the DOM and a GSAP instance).
 * @param {Element} root container holding the injected SVG
 * @param {Array<{id:string,effect:string,start:number,duration:number,repeat?:number,yoyo?:boolean,ease?:string,path?:string}>} animations
 * @param {*} gsap the GSAP instance (register MotionPathPlugin for the `move` effect)
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
        tl.from(el, withOverrides({ opacity: 0, duration: a.duration, ease: 'power1.out' }, a), a.start);
        break;
      case 'pop':
        tl.from(
          el,
          withOverrides({ scale: 0, transformOrigin: '50% 50%', duration: a.duration, ease: 'back.out(1.7)' }, a),
          a.start
        );
        break;
      case 'draw':
        drawOn(el, a, tl);
        break;
      case 'move':
        moveAlong(root, el, a, tl);
        break;
      default:
        tl.from(el, withOverrides({ opacity: 0, duration: a.duration }, a), a.start);
    }
  }
  return tl;
}

// Fold the optional GSAP overrides (repeat/yoyo/ease) into a tween's vars.
// `ease` replaces the effect's default easing; repeat/yoyo loop the tween.
function withOverrides(vars, a) {
  if (a.repeat) vars.repeat = a.repeat;
  if (a.yoyo) vars.yoyo = true;
  if (a.ease) vars.ease = a.ease;
  return vars;
}

// The `move` effect: travel `el` along the geometry of the object `a.path`
// references, via GSAP's MotionPathPlugin. The consumer must have registered
// it (`gsap.registerPlugin(MotionPathPlugin)`); without it GSAP no-ops the
// motionPath and the object simply stays put.
function moveAlong(root, el, a, tl) {
  if (!a.path) return;
  const psel =
    typeof CSS !== 'undefined' && CSS.escape ? '#' + CSS.escape(a.path) : `[id="${a.path}"]`;
  const group = root.querySelector(psel);
  const pathEl = group && group.querySelector('path, polyline, line, polygon');
  if (!pathEl) return;
  // `align` maps the path into the traveller's coordinate space; a `move`
  // tween is a `.to()` (current position → along the path), not a `.from()`.
  tl.to(
    el,
    withOverrides(
      {
        motionPath: { path: pathEl, align: pathEl, alignOrigin: [0.5, 0.5], autoRotate: false },
        duration: a.duration,
        ease: 'none',
      },
      a
    ),
    a.start
  );
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
        withOverrides({ strokeDashoffset: 0, duration: a.duration, ease: 'none' }, a),
        a.start
      );
    } else {
      tl.from(el, withOverrides({ opacity: 0, duration: a.duration }, a), a.start);
    }
  });
  const texts = group.querySelectorAll('text');
  if (texts.length) {
    tl.from(texts, { opacity: 0, duration: a.duration * 0.6 }, a.start + a.duration * 0.4);
  }
}
