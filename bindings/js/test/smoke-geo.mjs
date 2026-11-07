// Smoke test for the geo-enabled build: compileGeoJSON / geoToCanvas
// (docs/geopic.md), mirroring bindings/python/tests/test_geo.py. Runs in its
// own process — ready() fixes the build choice per module instance.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { compile, compileGeoJSON, GeoError, geoToCanvas, ready, renderSvg } from '../index.js';

const close = (a, b, tol = 1e-6) => Math.abs(a - b) <= tol;
const kinds = (fc) => fc.features.map((f) => f.properties['geopic:kind']);
const ofKind = (fc, k) => fc.features.filter((f) => f.properties['geopic:kind'] === k);

// before ready(): a clear error, not a crash
assert.throws(() => compileGeoJSON('box'), /call `await ready\(\)`/);
await assert.rejects(() => ready(undefined, { math: true, geo: true }), /cannot load the math and geo builds together/);

const wasm = readFileSync(new URL('../pkg/rpic_wasm_geo_bg.wasm', import.meta.url));
await ready(wasm, { geo: true });
await assert.rejects(() => ready(wasm), /already initialized the geo-enabled build; cannot switch to lean/);
await assert.rejects(() => ready(wasm, { math: true }), /already initialized the geo-enabled build/);

// the plain compiler still works on the geo build
assert.match(renderSvg('box "hi"'), /hi/);

// declared anchor → a FeatureCollection in WGS84 with the expected kinds
const SRC = 'geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 400\nB: box "hub" at (1, 0.5)\narrow from (0,0) to B.sw';
const out = compileGeoJSON(SRC);
assert.deepEqual(Object.keys(out).sort(), ['canvas', 'diagnostics', 'geojson', 'registration', 'warnings']);
assert.equal(out.geojson.type, 'FeatureCollection');
for (const k of ['box', 'arrow', 'arrowhead']) assert.ok(kinds(out.geojson).includes(k), k);
for (const f of out.geojson.features) assert.equal(f.properties['geopic:schema'], 1);
const [lon0, lat0] = ofKind(out.geojson, 'box')[0].geometry.coordinates[0][0];
assert.ok(lon0 > -34.9 && lon0 < -34.8 && lat0 > -8.1 && lat0 < -8.0);
assert.equal(out.registration.method, 'anchor');
assert.equal(out.registration.frame, 'enu');
assert.deepEqual(out.canvas, { rescale: 1, scale: 1 });
assert.deepEqual(out.warnings, []);

// overrides, mercator frame
const merc = compileGeoJSON(SRC, { anchor: [-46.6333, -23.5505], scale: 50, bearing: 90, frame: 'mercator' }).registration;
assert.deepEqual(merc.anchor, { lon: -46.6333, lat: -23.5505 });
assert.equal(merc.scale_m_per_in, 50);
assert.equal(merc.bearing_deg, 90);
assert.equal(merc.frame, 'mercator');
assert.throws(() => compileGeoJSON(SRC, { frame: 'utm' }), (e) => e instanceof GeoError && e.info.kind === 'invalid_frame');

// two control points the source defines → two_points fit
const two = compileGeoJSON('Recife: box at (2, 1)\nOlinda: box at (5, 3)\narrow from Recife to Olinda', {
  features: { Recife: [-34.8711, -8.0631], Olinda: [-34.8553, -8.0089] },
});
assert.equal(two.registration.method, 'two_points');
assert.ok(two.registration.rms_m < 1e-6);
// …and the source's own `<Label>_lon/_lat` control points, no host features
const own = compileGeoJSON('A: (0, 0)\nB: (3, 0)\nA_lon = -34.8711; A_lat = -8.0631\nB_lon = -34.8553; B_lat = -8.0089\nline from A to B');
assert.equal(own.registration.method, 'two_points');
assert.equal(own.registration.control_points, 2);

// feature injection: the source never defines Recife or Olinda
const inj = compileGeoJSON(
  'geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 1000\narrow from Recife to Olinda\n"R" at Recife',
  { features: [{ name: 'Recife', lon: -34.8711, lat: -8.0631 }, ['Olinda', -34.8553, -8.0089]] }
);
assert.equal(inj.registration.method, 'two_points');
const arrow = ofKind(inj.geojson, 'arrow')[0].geometry.coordinates;
assert.ok(close(arrow[0][0], -34.8711) && close(arrow[0][1], -8.0631), JSON.stringify(arrow[0]));
const tip = ofKind(inj.geojson, 'arrowhead')[0].geometry.coordinates[0];
assert.ok(tip.some((p) => close(p[0], -34.8553) && close(p[1], -8.0089)), JSON.stringify(tip));
assert.ok(!inj.warnings.some((w) => w.kind === 'ignored_with_control_points'), JSON.stringify(inj.warnings));

// a bbox feature: `Zone.n` is the northern edge
const zone = compileGeoJSON('geo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 1000\ndot at Zone.c\ndot at Zone.n', {
  features: { Zone: { lon: -34.86, lat: -8.05, bbox: [-34.87, -8.06, -34.85, -8.04] } },
});
const dots = ofKind(zone.geojson, 'dot').map((f) => f.geometry.coordinates);
assert.ok(close(dots[0][1], -8.05, 1e-4) && close(dots[1][1], -8.04, 1e-4), JSON.stringify(dots));

// injected places honour pic `scale` and the whole-picture rescale
const sc = compileGeoJSON(
  '.PS 2\nscale = 10\ngeo_lon = -34.8711\ngeo_lat = -8.0631\ngeo_scale = 1000\nline from (0,0) to (40, 0)\ndot at Olinda\n.PE',
  { features: { Olinda: [-34.8553, -8.0089] } }
);
assert.equal(sc.canvas.scale, 10);
assert.ok(sc.canvas.rescale < 1);
const [dlon, dlat] = ofKind(sc.geojson, 'dot')[0].geometry.coordinates;
assert.ok(close(dlon, -34.8553) && close(dlat, -8.0089), `${dlon}, ${dlat}`);

// geoToCanvas inverts the registration: the anchor sits at the origin
const [[ox, oy]] = geoToCanvas(out.registration, [[-34.8711, -8.0631]]);
assert.ok(close(ox, out.registration.origin[0]) && close(oy, out.registration.origin[1]));
assert.throws(() => geoToCanvas({ nope: 1 }, [[0, 0]]), (e) => e instanceof GeoError && e.info.kind === 'invalid_options');

// no registration → GeoError with structured info
assert.throws(
  () => compileGeoJSON('box'),
  (e) => e instanceof GeoError && e.info.kind === 'no_registration' && /geo_lon/.test(e.info.hint)
);

// a pic error is the usual compile error (errorInfo), lines relative to the
// caller's source even after feature injection
assert.throws(
  () => compileGeoJSON('geo_lon = 0\ngeo_lat = 0\nbox )\n', { features: { A: [0, 0], B: [0.001, 0] } }),
  (e) => !(e instanceof GeoError) && e.errorInfo?.line === 3
);
const grad = compileGeoJSON('geo_lon = 0\ngeo_lat = 0\nbox gradient "red" "blue"\ndot at A', { features: { A: [0.001, 0] } });
assert.equal(grad.warnings.find((w) => w.kind === 'unsupported_style').span.line, 3);

// feature names are validated before anything compiles — never interpolated
const plain = 'geo_lon = 0\ngeo_lat = 0\nbox';
for (const bad of ['A; copy "/etc/passwd"', 'lower', '1A', 'A B', 'A\nbox', '']) {
  assert.throws(() => compileGeoJSON(plain, { features: { [bad]: [0, 0] } }), /not a pic label/, bad);
}
assert.throws(() => compileGeoJSON(plain, { features: { A: [NaN, 0] } }), /finite/);
assert.throws(() => compileGeoJSON(plain, { features: [['A', 0, 0, [1, 2, 3]]] }), /bbox/);
for (const form of ['Recife', [['A', 1]], [['A', 1, 2, null, 5]], [42]]) {
  assert.throws(() => compileGeoJSON(plain, { features: form }), /features must be/);
}
const marco = compileGeoJSON(plain, { features: { Marco_Zero_1: [0, 0] } });
assert.equal(marco.registration.control_points, 1);
assert.deepEqual(marco.warnings, []);

// animations cannot ride into GeoJSON — warned, never silently dropped
const anim = compileGeoJSON('geo_lon = 0\ngeo_lat = 0\nbox\nanimate last box with "fade"');
assert.ok(anim.warnings.some((w) => w.kind === 'ignored' && /animation/.test(w.message)), JSON.stringify(anim.warnings));

// bad option shapes are geo errors, not crashes
assert.throws(() => compileGeoJSON(plain, { screenClasses: 'screen' }), (e) => e instanceof GeoError && e.info.kind === 'invalid_options');
assert.throws(() => compileGeoJSON('geo_lon = 0\ngeo_lat = 95\nbox'), (e) => e instanceof GeoError && e.info.kind === 'invalid_anchor');

// engine warnings and print lines ride along (review #427, finding 1)
const eng = compileGeoJSON('geo_lon = 0\ngeo_lat = 0\nbox color "nocolour"\nprint "hi"');
assert.deepEqual(eng.diagnostics, ['hi']);
assert.ok(eng.warnings.some((w) => w.kind === 'invalid_color' && w.line === 3), JSON.stringify(eng.warnings));
assert.deepEqual(out.diagnostics, []);
// …with caller line numbers after feature injection, engine warnings first
const engInj = compileGeoJSON('geo_lon = 0\ngeo_lat = 0\nbox color "nocolour" gradient "red" "blue"\ndot at A', {
  features: { A: [0.001, 0] },
});
const ic = engInj.warnings.findIndex((w) => w.kind === 'invalid_color');
const us = engInj.warnings.findIndex((w) => w.kind === 'unsupported_style');
assert.ok(ic >= 0 && us > ic, JSON.stringify(engInj.warnings));
assert.equal(engInj.warnings[ic].line, 3);
// texlabels without a math renderer: the notice is a diagnostic, not lost
const tex = compileGeoJSON('geo_lon = 0\ngeo_lat = 0\nbox "$x^2$"', { texlabels: true });
assert.ok(tex.diagnostics.some((d) => /math renderer/.test(d)), JSON.stringify(tex.diagnostics));

// numeric options are validated, never silently dropped (finding 3)
for (const [o, re] of [
  [{ scale: NaN }, /scale must be a positive finite number/],
  [{ scale: 0 }, /scale must be/],
  [{ flatten: Infinity }, /flatten must be/],
  [{ viewScale: -1 }, /viewScale must be/],
  [{ bearing: NaN }, /bearing must be a finite number/],
  [{ anchor: [0, NaN] }, /anchor must be \[lon, lat\]/],
  [{ anchor: [0] }, /anchor must be/],
  [{ anchor: 'here' }, /anchor must be/],
]) {
  assert.throws(() => compileGeoJSON(plain, o), re, JSON.stringify(o));
}

// a feature object without a name, and duplicate names (finding 5)
assert.throws(() => compileGeoJSON(plain, { features: [{ lon: 0, lat: 0 }] }), /features must be.*name is required/);
assert.throws(() => compileGeoJSON(plain, { features: [[1, 0, 0]] }), /name is required/);
assert.throws(
  () => compileGeoJSON(plain, { features: [['A', 0, 0], { name: 'A', lon: 1, lat: 1 }] }),
  /feature "A" is given more than once/
);

// the regular compile surface still carries scenes
assert.ok(compile('box', { scene: true }).scene.items.length === 1);

console.log('smoke-geo: ok');
