# rpic

JS/TS bindings for [rpic](https://github.com/milkway/rpic-lang) — the pic
graphics language compiled to **SVG** with animation and diagnostic manifests,
via WebAssembly. Works in the browser and in Node, ships TypeScript types.

```js
import * as rpic from '@strategicprojects/rpic';

await rpic.ready();                       // browser: wasm fetched automatically

const { svg, animations, diagnostics, warnings, objects } = rpic.compile('box "A"; arrow; box "B"');
document.querySelector('#stage').innerHTML = svg;

// animate with GSAP:
import { gsap } from 'gsap';
rpic.animate(document.querySelector('#stage'), animations, gsap);

// pre-rendered SVG (e.g. from `rpic --json` on the CLI)? Import just the
// player — a zero-import module that never touches the wasm compiler:
// import { animate } from '@strategicprojects/rpic/player';

// circuit library:
rpic.renderSvg('A:(0,0); B:(2,0)\nresistor(A,B)', { circuits: true });
```

### No bundler? Plain HTML via CDN

The package works straight from a CDN — compile in the page, or pre-render
with the CLI (`rpic --json fig.pic`) and import only the player (`animate()`
never touches the wasm):

```html
<div id="stage"></div>
<script type="module">
  import { ready, compile, animate } from
    'https://cdn.jsdelivr.net/npm/@strategicprojects/rpic@0.10.0/index.js';
  import { gsap } from 'https://cdn.jsdelivr.net/npm/gsap@3.13.0/+esm';
  await ready();                    // the .wasm is fetched from the CDN
  const { svg, animations } = compile('box "A"; arrow; box "B"\nanimate last box with "pop"');
  const stage = document.querySelector('#stage');
  stage.innerHTML = svg;
  animate(stage, animations, gsap);
</script>
```

The [animate docs](https://rpic.dev/docs/extensions/animate#quick-start--plain-html-no-build-step)
carry the full recipe, including the pre-rendered variant with classic
`<script>` tags (with SRI hashes) and the plugin files `move`/`morph`/
`scramble`/`wiggle` need.

### Node

```js
import { readFileSync } from 'node:fs';
import * as rpic from '@strategicprojects/rpic';
await rpic.ready(readFileSync(new URL('./node_modules/@strategicprojects/rpic/pkg/rpic_wasm_bg.wasm', import.meta.url)));
console.log(rpic.renderSvg('box "hi"'));
```

### TeX math labels (`texlabels`)

The default wasm build is lean and renders `$…$` labels as literal text (plus
a diagnostic). To typeset them exactly like the native CLI, opt into the
math-enabled build — a second, heavier `.wasm` (RaTeX + embedded KaTeX glyph
data) that is only fetched when you ask for it:

```js
await rpic.ready(undefined, { math: true }); // browser: fetches pkg/rpic_wasm_math_bg.wasm
rpic.renderSvg('box "$-\\frac{T}{2}$" fit', { texlabels: true });
```

Apps can keep the fast path untouched and lazy-load math only when the source
contains `$…$`. The build choice is fixed by the first `ready()` call; a later
call that asks for the other build rejects instead of silently reusing the
first build. In Node, pass the math artifact's bytes:
`ready(readFileSync(new URL('…/pkg/rpic_wasm_math_bg.wasm', import.meta.url)), { math: true })`.

## API

| Function | Description |
|----------|-------------|
| `ready(wasmInput?, {math?})` | Initialize WASM. Browser: no arg. Node: pass `.wasm` bytes/URL. `math: true` loads the math-enabled build; conflicting later calls reject. |
| `compile(src, {circuits?, texlabels?})` | → `{ svg, animations, diagnostics, warnings, objects }` (throws on a pic error with `errorInfo`). |
| `renderSvg(src, {circuits?, texlabels?})` | → SVG string. |
| `animate(root, animations, gsap)` | Build/play a GSAP timeline (`draw`/`fade`/`pop`). Browser only. |

Compile errors keep a readable `message` and expose structured data for editors:

```js
try {
  rpic.compile('bxo');
} catch (err) {
  console.log(err.errorInfo); // { message, line, col, end_col, file, kind, found, expected, hint }
}
```

Positions are always relative to your own source — with `circuits: true` an
error on your line 1 reports `line: 1`, and a problem inside a `copy` include
carries the include's name in `file` (`null` means your own input).

PNG/PDF are available via the CLI, the Python package, or the R package (the
WASM core renders SVG; rasterization isn't bundled here).

## Rebuild

```sh
npm run build:wasm
npm test
```
