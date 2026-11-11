// The MapLibre module (maplibre.js): its JS port of the layers equals the
// engine's (geopic_core::maplibre through the geo wasm) and the pre-move
// snapshots in examples/geo/maplibre/; `attach`/`createGeoMap` drive a fake
// map that records every call. Runs in its own process (ready() fixes the
// build per module instance).
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { attach, bounds, checkLayerOptions, createGeoMap, FONTS, LAYER_IDS, layers, MAX_TEXT_SCALE, sizing, SOURCE, textFont } from '../maplibre.js';
import { compileGeoJSON, maplibreLayers, ready } from '../index.js';

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
  // G11: the default scale is the default output; 1.5 agrees port vs engine
  assert.deepEqual(layers(fc, { textScale: 1 }), port, `${name} textScale 1`);
  assert.deepEqual(maplibreLayers(fc, { textScale: 1 }), maplibreLayers(fc), `${name} engine textScale 1`);
  close(layers(fc, { textScale: 1.5 }), maplibreLayers(fc, { textScale: 1.5 }), `${name} textScale 1.5 vs engine`);
}

// textScale: the font metres factor scales, the text stop follows the scaled
// font (one zoom lower at ×2), nothing else moves
{
  const fc = cases.callout;
  const [b, x2] = [layers(fc), layers(fc, { textScale: 2 })];
  assert.equal(sizing(fc, { textScale: 2 }).textZHi, sizing(fc).textZHi - 1);
  assert.equal(sizing(fc, { textScale: 2 }).textScale, 2);
  assert.equal(sizing(fc).textScale, 1);
  for (const id of ['pic-text', 'pic-title']) {
    const [tb, t2] = [b, x2].map((ls) => ls.find((l) => l.id === id).layout['text-size']);
    assert.equal(t2[4][2][1][2], 2 * tb[4][2][1][2], id);
    assert.equal(t2[5], tb[5] - 1, id);
  }
  for (let i = 0; i < 4; i++) assert.deepEqual(x2[i], b[i], LAYER_IDS[i]);
  assert.equal(MAX_TEXT_SCALE, 16);
  assert.ok(layers(fc, { textScale: 16 }));
  for (const k of [0, -1, NaN, Infinity, 17, '2']) {
    assert.throws(() => layers(fc, { textScale: k }), /textScale/, `port ${k}`);
    assert.throws(() => maplibreLayers(fc, { textScale: k }), /textScale/, `engine ${k}`);
  }
  assert.throws(() => maplibreLayers(fc, { text_scale: 2 }), /unknown layer option/);
  // compileGeoJSON: `layers: { textScale }` = maplibreLayers on its collection
  const src = readFileSync(new URL('callout.pic', geoDir), 'utf8');
  const plain = compileGeoJSON(src, { circuits: true, layers: true });
  const big = compileGeoJSON(src, { circuits: true, layers: { textScale: 1.5 } });
  assert.deepEqual(big.geojson, plain.geojson);
  assert.deepEqual(plain.layers, maplibreLayers(plain.geojson));
  assert.deepEqual(big.layers, maplibreLayers(big.geojson, { textScale: 1.5 }));
  assert.deepEqual(compileGeoJSON(src, { circuits: true, layers: {} }).layers, plain.layers);
  assert.equal(compileGeoJSON(src, { circuits: true }).layers, undefined);
  assert.throws(() => compileGeoJSON(src, { layers: { textScale: NaN } }), /textScale/);
  // one validator, before the wasm runs: a plain Error naming textScale
  assert.throws(() => compileGeoJSON(src, { layers: { textScale: 20 } }), (e) => !e.info && /textScale/.test(e.message));
  assert.throws(() => compileGeoJSON(src, { layers: [] }), /must be an object/);
  assert.deepEqual(checkLayerOptions(), { textScale: 1 });
  assert.deepEqual(checkLayerOptions({ textScale: 2 }), { textScale: 2 });
  assert.throws(() => layers(fc, { foo: 1 }), /unknown layer option/);
  assert.throws(() => compileGeoJSON(src, { layers: { before: 'labels' } }), /unknown layer option/);
}

// pic-title draws the title Points (#435), styled exactly as pic-text
{
  const ls = layers(cases.callout);
  const [text, title] = ['pic-text', 'pic-title'].map((id) => ls.find((l) => l.id === id));
  assert.deepEqual(title.filter, ['==', ['get', 'geopic:kind'], 'title']);
  assert.deepEqual(title.layout, text.layout);
  assert.deepEqual(title.paint, text.paint);
  // G10 addendum: a trailing blank row is padded with a no-break space
  // (MapLibre drops a final "\n"); the data stays pure
  const t = ['get', 'geopic:text'];
  assert.deepEqual(text.layout['text-field'],
    ['case', ['==', ['slice', t, ['max', 0, ['-', ['length', t], 1]]], '\n'], ['concat', t, '\u00a0'], t]);
  // pic's line skip is the font size, not MapLibre's default 1.2 em
  assert.equal(text.layout['text-line-height'], 1);
  assert.equal(cases.callout.features.filter((f) => f.properties['geopic:kind'] === 'title').length, 1);
}

// sizing: 400 m/in at lat −8.0631 → p₀ = 1/(78271.517·cos φ), 11 pt → stop 18
const s = sizing(cases.callout);
assert.equal(s.mPerIn, 400);
assert.equal(s.pxPerMetreAtZ0, 1 / (78271.517 * Math.cos((-8.0631 * Math.PI) / 180)));
assert.equal(sizing(merc).pxPerMetreAtZ0, 1 / 78271.517);
assert.equal(sizing(cases.empty).textZHi, 24);
assert.deepEqual(bounds(cases.empty), null);

// ---- attach on a fake map ----------------------------------------------------
function fakeMap({ loaded, style }) {
  const calls = [];
  const layersById = new Map((style ?? []).map((l) => [l.id, l]));
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
    addLayer(l, before) {
      calls.push(['addLayer', l.id, before]);
      const all = [...layersById.values()];
      const at = before === undefined ? all.length : all.findIndex((x) => x.id === before);
      assert.ok(at >= 0, `addLayer before a missing layer ${before}`);
      all.splice(at, 0, l);
      layersById.clear();
      for (const x of all) layersById.set(x.id, x);
    },
    getLayer: (id) => layersById.get(id),
    getStyle: () => ({ layers: [...layersById.values()].map(({ id, type, layout }) => ({ id, type, ...(layout ? { layout } : {}) })) }),
    order: () => [...layersById.keys()],
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
assert.deepEqual(m.calls, [['once', 'load'], ['once', 'idle']]);
h.setGeoJSON(cases.radius); // before load: picked up by the load handler
h.fitTo(cases.callout); // a pending fit keeps its own target
m.fire('load');
m.fire('idle'); // a second start is a no-op
assert.deepEqual(ops(m, 'addSource'), [['addSource', SOURCE, 'geojson']]);
assert.deepEqual(ops(m, 'addLayer').map((c) => c[1]), LAYER_IDS);
assert.deepEqual(ops(m, 'fitBounds'), [ops(m, 'fitBounds')[0]]); // once, not twice
assert.deepEqual(ops(m, 'fitBounds')[0][1], bounds(cases.callout));
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
assert.equal(ops(m, 'setFilter').length, 0); // only the sizes move on an update
assert.deepEqual(ops(m, 'setLayout').map((c) => c[2]), ['text-size', 'text-size']);
h.fitTo();
assert.deepEqual(ops(m, 'fitBounds')[0][1], bounds(cases.radius));
h.detach();
assert.deepEqual(ops(m, 'removeLayer').map((c) => c[1]), [...LAYER_IDS].reverse());
assert.deepEqual(ops(m, 'removeSource'), [['removeSource', SOURCE]]);
h.detach(); // idempotent
assert.equal(ops(m, 'removeSource').length, 1);

// a map that is up but fetching tiles: isStyleLoaded() is false, `load` is
// long gone — `idle` draws it
m = fakeMap({ loaded: false });
h = attach(m, callout, { layers: engine, fit: true });
m.fire('idle');
assert.equal(ops(m, 'addLayer').length, 6);
assert.equal(ops(m, 'fitBounds').length, 1);

// detached before load: the pending load does nothing
m = fakeMap({ loaded: false });
attach(m, callout).detach();
assert.deepEqual(ops(m, 'off').map((c) => c[1]), ['load', 'idle']);
m.fire('load');
m.fire('idle');
assert.equal(ops(m, 'addSource').length, 0);

// ---- before: under the basemap's labels, or under a given layer --------------
const basemap = [
  { id: 'background', type: 'background' },
  { id: 'water', type: 'fill' },
  { id: 'roads', type: 'line' },
  { id: 'road-names', type: 'symbol' },
  { id: 'place-names', type: 'symbol' },
];
// only the geometry goes under; the drawing's own labels stay on top
const GEOM = LAYER_IDS.slice(0, 4);
const TEXT = LAYER_IDS.slice(4);
assert.deepEqual(TEXT, ['pic-text', 'pic-title']);
const split = (id) => [...GEOM.map(() => id), ...TEXT.map(() => undefined)];
m = fakeMap({ loaded: true, style: basemap });
h = attach(m, callout, { layers: engine, before: 'labels' });
assert.deepEqual(ops(m, 'addLayer').map((c) => c[2]), split('road-names'));
const under = ['background', 'water', 'roads', ...GEOM, 'road-names', 'place-names', ...TEXT];
assert.deepEqual(m.order(), under);
// an update moves nothing; detach + re-attach resolves again (our own
// symbol layers never count as "the labels")
h.setGeoJSON(cases.radius);
assert.equal(ops(m, 'addLayer').length, 6);
h.detach();
h = attach(m, callout, { before: 'labels' });
assert.deepEqual(m.order(), under);
h.detach();
// 'labels' names a real layer when the style has one with that id
m = fakeMap({ loaded: true, style: [...basemap.slice(0, 3), { id: 'labels', type: 'fill' }, ...basemap.slice(3)] });
attach(m, callout, { before: 'labels' });
assert.deepEqual(m.order(), ['background', 'water', 'roads', ...GEOM, 'labels', 'road-names', 'place-names', ...TEXT]);
// …otherwise it is the start of the TRAILING symbol block (above: road-names),
// not the first symbol layer: a symbol amid the geometry (CARTO positron's
// waterway_label at 13, with roads and buildings up to 67) would bury the
// shapes under the roads and buildings that follow it
const carto = [
  { id: 'bg', type: 'background' },
  { id: 'water', type: 'fill' },
  { id: 'waterway_label', type: 'symbol' },
  { id: 'road', type: 'line' },
  { id: 'building', type: 'fill' },
  { id: 'road_shield', type: 'symbol' },
  { id: 'place_city', type: 'symbol' },
];
m = fakeMap({ loaded: true, style: carto });
attach(m, callout, { layers: engine, before: 'labels' });
assert.deepEqual(ops(m, 'addLayer').map((c) => c[2]), split('road_shield'));
assert.deepEqual(m.order(), [...carto.slice(0, 5).map((l) => l.id), ...GEOM, 'road_shield', 'place_city', ...TEXT]);
// interleaved symbols; the last geometry layer is followed by place_city only
const interleaved = [
  { id: 'bg', type: 'background' },
  { id: 'waterway_label', type: 'symbol' },
  { id: 'road', type: 'line' },
  { id: 'road_shield', type: 'symbol' },
  { id: 'building', type: 'fill-extrusion' },
  { id: 'place_city', type: 'symbol' },
];
m = fakeMap({ loaded: true, style: interleaved });
attach(m, callout, { before: 'labels' });
assert.deepEqual(ops(m, 'addLayer').map((c) => c[2]), split('place_city'));
assert.deepEqual(m.order(), [...interleaved.slice(0, 5).map((l) => l.id), ...GEOM, 'place_city', ...TEXT]);
// symbols only amid the geometry, none after it: on top
const noTrailing = [
  { id: 'bg', type: 'background' },
  { id: 'waterway_label', type: 'symbol' },
  { id: 'road', type: 'line' },
];
m = fakeMap({ loaded: true, style: noTrailing });
attach(m, callout, { before: 'labels' });
assert.deepEqual(ops(m, 'addLayer').map((c) => c[2]), LAYER_IDS.map(() => undefined));
assert.deepEqual(m.order(), [...noTrailing.map((l) => l.id), ...LAYER_IDS]);
// a layer id
m = fakeMap({ loaded: true, style: basemap });
attach(m, callout, { before: 'roads' });
assert.deepEqual(m.order(), ['background', 'water', ...GEOM, 'roads', 'road-names', 'place-names', ...TEXT]);
// a style without symbol layers, or a missing id: on top
m = fakeMap({ loaded: true, style: basemap.slice(0, 3) });
attach(m, callout, { before: 'labels' });
assert.deepEqual(m.order(), ['background', 'water', 'roads', ...LAYER_IDS]);
{
  const warn = console.warn;
  const warned = [];
  console.warn = (msg) => warned.push(msg);
  try {
    m = fakeMap({ loaded: true, style: basemap });
    attach(m, callout, { before: 'nope' });
  } finally {
    console.warn = warn;
  }
  assert.deepEqual(m.order(), [...basemap.map((l) => l.id), ...LAYER_IDS]);
  assert.equal(warned.length, 1);
  assert.match(warned[0], /nope/);
}
// resolved at apply time: a map whose style loads later
m = fakeMap({ loaded: false, style: basemap });
attach(m, callout, { before: 'labels' });
m.fire('load');
assert.deepEqual(ops(m, 'addLayer').map((c) => c[2]), split('road-names'));
// no `before`: appended, beforeId undefined (the previous behaviour)
m = fakeMap({ loaded: true, style: basemap });
attach(m, callout);
assert.deepEqual(ops(m, 'addLayer').map((c) => c[2]), LAYER_IDS.map(() => undefined));

// ---- font: the drawing's labels follow the style's glyphs -------------------
// CARTO's glyph server serves a Noto with other vertical metrics: the spec's
// placeholder font put a label's box centred on its line there
const NOTO = ['Noto Sans Regular'];
const MONT = ['Montserrat Regular', 'Open Sans Regular', 'Noto Sans Regular'];
const MONT_B = ['Montserrat Bold', 'Open Sans Bold', 'Noto Sans Regular'];
const OPEN = ['Open Sans Regular'];
const sym = (id, f) => ({ id, type: 'symbol', layout: { 'text-font': f } });
const cartoFonts = [
  { id: 'bg', type: 'background' },
  { id: 'road', type: 'line', layout: { 'line-cap': 'round' } },
  sym('waterway_label', MONT),
  sym('place_city', MONT),
  sym('place_country', MONT_B),
  sym('roadname', ['literal', OPEN]), // an expression wrapping a plain stack counts
  sym('poi', ['step', ['zoom'], ['literal', OPEN], 12, ['literal', MONT_B]]), // not a plain stack: skipped
  { id: 'shield', type: 'symbol', layout: {} }, // no text-font: skipped
];
const fontOf = (map, id) => map.getLayer(id).layout['text-font'];
const engineCopy = JSON.stringify(engine);
// the specs choose a face from geopic:font_weight / geopic:font_style (G12)
const SPEC_FONT = textFont();
assert.deepEqual(engine.find((l) => l.id === 'pic-text').layout['text-font'], SPEC_FONT);
assert.deepEqual(SPEC_FONT.at(-1), ['literal', NOTO]);
assert.deepEqual(FONTS.boldItalic, FONTS.bold); // the reference glyph set has no bold italic
/** `text-font` for these faces, as attach builds it */
const faces = (regular, bold = regular, italic = regular, boldItalic = bold) => textFont({ regular, bold, italic, boldItalic });
// default 'style': the most common stack, on both label layers only
m = fakeMap({ loaded: true, style: cartoFonts });
h = attach(m, callout, { layers: engine, before: 'labels' });
// … with the style's own Bold of the same family for bold labels; it has no
// italic Montserrat, so italic labels keep the regular face
const MONT_FACES = faces(MONT, MONT_B);
assert.deepEqual(fontOf(m, 'pic-text'), MONT_FACES);
assert.deepEqual(fontOf(m, 'pic-title'), MONT_FACES);
assert.deepEqual(MONT_FACES[2], ['literal', MONT_B]); // bold italic → the bold
assert.deepEqual(MONT_FACES[6], ['literal', MONT]); // italic → regular
assert.equal(JSON.stringify(engine), engineCopy); // the caller's specs are copied, never changed
// an update keeps the font (only the sizes move) and needs no new resolution
m.calls.length = 0;
h.setGeoJSON(cases.radius);
assert.deepEqual(ops(m, 'setLayout').map((c) => c[2]), ['text-size', 'text-size']);
assert.deepEqual(fontOf(m, 'pic-text'), MONT_FACES);
// the port's layers get the same treatment
m = fakeMap({ loaded: true, style: cartoFonts });
attach(m, callout);
assert.deepEqual(fontOf(m, 'pic-title'), MONT_FACES);
// a plain face (Regular/Medium, not Italic) wins even over a more common Bold
// stack — CARTO positron's most common stack is its SemiBold place names
m = fakeMap({ loaded: true, style: [sym('a', ['Roboto Bold']), sym('b', ['Roboto Italic']), sym('c', ['Roboto Medium']), sym('d', ['Roboto Bold'])] });
attach(m, callout, { layers: engine });
assert.deepEqual(fontOf(m, 'pic-text'), faces(['Roboto Medium'], ['Roboto Bold'], ['Roboto Italic'], ['Roboto Bold'])); // plain first, then count
m = fakeMap({ loaded: true, style: [sym('a', ['Roboto Regular']), sym('b', ['Roboto Medium']), sym('c', ['Roboto Medium'])] });
attach(m, callout, { layers: engine });
assert.deepEqual(fontOf(m, 'pic-text'), ['Roboto Medium']); // most common among the plain ones
m = fakeMap({ loaded: true, style: [sym('a', ['Roboto Bold']), sym('b', ['Roboto Italic']), sym('c', ['Roboto Bold'])] });
attach(m, callout, { layers: engine });
// no plain stack: most common of all — and the faces still come from the family
assert.deepEqual(fontOf(m, 'pic-text'), faces(['Roboto Bold'], ['Roboto Bold'], ['Roboto Italic'], ['Roboto Bold']));
m = fakeMap({ loaded: true, style: [sym('a', ['Roboto Regular Italic']), sym('b', ['Roboto Bold'])] });
attach(m, callout, { layers: engine });
// Italic is not plain; tie → first seen
assert.deepEqual(fontOf(m, 'pic-text'), faces(['Roboto Regular Italic'], ['Roboto Bold'], ['Roboto Regular Italic'], ['Roboto Bold']));
// the faces of the family: Bold before SemiBold before Black, an italic of
// plain weight (not Light Italic), a real bold italic when the style has one;
// another family's Bold is never borrowed
m = fakeMap({ loaded: true, style: [
  sym('a', ['Montserrat Medium']), sym('b', ['Montserrat SemiBold']), sym('c', ['Montserrat SemiBold']),
  sym('d', ['Montserrat Black']), sym('e', ['Montserrat Light Italic']), sym('f', ['Montserrat Medium Italic']),
  sym('g', ['Montserrat SemiBold Italic']), sym('h', ['Open Sans Bold']),
] });
attach(m, callout, { layers: engine });
assert.deepEqual(fontOf(m, 'pic-text'), faces(['Montserrat Medium'], ['Montserrat SemiBold'], ['Montserrat Medium Italic'], ['Montserrat SemiBold Italic']));
m = fakeMap({ loaded: true, style: [sym('a', ['Noto Sans Regular']), sym('b', ['Noto Sans SemiBold']), sym('c', ['Noto Sans SemiBold']), sym('d', ['Noto Sans Bold']), sym('e', ['Noto Sans Italic'])] });
attach(m, callout, { layers: engine });
assert.deepEqual(fontOf(m, 'pic-text'), faces(NOTO, ['Noto Sans Bold'], ['Noto Sans Italic'], ['Noto Sans Bold']));
// a style with one face only: the plain stack, no expression
m = fakeMap({ loaded: true, style: [sym('a', OPEN), sym('b', MONT_B)] });
attach(m, callout, { layers: engine });
assert.deepEqual(fontOf(m, 'pic-text'), OPEN);
// 'spec' keeps the engine's Noto
m = fakeMap({ loaded: true, style: cartoFonts });
attach(m, callout, { layers: engine, font: 'spec' });
assert.deepEqual(fontOf(m, 'pic-text'), SPEC_FONT);
assert.deepEqual(fontOf(m, 'pic-title'), SPEC_FONT);
// an explicit stack wins
m = fakeMap({ loaded: true, style: cartoFonts });
attach(m, callout, { layers: engine, font: ['Inter Regular'] });
assert.deepEqual(fontOf(m, 'pic-text'), ['Inter Regular']);
assert.deepEqual(fontOf(m, 'pic-title'), ['Inter Regular']);
// explicit faces: missing ones fall back (bold italic → bold → italic → regular)
m = fakeMap({ loaded: true, style: cartoFonts });
attach(m, callout, { layers: engine, font: { regular: ['Inter Regular'], bold: ['Inter Bold'] } });
assert.deepEqual(fontOf(m, 'pic-text'), faces(['Inter Regular'], ['Inter Bold']));
m = fakeMap({ loaded: true, style: cartoFonts });
attach(m, callout, { layers: engine, font: { regular: ['Inter Regular'], italic: ['Inter Italic'] } });
assert.deepEqual(fontOf(m, 'pic-title'), faces(['Inter Regular'], ['Inter Regular'], ['Inter Italic'], ['Inter Italic']));
// no symbol layer with a text-font: the spec's
m = fakeMap({ loaded: true, style: basemap });
attach(m, callout, { layers: engine });
assert.deepEqual(fontOf(m, 'pic-text'), SPEC_FONT);
m = fakeMap({ loaded: true });
attach(m, callout, { layers: engine });
assert.deepEqual(fontOf(m, 'pic-title'), SPEC_FONT);
// a bad value throws up front
assert.throws(() => attach(fakeMap({ loaded: true }), callout, { font: 'Montserrat' }), /font must be/);
assert.throws(() => attach(fakeMap({ loaded: true }), callout, { font: [] }), /font must be/);
assert.throws(() => attach(fakeMap({ loaded: true }), callout, { font: { bold: ['Inter Bold'] } }), /font must be/);
assert.throws(() => attach(fakeMap({ loaded: true }), callout, { font: { regular: ['Inter'], heavy: ['Inter Black'] } }), /font must be/);
assert.throws(() => attach(fakeMap({ loaded: true }), callout, { font: { regular: ['Inter'], bold: 'Inter Bold' } }), /font must be/);
assert.throws(() => attach(fakeMap({ loaded: true }), callout, { font: null ?? 0 }), /font must be/);

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
// `font` is forwarded to attach
{
  const g3 = createGeoMap(fakeGl, 'el4', { font: ['Inter Regular'] });
  g3.setGeoJSON(callout, engine);
  made.map.fire('load');
  assert.deepEqual(made.map.getLayer('pic-text').layout['text-font'], ['Inter Regular']);
  g3.destroy();
}
assert.equal(ops(made.map, 'removeSource').length, 1);
createGeoMap(fakeGl, 'el2', { style: 'x', cooperativeGestures: false, navigation: false });
assert.equal(made.opts.style, 'x');
assert.equal(made.opts.cooperativeGestures, false);
assert.equal(made.controls.length, 0);
// `before` is forwarded to attach
{
  const g2 = createGeoMap(fakeGl, 'el3', { before: 'labels' });
  for (const l of basemap) made.map.addLayer(l);
  made.map.calls.length = 0;
  g2.setGeoJSON(callout, engine);
  made.map.fire('load');
  assert.deepEqual(ops(made.map, 'addLayer').map((c) => c[2]), split('road-names'));
}

console.log('smoke-maplibre: ok');
