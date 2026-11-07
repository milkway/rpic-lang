// rpic — JS/TS bindings for the rpic pic graphics language (WASM).
//
// Browser:  await ready();                         // wasm fetched automatically
// Node:     await ready(fs.readFileSync(url));      // pass the .wasm bytes/URL
// Math:     await ready(undefined, { math: true }); // texlabels typeset in-browser
// Maps:     await ready(undefined, { geo: true });  // compileGeoJSON (drawing → GeoJSON)
import * as leanModule from './pkg/rpic_wasm.js';
// the one layer-options validator (maplibre.js itself stays zero-import)
import { checkLayerOptions } from './maplibre.js';

// display names of the three builds, keyed by ready()'s choice
const BUILDS = { lean: 'lean', math: 'math-enabled', geo: 'geo-enabled' };

let initPromise = null;
let wasm = null; // the initialized glue module (lean, math or geo)
let initBuild = null; // 'lean' | 'math' | 'geo' while an init is pending/done

/**
 * Initialize the WebAssembly module (idempotent; concurrent calls share one
 * init). In the browser, call with no argument (the .wasm is fetched relative
 * to the module). In Node, pass the wasm bytes or a file URL.
 *
 * Pass `{ math: true }` to load the math-enabled build instead: it bundles
 * the RaTeX renderer so `texlabels` sources typeset `$…$` labels exactly like
 * the native CLI. Pass `{ geo: true }` for the geo-enabled build, which adds
 * `compileGeoJSON`/`geoToCanvas` (geopic: drawings as GeoJSON). Either extra
 * glue + wasm is only fetched when requested, so the lean fast path stays
 * untouched; the two cannot be combined. The choice is fixed by the first
 * successful (or in-flight) call; later calls asking for another build
 * reject. A *failed* init (bad bytes, fetch error) clears the slot so
 * `ready()` can be retried. In Node, pass the bytes of
 * `pkg/rpic_wasm_math_bg.wasm` / `pkg/rpic_wasm_geo_bg.wasm` along with it.
 */
export function ready(wasmInput, opts = {}) {
  if (opts.math && opts.geo) {
    return Promise.reject(new Error('rpic: ready() cannot load the math and geo builds together; pick one'));
  }
  const wanted = opts.math ? 'math' : opts.geo ? 'geo' : 'lean';
  if (initPromise) {
    if (initBuild !== wanted) {
      return Promise.reject(
        new Error(
          `rpic: ready() already initialized the ${BUILDS[initBuild]} build; cannot switch to ${BUILDS[wanted]}`
        )
      );
    }
    return initPromise;
  }
  initBuild = wanted;
  const load =
    wanted === 'math'
      ? import('./pkg/rpic_wasm_math.js')
      : wanted === 'geo'
        ? import('./pkg/rpic_wasm_geo.js')
        : Promise.resolve(leanModule);
  const attempt = load
    .then(async (mod) => {
      await mod.default(wasmInput === undefined ? undefined : { module_or_path: wasmInput });
      wasm = mod;
    })
    .catch((e) => {
      // a settled rejection must not poison future calls — clear the slot
      // (only if it is still ours) so the next ready() starts fresh
      if (initPromise === attempt) {
        initPromise = null;
        initBuild = null;
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
 *
 * Pass `scene: true` for the device-independent resolved draw list as a
 * `scene` key (see docs/scene.md; versioned, y-up inches, cubic curves);
 * `flatten` (inches) delivers its curves as polylines instead, and
 * `outline` embeds typeset math as SVG glyph fragments.
 *
 * `preludes: [{ name, text }]` are named source units (a macro library, a
 * house style …) compiled before `src`, after `circuits`/`texlabels`, in
 * order: positions inside one carry `file: name`, the caller's own positions
 * never shift, and objects drawn by a prelude macro called from `src` keep
 * the call's provenance. A bad name throws with `errorInfo.kind ===
 * 'invalid_options'`.
 * @param {string} src
 * @param {{circuits?: boolean, texlabels?: boolean, scene?: boolean, flatten?: number, outline?: boolean, preludes?: {name: string, text: string}[]}} [opts]
 */
export function compile(src, opts = {}) {
  ensure();
  if (opts.flatten !== undefined && !(Number.isFinite(opts.flatten) && opts.flatten > 0)) {
    throw new Error('rpic: flatten must be a positive finite tolerance in inches');
  }
  const preludes = normalizePreludes(opts.preludes);
  const scene = !!opts.scene || opts.flatten !== undefined;
  const json = preludes.length
    ? wasm.compile_preludes(
        src,
        !!opts.circuits,
        !!opts.texlabels,
        preludes.map((p) => p.name),
        preludes.map((p) => p.text),
        scene,
        opts.flatten,
        !!opts.outline,
      )
    : scene
    ? wasm.compile_scene(src, !!opts.circuits, !!opts.texlabels, opts.flatten, !!opts.outline)
    : opts.texlabels
      ? wasm.compile_with(src, !!opts.circuits, true)
      : opts.circuits
        ? wasm.compile_circuits(src)
        : wasm.compile(src);
  const out = JSON.parse(json);
  if (out.error) {
    throw compileError(out);
  }
  return out;
}

/**
 * `preludes` as `[{ name, text }]` with string fields (the engine validates
 * the names); `undefined`/`null` = none.
 */
function normalizePreludes(preludes) {
  if (preludes == null) return [];
  if (!Array.isArray(preludes)) {
    throw new TypeError('rpic: preludes must be an array of { name, text }');
  }
  return preludes.map((p, i) => {
    if (!p || typeof p.name !== 'string' || typeof p.text !== 'string') {
      throw new TypeError(`rpic: preludes[${i}] must be { name: string, text: string }`);
    }
    return { name: p.name, text: p.text };
  });
}

/** The Error a pic error becomes: `.errorInfo` (a Diagnostic) + `.warnings`. */
function compileError(out) {
  const err = new Error(out.error);
  err.errorInfo = out.error_info;
  err.warnings = out.warnings ?? [];
  return err;
}

/** Compile and return just the SVG string. */
export function renderSvg(src, opts) {
  return compile(src, opts).svg;
}

// ---------------------------------------------------------------------------
// geopic: pic drawings as GeoJSON (docs/geopic.md). Needs the geo build:
// `await ready(undefined, { geo: true })`. Mirrors Python's `rpic.geo`.
// ---------------------------------------------------------------------------

/**
 * A georeferencing error. `.message` is the readable text; `.info` is the
 * structured `{ kind, message, hint? }` (`no_registration`, `invalid_anchor`,
 * `invalid_frame`, `invalid_options`, …).
 */
export class GeoError extends Error {
  constructor(message, info) {
    super(message);
    this.name = 'GeoError';
    this.info = info;
  }
}

// pic labels: an uppercase letter, then letters, digits or underscores.
// Feature names are written into pic source, so anything else is rejected
// outright — never interpolated.
const LABEL = /^[A-Z][A-Za-z0-9_]*$/;
const FORMS =
  'rpic: features must be a {name: [lon, lat]} / {name: {lon, lat, bbox?}} object, ' +
  'or an array of {name, lon, lat, bbox?} objects or [name, lon, lat, bbox?] tuples';

function featureLabel(name) {
  const s = String(name);
  if (!LABEL.test(s)) {
    throw new Error(
      `rpic: feature name ${JSON.stringify(s)} is not a pic label (an uppercase letter followed by letters, digits or underscores)`
    );
  }
  return s;
}

function featureCoord(v, what) {
  if (typeof v !== 'number' || !Number.isFinite(v)) {
    throw new Error(`rpic: feature ${what} must be a finite number, got ${JSON.stringify(v) ?? String(v)}`);
  }
  return v;
}

function featureBbox(name, bbox) {
  if (bbox === undefined || bbox === null) return null;
  if (!Array.isArray(bbox) || bbox.length !== 4) {
    throw new Error(`rpic: feature ${JSON.stringify(name)}: bbox must be [west, south, east, north]`);
  }
  return bbox.map((v) => featureCoord(v, `${name} bbox`));
}

function oneFeature(name, pos) {
  const n = featureLabel(name);
  if (Array.isArray(pos)) {
    if (pos.length !== 2 && pos.length !== 3) throw new Error(FORMS);
    return { name: n, lon: featureCoord(pos[0], `${n} lon`), lat: featureCoord(pos[1], `${n} lat`), bbox: featureBbox(n, pos[2]) };
  }
  if (pos && typeof pos === 'object') {
    return { name: n, lon: featureCoord(pos.lon, `${n} lon`), lat: featureCoord(pos.lat, `${n} lat`), bbox: featureBbox(n, pos.bbox) };
  }
  throw new Error(FORMS);
}

/** `features` in any accepted form → `[{name, lon, lat, bbox|null}]`. */
function normalizeFeatures(features) {
  if (features === undefined || features === null) return [];
  if (Array.isArray(features)) {
    const out = features.map((item) => {
      if (Array.isArray(item)) {
        if (item.length !== 3 && item.length !== 4) throw new Error(FORMS);
        if (typeof item[0] !== 'string') throw new Error(`${FORMS} (name is required)`);
        return oneFeature(item[0], item.slice(1));
      }
      if (item && typeof item === 'object') {
        if (typeof item.name !== 'string') throw new Error(`${FORMS} (name is required)`);
        return oneFeature(item.name, item);
      }
      throw new Error(FORMS);
    });
    return unique(out);
  }
  if (typeof features === 'object') {
    return Object.entries(features).map(([name, pos]) => oneFeature(name, pos));
  }
  throw new Error(FORMS);
}

/** Reject a name given twice: which position would win is not obvious. */
function unique(feats) {
  const seen = new Set();
  for (const f of feats) {
    if (seen.has(f.name)) throw new Error(`rpic: feature ${JSON.stringify(f.name)} is given more than once`);
    seen.add(f.name);
  }
  return feats;
}

/**
 * Re-base one position (`line`/`file`) past the `k` prelude lines. A
 * position *inside* the prelude is not the caller's: it is tagged
 * `file: 'features'` (`line` stays relative to the prelude).
 */
function rebaseLine(d, k) {
  if (!k || !d || typeof d !== 'object' || d.file != null) return;
  if (!Number.isInteger(d.line) || d.line < 1) return;
  if (d.line > k) d.line -= k;
  else d.file = 'features';
}

/**
 * Shift warnings back by the `k` prelude lines the host prepended, so
 * positions refer to the caller's own source (positions inside an
 * include/library carry a `file` and are left alone).
 */
function rebase(out, k) {
  if (!k) return out;
  for (const w of out.warnings ?? []) {
    if (w.span && typeof w.span === 'object') rebaseLine(w.span, k);
    else rebaseLine(w, k);
  }
  return out;
}

function geoWasm() {
  ensure();
  if (typeof wasm.compile_geo !== 'function') {
    throw new Error('rpic: compileGeoJSON needs the geo-enabled build: `await ready(undefined, { geo: true })`');
  }
  return wasm;
}

/** One native pass, diagnostics re-based by `k` lines. */
function geoPass(src, k, geoOpts, opts) {
  const out = JSON.parse(geoWasm().compile_geo(src, !!opts.circuits, !!opts.texlabels, JSON.stringify(geoOpts)));
  if (out.error_info) {
    const err = compileError(out);
    rebaseLine(err.errorInfo, k);
    throw err;
  }
  if (out.geo_error) throw new GeoError(out.error, out.geo_error);
  return rebase(out, k);
}

/** 9 significant digits, like `%.9g` (the pic lexer reads exponents). */
const num = (x) => String(Number(x.toPrecision(9)));

/**
 * pic statements defining every feature at its position, in the source's own
 * units (pic `scale`, before the whole-picture `rescale`). A feature with a
 * bbox becomes an invisible labelled box spanning the axis-aligned canvas
 * extent of its four corners; a point feature a plain place. Names were
 * validated by `featureLabel` — nothing else reaches the source.
 */
function prelude(registration, canvas, feats) {
  const units = (canvas.scale ?? 1) / (canvas.rescale ?? 1); // source units per emitted inch
  const pts = [];
  for (const f of feats) {
    pts.push([f.lon, f.lat]);
    if (f.bbox) {
      const [w, s, e, n] = f.bbox;
      pts.push([w, s], [e, s], [e, n], [w, n]);
    }
  }
  const canvasPts = geoToCanvas(registration, pts);
  let i = 0;
  const lines = feats.map((f) => {
    const [x, y] = canvasPts[i++];
    const at = `(${num(x * units)}, ${num(y * units)})`;
    if (!f.bbox) return `${f.name}: ${at}`;
    const corners = canvasPts.slice(i, i + 4);
    i += 4;
    const xs = corners.map((c) => c[0]);
    const ys = corners.map((c) => c[1]);
    const wid = (Math.max(...xs) - Math.min(...xs)) * units;
    const ht = (Math.max(...ys) - Math.min(...ys)) * units;
    return `${f.name}: box invis wid ${num(wid)} ht ${num(ht)} at ${at}`;
  });
  return lines.join('\n') + '\n';
}

/**
 * Two passes. Features the source itself defines (as places or labelled
 * objects) are control points of the source's own geometry; the others are
 * injected — placed by the registration of a first pass — as places or
 * invisible boxes, then everything is georeferenced together.
 */
function compileWithFeatures(src, feats, base, opts) {
  const names = new Set(feats.map((f) => f.name));
  const k = feats.length;
  const placeholders = feats.map((f) => `${f.name}: (0, 0)\n`).join('');
  // which names does the source define? their definitions sit after the
  // placeholder lines (a span is relative to the caller's text + prelude)
  let scene;
  try {
    scene = compile(placeholders + src, {
      circuits: opts.circuits,
      texlabels: opts.texlabels,
      preludes: opts.preludes,
      scene: true,
    }).scene;
  } catch (e) {
    rebaseLine(e.errorInfo, k);
    throw e;
  }
  const defined = new Set();
  for (const p of [...(scene.places ?? []), ...(scene.items ?? [])]) {
    // a definition inside a named prelude (`file` set) is the source's too
    if (names.has(p.label) && (p.span?.file != null || (p.span?.line ?? 0) > k)) defined.add(p.label);
  }
  const own = feats.filter((f) => defined.has(f.name));
  const inject = feats.filter((f) => !defined.has(f.name));
  // pass 1: the registration — declared anchor, or the source's control
  // points, the source-defined features among them
  const first = geoPass(placeholders + src, k, { ...base, features: own }, opts);
  if (!inject.length) return first;
  const pre = prelude(first.registration, first.canvas, inject);
  const out = geoPass(pre + src, inject.length, { ...base, features: feats }, opts);
  if (first.registration.method === 'anchor' && first.registration.control_points === 0) {
    // the injected features were placed *by* the declared values; the second
    // pass's "declared values ignored" would blame the source
    out.warnings = out.warnings.filter((w) => w.kind !== 'ignored_with_control_points');
  }
  return out;
}

/**
 * Numeric options are checked here: `JSON.stringify` would turn NaN/Infinity
 * into `null`, which the wasm reads as "not given" — a silent fallback.
 */
function checkGeoOptions(opts) {
  const finite = (v) => typeof v === 'number' && Number.isFinite(v);
  for (const key of ['scale', 'flatten', 'viewScale']) {
    if (opts[key] !== undefined && !(finite(opts[key]) && opts[key] > 0)) {
      throw new Error(`rpic: ${key} must be a positive finite number`);
    }
  }
  if (opts.bearing !== undefined && !finite(opts.bearing)) {
    throw new Error('rpic: bearing must be a finite number (degrees)');
  }
  if (
    opts.anchor !== undefined &&
    !(Array.isArray(opts.anchor) && opts.anchor.length === 2 && opts.anchor.every(finite))
  ) {
    throw new Error('rpic: anchor must be [lon, lat], two finite numbers');
  }
}

/**
 * MapLibre layer options (`{ textScale }`) in the engine's JSON spelling,
 * validated by the maplibre module's own check (one validator, run before the
 * wasm — `JSON.stringify` would also turn NaN/Infinity into `null`, i.e. the
 * default).
 */
function layerOptions(o) {
  return { text_scale: checkLayerOptions(o).textScale };
}

/**
 * Georeference pic source: `{ geojson, warnings, diagnostics, registration,
 * canvas }`.
 * Needs the geo build (`await ready(undefined, { geo: true })`).
 *
 * `frame` (`'enu'` | `'mercator'`), `anchor` `[lon, lat]`, `scale` (metres per
 * inch), `bearing` (degrees clockwise from north), `viewScale` (px per inch
 * under `geo_px = 1`) and `screenClasses` override the source's `geo_*`
 * variables; `flatten` is the curve tolerance (inches, default `1e-3`).
 *
 * `features` names points the source may use without defining them
 * (`arrow from Recife to Olinda`): features the source does define act as
 * control points of its own geometry; the others are projected by a first
 * pass's registration and injected as places — invisible labelled boxes when
 * they have a `bbox`, so `Name.n`/`Name.e` follow its extent — before the
 * final pass. Diagnostics keep the caller's line numbers.
 *
 * `preludes: [{ name, text }]` are named source units compiled before
 * `src`, as in `compile`.
 *
 * `layers: true` adds `layers`: the MapLibre style-spec layers that draw the
 * collection with sizes true to the ground (source id `"pic"`; hand them to
 * `attach` from `@strategicprojects/rpic/maplibre`). `layers: { textScale }`
 * does the same with every label's font size multiplied by `textScale`.
 *
 * Throws an `Error` with `.errorInfo` on a pic error (like `compile`) and a
 * `GeoError` (`.info = { kind, message, hint }`) on a georeferencing error.
 */
export function compileGeoJSON(src, opts = {}) {
  geoWasm();
  checkGeoOptions(opts);
  // validated once, here, before any compile_geo pass
  const layers = !opts.layers ? undefined : typeof opts.layers === 'object' ? layerOptions(opts.layers) : true;
  const feats = normalizeFeatures(opts.features);
  const base = {
    frame: opts.frame,
    anchor: opts.anchor,
    scale: opts.scale,
    bearing: opts.bearing,
    flatten: opts.flatten,
    view_scale: opts.viewScale,
    screen_classes: opts.screenClasses,
    // named preludes (compiled before the source, see `compile`)
    preludes: normalizePreludes(opts.preludes),
    // the MapLibre layers (docs/geopic.md § 6); absent unless asked for
    layers,
  };
  return feats.length
    ? compileWithFeatures(src, feats, base, opts)
    : geoPass(src, 0, { ...base, features: [] }, opts);
}

/**
 * Canvas positions (inches) of WGS84 `[lon, lat]` points under a
 * registration (as `compileGeoJSON` returned it). Needs the geo build.
 * @param {object | string} registration
 * @param {[number, number][]} points
 * @returns {[number, number][]}
 */
export function geoToCanvas(registration, points) {
  const reg = typeof registration === 'string' ? registration : JSON.stringify(registration);
  const out = JSON.parse(geoWasm().geo_to_canvas(reg, JSON.stringify(points)));
  if (!Array.isArray(out)) throw new GeoError(out.error, out.geo_error);
  return out;
}

/**
 * The MapLibre layers (docs/geopic.md § 6) that draw a geopic
 * FeatureCollection — the engine's own output, as `compileGeoJSON(src,
 * { layers: true })` gives it. Needs the geo build.
 * @param {object | string} fc a FeatureCollection from `compileGeoJSON` (or its JSON text)
 * @param {{ textScale?: number }} [opts] `textScale` multiplies every label's font size
 * @returns {object[]}
 */
export function maplibreLayers(fc, opts = {}) {
  const text = typeof fc === 'string' ? fc : JSON.stringify(fc);
  const out = JSON.parse(geoWasm().maplibre_layers(text, JSON.stringify(layerOptions(opts))));
  if (!Array.isArray(out)) throw new GeoError(out.error, out.geo_error);
  return out;
}

// The GSAP player lives in player.js — a zero-import module, so consumers
// with pre-rendered SVG can `import { animate } from '…/player.js'` and pull
// in nothing wasm-related. Re-exported here to keep the one-stop API.
export { animate, interactive } from './player.js';
