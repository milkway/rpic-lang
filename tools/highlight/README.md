# rpic syntax highlighting

`rpic.tmLanguage.json` is the **TextMate grammar** for the rpic language —
the single source of truth for highlighting. It is consumed here by
[Shiki](https://shiki.style) (the VS Code engine used by modern doc
generators: Astro/Starlight, VitePress, Nextra), and the same file can later
back a VS Code extension and GitHub Linguist registration.

## Using with Shiki (docs site)

```js
import { createHighlighter } from 'shiki';
import grammar from './rpic.tmLanguage.json' with { type: 'json' };

import forestLight from './themes/rpic-forest-light.json' with { type: 'json' };
import forestDark from './themes/rpic-forest-dark.json' with { type: 'json' };

const hl = await createHighlighter({
  themes: [forestLight, forestDark],
  langs: [grammar],
});
const html = hl.codeToHtml(src, { lang: 'rpic', theme: 'rpic-forest-light' });
```

## Theme

**rpic-forest** (`themes/rpic-forest-{light,dark}.json`) is the project
theme: dark greens take the roles red/blue play in stock themes, rpic
extensions render in dark teal (bold italic), and `$…$` math in earth tones.
Any VS Code/TextMate theme also works — compare candidates with
`node themes-demo.mjs`.

Shiki emits inline styles, so the blocks drop into a TailwindCSS page with
zero style conflicts and native dual-theme support.

## Scopes worth knowing

- `keyword.other.extension.rpic` — **rpic-only extensions** (`fit`, `hatch*`,
  `gradient*`, `opacity`, `class`, `close`, `behind`, `texlabels`,
  `margin*`, `brace*`) get their own scope so documentation themes can
  visually distinguish them from classic pic/dpic.
- `markup.italic.math.rpic` / `support.function.tex.rpic` — the `$…$` math
  payload inside strings (pairs with the `texlabels` extension).
- Contextual keywords (`class = 2` is a plain assignment) are highlighted as
  keywords regardless of context — highlighting is approximate by design.

## Develop

```sh
npm install
npm test    # scope assertions + corpus tokenization smoke test
npm run demo && open demo.html
```
