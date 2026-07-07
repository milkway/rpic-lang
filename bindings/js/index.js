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
 * @param {Array<{id:string,effect:string,start:number,duration:number,repeat?:number,yoyo?:boolean,ease?:string,path?:string,color?:string,out?:boolean,from?:string,morph?:string}>} animations
 * @param {*} gsap the GSAP instance (register MotionPathPlugin for `move`, MorphSVGPlugin for `morph`)
 * @returns the GSAP timeline
 */
export function animate(root, animations, gsap) {
  const tl = gsap.timeline();
  // GSAP's MotionPath/MorphSVG plugins need real <path> elements, but rpic
  // emits <line>/<rect>/<circle>/<polygon>. Convert the shapes those effects
  // reference to <path> up front — before any tween captures element refs, so
  // a `draw` on the same shape traces the path instead of a detached primitive.
  preconvertGeometry(root, animations);
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
      case 'morph':
        morphInto(root, el, a, tl);
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

const SVG_NS = 'http://www.w3.org/2000/svg';

// Build SVG path data (`d`) for the primitives rpic emits. GSAP's MotionPath
// and MorphSVG need a real <path>; passing a raw <line>/<rect>/<circle> throws
// "Expecting a <path> element or an SVG path data string".
function pathDataOf(el) {
  const n = el.tagName.toLowerCase();
  const f = (a) => parseFloat(el.getAttribute(a)) || 0;
  if (n === 'path') return el.getAttribute('d');
  if (n === 'line') return `M${f('x1')},${f('y1')} L${f('x2')},${f('y2')}`;
  if (n === 'polyline' || n === 'polygon') {
    const nums = (el.getAttribute('points') || '').trim().split(/[\s,]+/).map(Number);
    if (nums.length < 4) return null;
    let d = '';
    for (let i = 0; i + 1 < nums.length; i += 2) d += `${i ? 'L' : 'M'}${nums[i]},${nums[i + 1]} `;
    return n === 'polygon' ? d + 'Z' : d.trim();
  }
  if (n === 'rect') return `M${f('x')},${f('y')} h${f('width')} v${f('height')} h${-f('width')} Z`;
  if (n === 'circle') {
    const r = f('r'), cx = f('cx'), cy = f('cy');
    return `M${cx - r},${cy} a${r},${r} 0 1,0 ${2 * r},0 a${r},${r} 0 1,0 ${-2 * r},0 Z`;
  }
  if (n === 'ellipse') {
    const rx = f('rx'), ry = f('ry'), cx = f('cx'), cy = f('cy');
    return `M${cx - rx},${cy} a${rx},${ry} 0 1,0 ${2 * rx},0 a${rx},${ry} 0 1,0 ${-2 * rx},0 Z`;
  }
  return null;
}

// Replace a primitive SVG element with an equivalent <path> (preserving paint),
// in place. Idempotent: a <path> is returned untouched.
function convertToPath(el) {
  if (!el || el.tagName.toLowerCase() === 'path') return el;
  const d = pathDataOf(el);
  if (!d || !el.parentNode || typeof document === 'undefined') return el;
  const p = document.createElementNS(SVG_NS, 'path');
  p.setAttribute('d', d);
  for (const attr of el.attributes) {
    if (!/^(x1|y1|x2|y2|points|x|y|width|height|cx|cy|r|rx|ry)$/.test(attr.name)) {
      p.setAttribute(attr.name, attr.value);
    }
  }
  el.parentNode.replaceChild(p, el);
  return p;
}

// Convert every shape a `move`/`morph` references to a <path> before any tween
// captures element references (so a concurrent `draw` traces the path, not a
// now-detached primitive).
function preconvertGeometry(root, animations) {
  const ALL = 'path, polyline, line, rect, circle, ellipse, polygon';
  const pick = (id, sel) => {
    if (!id) return;
    const g = root.querySelector(`[id="${id}"]`);
    const prim = g && g.querySelector(sel);
    if (prim) convertToPath(prim);
  };
  for (const a of animations || []) {
    if (a.effect === 'move') pick(a.path, 'path, polyline, line');
    if (a.effect === 'morph') {
      pick(a.id, ALL);
      pick(a.morph, ALL);
    }
  }
}

// The `highlight` effect: emphasise `el`. With a target colour, recolour and
// thicken its outline AND give the whole object a small scale pulse, so the
// emphasis reads at a glance rather than a bare colour swap; without a colour,
// a colour-free scale pulse. One-directional `.to()` — pair with `repeat 1
// yoyo` for a flash-and-return, or `repeat -1 yoyo` for a continuous pulse.
function highlightWith(el, a, tl) {
  if (a.color) {
    const shapes = el.querySelectorAll('path, polyline, line, rect, circle, ellipse, polygon');
    tl.to(
      shapes,
      withOverrides({ stroke: a.color, strokeWidth: '+=2', duration: a.duration, ease: 'power1.inOut' }, a),
      a.start
    );
    tl.to(
      el,
      withOverrides({ scale: 1.1, transformOrigin: '50% 50%', duration: a.duration, ease: 'power1.inOut' }, a),
      a.start
    );
  } else {
    tl.to(
      el,
      withOverrides({ scale: 1.12, transformOrigin: '50% 50%', duration: a.duration, ease: 'power1.inOut' }, a),
      a.start
    );
  }
}

// The `morph` effect: tween `el`'s outline into the shape of the object
// `a.morph` references, via GSAP's MorphSVGPlugin. The consumer must have
// registered it (`gsap.registerPlugin(MorphSVGPlugin)`); without it GSAP
// no-ops the morphSVG and the shape stays put. Both source and target were
// converted to <path> by preconvertGeometry.
function morphInto(root, el, a, tl) {
  if (!a.morph) return;
  const tsel =
    typeof CSS !== 'undefined' && CSS.escape ? '#' + CSS.escape(a.morph) : `[id="${a.morph}"]`;
  const target = root.querySelector(tsel);
  const src = el.querySelector('path');
  const dst = target && target.querySelector('path');
  if (!src || !dst) return;
  tl.to(src, withOverrides({ morphSVG: dst, duration: a.duration, ease: 'power1.inOut' }, a), a.start);
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
  // The shaft was converted to a <path> by preconvertGeometry; skip the
  // arrowhead <polygon> — we want the line the traveller rides, not the tip.
  const pathEl = group && group.querySelector('path');
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
