import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { compile, ready, renderSvg } from '../index.js';

const wasm = readFileSync(new URL('../pkg/rpic_wasm_bg.wasm', import.meta.url));

// #210: a failed init must not be cached — ready() is retryable
await assert.rejects(() => ready(Buffer.from('not a wasm module')), undefined, 'garbage bytes must reject');

// concurrent callers share one in-flight init; retry after failure succeeds
const p1 = ready(wasm);
const p2 = ready(wasm);
assert.equal(p1, p2, 'concurrent ready() calls must share the same promise');
await p1;

await assert.rejects(
  () => ready(wasm, { math: true }),
  /already initialized the lean build/,
  'ready() must reject attempts to switch builds after initialization'
);

const svg = renderSvg('box "hi"');
assert.match(svg, /<svg\b/);
assert.match(svg, /hi/);

const circuits = compile('A:(0,0); B:(2,0)\nresistor(A,B)', { circuits: true });
assert.match(circuits.svg, /<svg\b/);

// `copy "circuits"` loads the embedded library with no option — even under
// wasm, where file includes are unavailable
const inSource = compile('copy "circuits"\nA:(0,0); B:(2,0)\nresistor(A,B)');
assert.equal(inSource.svg, circuits.svg);

// #227: per-object geometry rides along in the bundle
const geom = compile('box wid 1 ht 0.5\narrow right 0.5');
assert.equal(geom.objects.length, 2);
assert.equal(geom.objects[0].id, 's0');
assert.equal(geom.objects[0].kind, 'box');
assert.equal(geom.objects[0].bbox.w, 96);
assert.equal(geom.objects[0].bbox.h, 48);
assert.equal(geom.objects[1].kind, 'path');
assert.equal(geom.objects[1].line, 2);
assert.equal(geom.objects[1].col, 1);

const warning = compile('box "a" dashd');
assert.equal(warning.warnings[0].kind, 'ignored_attribute');
assert.equal(warning.warnings[0].found, 'dashd');

assert.throws(
  () => compile('bxo'),
  (err) => {
    assert.equal(err.errorInfo.kind, 'expected_token');
    assert.equal(err.errorInfo.found, '`bxo`');
    assert.equal(err.errorInfo.hint, 'did you mean `box`?');
    return true;
  }
);

// #181: the circuits/texlabels preludes must not shift user positions —
// an error on the user's line 1 reports line 1, not ~line 1093
assert.throws(
  () => compile('bxo', { circuits: true, texlabels: true }),
  (err) => {
    assert.equal(err.errorInfo.line, 1);
    assert.equal(err.errorInfo.col, 1);
    assert.equal(err.errorInfo.file, null);
    return true;
  }
);

// lean build: texlabels falls back to literal text plus a diagnostic
const texlabels = compile('box "$x$"', { texlabels: true });
assert.match(texlabels.svg, /<svg\b/);
assert.ok(
  texlabels.diagnostics.some((d) => d.includes('no math renderer')),
  'lean build must diagnose the fallback'
);
