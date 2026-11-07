// Snapshot check for the Maps page (#432): the layer specs the site's maps
// draw — from the build-time engine (`rpic --geojson --maplibre`, GeoMap.astro)
// and from the package's JS port (`@strategicprojects/rpic/maplibre`, the
// fallback when no layers are passed) — must equal the specs the site drew
// before the styling moved into geopic-core, frozen in
// examples/geo/maplibre/*.layers.json. Numbers compare to a relative 1e-12
// (cos φ comes from the platform libm). The playground's
// `compileGeoJSON(…, { layers: true })` path is pinned to the same snapshots
// by bindings/js/test/smoke-maplibre.mjs.
//
//   RPIC_BIN=<rpic with --maplibre> node scripts/check-maplibre-layers.mjs
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { layers } from '@strategicprojects/rpic/maplibre';
// maplibre-gl's own style-spec (its dependency): evaluates the layer expressions
import { createExpression, latest } from '@maplibre/maplibre-gl-style-spec';

const here = dirname(fileURLToPath(import.meta.url));
const geoDir = resolve(here, '../../examples/geo');
const snapDir = join(geoDir, 'maplibre');
const workspaceBin = resolve(here, '../../target/release/rpic');
const bin = process.env.RPIC_BIN ?? (existsSync(workspaceBin) ? workspaceBin : 'rpic');

function close(a, b, path) {
  if (typeof a === 'number' && typeof b === 'number') {
    assert.ok(Math.abs(a - b) <= 1e-12 * Math.max(Math.abs(a), Math.abs(b)), `${path}: ${a} vs ${b}`);
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

const snap = (name) => JSON.parse(readFileSync(join(snapDir, `${name}.layers.json`), 'utf8'));
const pics = readdirSync(geoDir).filter((f) => f.endsWith('.pic')).sort();
assert.ok(pics.length >= 4, 'the four geo examples');
for (const pic of pics) {
  const name = pic.replace(/\.pic$/, '');
  const out = JSON.parse(
    execFileSync(bin, ['-c', '--geojson', '--maplibre', pic], { cwd: geoDir, encoding: 'utf8', maxBuffer: 16 << 20 }),
  );
  close(out.layers, snap(name), `${name}: engine`);
  close(layers(out.geojson), snap(name), `${name}: JS port`);
  console.log(`ok ${name}`);
}
// the playground's frame switch: the callout under mercator, and an empty map
const callout = JSON.parse(readFileSync(join(geoDir, 'callout.geojson'), 'utf8'));
callout.geopic.registration.frame = 'mercator';
close(layers(callout), snap('callout-mercator'), 'callout-mercator: JS port');
close(layers({ type: 'FeatureCollection', features: [] }), snap('empty'), 'empty: JS port');
// G10 addendum: evaluate the symbol layers' text-field with MapLibre's own
// expression engine (the style-spec maplibre-gl ships): a trailing blank row
// gains a no-break space so the shaper keeps it; everything else is verbatim
{
  const field = snap('callout').find((l) => l.id === 'pic-title').layout['text-field'];
  const e = createExpression(field, latest.layout_symbol['text-field']);
  assert.equal(e.result, 'success', JSON.stringify(e.value));
  const evalText = (t) => String(e.value.evaluate({ zoom: 16 }, { type: 'Point', properties: { 'geopic:text': t } }));
  for (const [t, want] of [['\nbelow', '\nbelow'], ['above\n', 'above\n\u00a0'], ['a\n\nc', 'a\n\nc'], ['one', 'one'], ['', '']]) {
    assert.equal(evalText(t), want, JSON.stringify(t));
  }
}
console.log('check-maplibre-layers: ok');
