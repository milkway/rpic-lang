// Smoke test for the math-enabled build: texlabels must actually typeset
// (glyph paths, no literal TeX, no fallback diagnostic). Runs in its own
// process — ready() fixes the build choice per module instance.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { compile, ready, renderSvg } from '../index.js';

const wasm = readFileSync(new URL('../pkg/rpic_wasm_math_bg.wasm', import.meta.url));
await ready(wasm, { math: true });

const out = compile('box "$-\\\\frac{T}{2}$" wid 1 ht 0.7', { texlabels: true });
assert.match(out.svg, /<svg\b/);
// the formula is embedded as a nested <svg> fragment of glyph paths
assert.match(out.svg, /<svg x="/);
assert.doesNotMatch(out.svg, /frac/, 'raw TeX must not leak');
assert.equal(
  out.diagnostics.filter((d) => d.includes('no math renderer')).length,
  0,
  'math build must not fall back'
);

// the plain path still works on the math build
assert.match(renderSvg('box "hi"'), /hi/);
