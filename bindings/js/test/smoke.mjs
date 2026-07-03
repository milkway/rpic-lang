import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { compile, ready, renderSvg } from '../index.js';

const wasm = readFileSync(new URL('../pkg/rpic_wasm_bg.wasm', import.meta.url));
await ready(wasm);

const svg = renderSvg('box "hi"');
assert.match(svg, /<svg\b/);
assert.match(svg, /hi/);

const circuits = compile('A:(0,0); B:(2,0)\nresistor(A,B)', { circuits: true });
assert.match(circuits.svg, /<svg\b/);

const texlabels = compile('box "$x$"', { texlabels: true });
assert.match(texlabels.svg, /<svg\b/);
