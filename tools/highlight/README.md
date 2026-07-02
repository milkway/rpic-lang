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

const hl = await createHighlighter({
  themes: ['github-light', 'github-dark'],
  langs: [grammar],
});
const html = hl.codeToHtml(src, { lang: 'rpic', theme: 'github-light' });
```

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
