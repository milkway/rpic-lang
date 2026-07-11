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
// the type effect splits the label into `.rpic-ch` tspans; `by word` rides the unit
const typeAnim = compile('box "hi there"\nanimate last with "type" by word');
assert.equal(typeAnim.animations[0].effect, 'type');
assert.equal(typeAnim.animations[0].unit, 'word');
assert.ok(typeAnim.svg.includes('<tspan class="rpic-ch">hi</tspan>'), 'type splits label');
// the scramble effect rides a custom charset and leaves the text intact
const scrAnim = compile('box "SECRET"\nanimate last with "scramble" by "01"');
assert.equal(scrAnim.animations[0].effect, 'scramble');
assert.equal(scrAnim.animations[0].chars, '01');
assert.ok(scrAnim.svg.includes('>SECRET</text>'), 'scramble does not split the label');
// the wiggle effect rides an oscillation count
const wigAnim = compile('box\nanimate last with "wiggle" wiggles 8');
assert.equal(wigAnim.animations[0].effect, 'wiggle');
assert.equal(wigAnim.animations[0].wiggles, 8);
// the draw effect rides an optional reveal window as stroke fractions
const drawRange = compile('line right 2\nanimate last with "draw" from 40% to 60%');
assert.equal(drawRange.animations[0].effect, 'draw');
assert.equal(drawRange.animations[0].drawFrom, 0.4);
assert.equal(drawRange.animations[0].drawTo, 0.6);
// a plain draw stays compact — no range keys
const plainDraw = compile('line right 2\nanimate last with "draw"');
assert.equal(plainDraw.animations[0].drawFrom, undefined);
assert.equal(plainDraw.animations[0].drawTo, undefined);
// the draggable directive rides a separate `interactions` array
const dragBundle = compile('B: box wid 3 ht 2\nN: circle at B.c\ndraggable N inertia bounds B x');
assert.ok(Array.isArray(dragBundle.interactions), 'interactions present');
assert.deepEqual(dragBundle.interactions[0], { id: 's1', kind: 'drag', inertia: true, bounds: 's0', axis: 'x' });
// a plain drawing has no interactions
const plainBundle = compile('box');
assert.ok(!plainBundle.interactions || plainBundle.interactions.length === 0, 'no interactions');
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

// #356: the player is a standalone, zero-import module — importable without
// the wasm glue, and the root re-export is the same function.
{
  const player = await import('../player.js');
  assert.equal(typeof player.animate, 'function');
  assert.equal(typeof player.interactive, 'function');
  const root = await import('../index.js');
  assert.equal(root.animate, player.animate, 'index.js must re-export the player');
  assert.equal(root.interactive, player.interactive);
  // regression guard: player.js must never grow an import (that is the point)
  const src = readFileSync(new URL('../player.js', import.meta.url), 'utf8');
  assert.ok(!/^\s*import\b/m.test(src), 'player.js must have zero imports');
  assert.ok(!/from\s+['"]\.\/pkg\//.test(src), 'player.js must not touch the wasm glue');
}
