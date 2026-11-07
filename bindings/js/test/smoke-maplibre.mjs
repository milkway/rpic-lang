// The MapLibre module (maplibre.js): its JS port of the layers equals the
// engine's (geopic_core::maplibre through the geo wasm) and the pre-move
// snapshots in examples/geo/maplibre/; `attach`/`createGeoMap` drive a fake
// map that records every call. Runs in its own process (ready() fixes the
// build per module instance).
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { attach, bounds, createGeoMap, LAYER_IDS, layers, sizing, SOURCE } from '../maplibre.js';
import { maplibreLayers, ready } from '../index.js';

// zero imports: the module must stay loadable without wasm or maplibre-gl
const own = readFileSync(new URL('../maplibre.js', import.meta.url), 'utf8');
assert.ok(!/^\s*import\s/m.test(own) && !/\bimport\(/.test(own), 'maplibre.js must have no imports');

/** Deep equality, numbers within a relative 1e-12 (cos φ comes from the platform libm). */
function close(a, b, path = '') {
  if (typeof a === 'number' && typeof b === 'number') {
    const tol = 1e-12 * Math.max(Math.abs(a), Math.abs(b));
    assert.ok(Math.abs(a - b) <= tol, `${path}: ${a} vs ${b}`);
  } else if (Array.isArray(a)) {
    assert.ok(Array.isArray(b) && a.length === b.length, `${path}: array length`);
    a.forEach((x, i) => close(x, b[i], `${path}[${i}]`));
  } else if (a && typeof a === 'object') {
    assert.deepEqual(Object.keys(a).sort(), Object.keys(b).sort(), `${path}: keys`);
    for (const k of Object.keys(a)) close(a[k], b[k], `${path}.${k}`);
  } else {
    assert.equal(a, b, path);
  }
}

// ---- the cases: the four examples, the callout under mercator, empty --------
const geoDir = new URL('../../../examples/geo/', import.meta.url);
const snapDir = new URL('maplibre/', geoDir);
const read = (u) => JSON.parse(readFileSync(u, 'utf8'));
const cases = {};
for (const f of readdirSync(geoDir).filter((f) => f.endsWith('.geojson'))) {
  cases[f.replace(/\.geojson$/, '')] = read(new URL(f, geoDir));
}
assert.ok(Object.keys(cases).length >= 4);
const merc = structuredClone(cases.callout);
merc.geopic.registration.frame = 'mercator';
cases['callout-mercator'] = merc;
cases.empty = { type: 'FeatureCollection', features: [] };

const wasm = readFileSync(new URL('../pkg/rpic_wasm_geo_bg.wasm', import.meta.url));
await ready(wasm, { geo: true });

for (const [name, fc] of Object.entries(cases)) {
  const port = layers(fc);
  assert.deepEqual(port.map((l) => l.id), LAYER_IDS, name);
  close(port, read(new URL(`${name}.layers.json`, snapDir)), `${name} vs snapshot`);
  close(port, maplibreLayers(fc), `${name} vs engine`);
}

// sizing: 400 m/in at lat −8.0631 → p₀ = 1/(78271.517·cos φ), 11 pt → stop 18
const s = sizing(cases.callout);
assert.equal(s.mPerIn, 400);
assert.equal(s.pxPerMetreAtZ0, 1 / (78271.517 * Math.cos((-8.0631 * Math.PI) / 180)));
assert.equal(sizing(merc).pxPerMetreAtZ0, 1 / 78271.517);
assert.equal(sizing(cases.empty).textZHi, 24);
assert.deepEqual(bounds(cases.empty), null);

// ---- attach on a fake map ----------------------------------------------------
function fakeMap({ loaded }) {
  const calls = [];
  const layersById = new Map();
  const sources = new Map();
  const listeners = {};
  return {
    calls,
    fire(ev) { const l = listeners[ev]; delete listeners[ev]; l?.(); },
    isStyleLoaded: () => loaded,
    once(ev, fn) { calls.push(['once', ev]); listeners[ev] = fn; },
    off(ev) { calls.push(['off', ev]); delete listeners[ev]; },
    addSource(id, src) { calls.push(['addSource', id, src.type]); sources.set(id, { setData: (d) => calls.push(['setData', d.features.length]) }); },
    getSource: (id) => sources.get(id),
    removeSource(id) { calls.push(['removeSource', id]); sources.delete(id); },
    addLayer(l) { calls.push(['addLayer', l.id]); layersById.set(l.id, l); },
    getLayer: (id) => layersById.get(id),
    removeLayer(id) { calls.push(['removeLayer', id]); layersById.delete(id); },
    setFilter(id) { calls.push(['setFilter', id]); },
    setPaintProperty(id, k, v) { calls.push(['setPaint', id, k, v]); },
    setLayoutProperty(id, k, v) { calls.push(['setLayout', id, k, v]); },
    fitBounds(b, o) { calls.push(['fitBounds', b, o]); },
  };
}
const ops = (m, op) => m.calls.filter((c) => c[0] === op);

// before the style loads: nothing is drawn, the load event does it
const callout = cases.callout;
const engine = maplibreLayers(callout);
let m = fakeMap({ loaded: false });
let h = attach(m, callout, { layers: engine, fit: true });
assert.deepEqual(m.calls, [['once', 'load']]);
h.setGeoJSON(cases.radius); // before load: picked up by the load handler
m.fire('load');
assert.deepEqual(ops(m, 'addSource'), [['addSource', SOURCE, 'geojson']]);
assert.deepEqual(ops(m, 'addLayer').map((c) => c[1]), LAYER_IDS);
assert.deepEqual(ops(m, 'fitBounds')[0][1], bounds(cases.radius));
assert.deepEqual(ops(m, 'fitBounds')[0][2], { padding: 48, maxZoom: 19, duration: 0 });

// already loaded: drawn at once; setGeoJSON re-uses the source and layers
m = fakeMap({ loaded: true });
h = attach(m, callout, { layers: engine });
assert.equal(ops(m, 'addLayer').length, 6);
assert.equal(ops(m, 'fitBounds').length, 0);
m.calls.length = 0;
h.setGeoJSON(cases.radius); // no layers: the port computes them
assert.deepEqual(ops(m, 'setData'), [['setData', cases.radius.features.length]]);
assert.equal(ops(m, 'addLayer').length + ops(m, 'addSource').length, 0);
const lw = ops(m, 'setPaint').find((c) => c[1] === 'pic-stroke' && c[2] === 'line-width');
close(lw[3], layers(cases.radius)[1].paint['line-width'], 'updated line-width');
assert.equal(ops(m, 'setFilter').length, 6);
h.fitTo();
assert.deepEqual(ops(m, 'fitBounds')[0][1], bounds(cases.radius));
h.detach();
assert.deepEqual(ops(m, 'removeLayer').map((c) => c[1]), [...LAYER_IDS].reverse());
assert.deepEqual(ops(m, 'removeSource'), [['removeSource', SOURCE]]);
h.detach(); // idempotent
assert.equal(ops(m, 'removeSource').length, 1);

// detached before load: the pending load does nothing
m = fakeMap({ loaded: false });
attach(m, callout).detach();
m.fire('load');
assert.equal(ops(m, 'addSource').length, 0);

// ---- createGeoMap with a fake maplibre-gl namespace -------------------------
let made;
const fakeGl = {
  Map: class {
    constructor(o) {
      made = { opts: o, map: fakeMap({ loaded: false }), controls: [], removed: false };
      return Object.assign(made.map, {
        addControl: (c, pos) => made.controls.push([c, pos]),
        remove: () => { made.removed = true; },
      });
    }
  },
  NavigationControl: class { constructor(o) { this.o = o; } },
};
const g = createGeoMap(fakeGl, 'el');
assert.equal(made.opts.container, 'el');
assert.equal(made.opts.style, 'https://tiles.openfreemap.org/styles/positron');
assert.equal(made.opts.cooperativeGestures, true);
assert.deepEqual(made.controls.map((c) => [c[0].o, c[1]]), [[{ showCompass: false }, 'top-right']]);
g.setGeoJSON(callout, engine);
g.fitTo(callout);
made.map.fire('load');
assert.equal(ops(made.map, 'addLayer').length, 6);
assert.equal(ops(made.map, 'fitBounds').length, 1);
g.destroy();
assert.ok(made.removed);
assert.equal(ops(made.map, 'removeSource').length, 1);
createGeoMap(fakeGl, 'el2', { style: 'x', cooperativeGestures: false, navigation: false });
assert.equal(made.opts.style, 'x');
assert.equal(made.opts.cooperativeGestures, false);
assert.equal(made.controls.length, 0);

console.log('smoke-maplibre: ok');
