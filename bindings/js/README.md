# rpic

JS/TS bindings for [rpic](https://github.com/milkway/rpic-lang) — the pic
graphics language compiled to **SVG** with animation and diagnostic manifests,
via WebAssembly. Works in the browser and in Node, ships TypeScript types.

```js
import * as rpic from '@strategicprojects/rpic';

await rpic.ready();                       // browser: wasm fetched automatically

const { svg, animations, diagnostics } = rpic.compile('box "A"; arrow; box "B"');
document.querySelector('#stage').innerHTML = svg;

// animate with GSAP:
import { gsap } from 'gsap';
rpic.animate(document.querySelector('#stage'), animations, gsap);

// circuit library:
rpic.renderSvg('A:(0,0); B:(2,0)\nresistor(A,B)', { circuits: true });
```

### Node

```js
import { readFileSync } from 'node:fs';
import * as rpic from '@strategicprojects/rpic';
await rpic.ready(readFileSync(new URL('./node_modules/rpic/pkg/rpic_wasm_bg.wasm', import.meta.url)));
console.log(rpic.renderSvg('box "hi"'));
```

## API

| Function | Description |
|----------|-------------|
| `ready(wasmInput?)` | Initialize WASM. Browser: no arg. Node: pass `.wasm` bytes/URL. |
| `compile(src, {circuits?, texlabels?})` | → `{ svg, animations, diagnostics }` (throws on a pic error). |
| `renderSvg(src, {circuits?, texlabels?})` | → SVG string. |
| `animate(root, animations, gsap)` | Build/play a GSAP timeline (`draw`/`fade`/`pop`). Browser only. |

PNG/PDF are available via the CLI, the Python package, or the R package (the
WASM core renders SVG; rasterization isn't bundled here).

## Rebuild

```sh
npm run build:wasm
npm test
```
