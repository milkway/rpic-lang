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

/**
 * Build a GSAP timeline from a drawing's animation manifest and play it on the
 * SVG inside `root`. Browser-only (needs the DOM and a GSAP instance).
 * @param {Element} root container holding the injected SVG
 * @param {Array<{id:string,effect:string,start:number,duration:number,repeat?:number,yoyo?:boolean,ease?:string,path?:string,color?:string,out?:boolean,from?:string}>} animations
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
        enterExit(tl, el, withOverrides({ opacity: 0, duration: a.duration, ease: 'power1.out' }, a), a);
        break;
      case 'pop':
        enterExit(
          tl,
          el,
          withOverrides({ scale: 0, transformOrigin: '50% 50%', duration: a.duration, ease: 'back.out(1.7)' }, a),
          a
        );
        break;
      case 'slide':
        slideIn(tl, el, a);
        break;
      case 'draw':
        drawOn(el, a, tl);
        break;
      case 'move':
        moveAlong(root, el, a, tl);
        break;
      case 'highlight':
        highlightWith(el, a, tl);
        break;
      default:
        enterExit(tl, el, withOverrides({ opacity: 0, duration: a.duration }, a), a);
    }
  }
  return tl;
}

// Entrances tween FROM the hidden `vars` to the natural state; `out` reverses
// it into an exit (TO the hidden state). Shared by fade/pop/slide.
function enterExit(tl, el, vars, a) {
  return a.out ? tl.to(el, vars, a.start) : tl.from(el, vars, a.start);
}

// The `slide` effect: enter (or, with `out`, leave) by translating from a
// compass direction, offset by the element's own extent so it clears its slot.
function slideIn(tl, el, a) {
  let bb = { width: 0, height: 0 };
  try {
    bb = el.getBBox();
  } catch {
    /* not yet laid out */
  }
  const off = {
    left: { x: -(bb.width * 1.5 || 40) },
    right: { x: bb.width * 1.5 || 40 },
    up: { y: -(bb.height * 1.5 || 40) },
    down: { y: bb.height * 1.5 || 40 },
  }[a.from || 'left'];
  enterExit(tl, el, withOverrides({ ...off, opacity: 0, duration: a.duration, ease: 'power2.out' }, a), a);
}

// Fold the optional GSAP overrides (repeat/yoyo/ease) into a tween's vars.
// `ease` replaces the effect's default easing; repeat/yoyo loop the tween.
function withOverrides(vars, a) {
  if (a.repeat) vars.repeat = a.repeat;
  if (a.yoyo) vars.yoyo = true;
  if (a.ease) vars.ease = a.ease;
  return vars;
}

// The `highlight` effect: emphasise `el`. With a target colour, tween the
// stroke of its geometry to that colour; without one, a colour-free scale
// pulse. It's a one-directional `.to()` — pair with `repeat 1 yoyo` for a
// flash-and-return, or `repeat -1 yoyo` for a continuous pulse.
function highlightWith(el, a, tl) {
  if (a.color) {
    const shapes = el.querySelectorAll('path, polyline, line, rect, circle, ellipse, polygon');
    tl.to(shapes, withOverrides({ stroke: a.color, duration: a.duration, ease: 'power1.inOut' }, a), a.start);
  } else {
    tl.to(
      el,
      withOverrides({ scale: 1.12, transformOrigin: '50% 50%', duration: a.duration, ease: 'power1.inOut' }, a),
      a.start
    );
  }
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
      // `out` reverses the trace: the stroke retracts (dashoffset 0 → len).
      const [begin, end] = a.out ? [0, len] : [len, 0];
      tl.fromTo(
        el,
        { strokeDasharray: len, strokeDashoffset: begin },
        withOverrides({ strokeDashoffset: end, duration: a.duration, ease: 'none' }, a),
        a.start
      );
    } else {
      enterExit(tl, el, withOverrides({ opacity: 0, duration: a.duration }, a), a);
    }
  });
  const texts = group.querySelectorAll('text');
  if (texts.length) {
    tl.from(texts, { opacity: 0, duration: a.duration * 0.6 }, a.start + a.duration * 0.4);
  }
}
