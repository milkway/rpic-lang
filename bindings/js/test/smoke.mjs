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

// animate manifest: optional GSAP keys ride along only when set
const anim = compile(
  'box\nanimate last box with "pop" for 0.4 repeat -1 yoyo ease "power2.inOut"'
);
assert.equal(anim.animations.length, 1);
assert.equal(anim.animations[0].id, 's0');
assert.equal(anim.animations[0].effect, 'pop');
assert.equal(anim.animations[0].repeat, -1);
assert.equal(anim.animations[0].yoyo, true);
assert.equal(anim.animations[0].ease, 'power2.inOut');
// a plain animation stays compact — no repeat/yoyo/ease keys
const plainAnim = compile('box\nanimate last box with "fade"');
assert.equal(plainAnim.animations[0].repeat, undefined);
assert.equal(plainAnim.animations[0].yoyo, undefined);
assert.equal(plainAnim.animations[0].ease, undefined);
assert.equal(plainAnim.animations[0].path, undefined);
// the move effect records the followed path's id
const moveAnim = compile('L: line right 3\nD: dot at L.start\nanimate D with "move" along L');
assert.equal(moveAnim.animations[0].effect, 'move');
assert.equal(moveAnim.animations[0].path, 's0');
// the highlight effect resolves its target colour
const hlAnim = compile('box\nanimate last box with "highlight" to rgb(255,140,0)');
assert.equal(hlAnim.animations[0].effect, 'highlight');
assert.equal(hlAnim.animations[0].color, '#ff8c00');
// stagger fans across a block's children into one entry each
const stAnim = compile('B: [ box; box; box ]\nanimate B with "fade" for 0.3 stagger 0.15');
assert.equal(stAnim.animations.length, 3);
assert.deepEqual(
  stAnim.animations.map((a) => [a.id, a.start]),
  [['s0', 0], ['s1', 0.15], ['s2', 0.3]]
);
// exit modifier and directional slide
const outAnim = compile('box\nanimate last box with "slide" from left out');
assert.equal(outAnim.animations[0].effect, 'slide');
assert.equal(outAnim.animations[0].from, 'left');
assert.equal(outAnim.animations[0].out, true);
// animate scroll: top-level hint, present only when set
assert.equal(compile('box\nanimate last box with "fade"\nanimate scroll').scroll, true);
assert.equal(compile('box\nanimate last box with "fade"').scroll, undefined);
// morph records the target shape id
const morphAnim = compile('A: box\nB: circle at A+(2,0)\nanimate A with "morph" into B');
assert.equal(morphAnim.animations[0].effect, 'morph');
assert.equal(morphAnim.animations[0].morph, 's1');

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
