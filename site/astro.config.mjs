// Astro config for the rpic documentation site (GitHub Pages).
import { defineConfig } from 'astro/config';
import mdx from '@astrojs/mdx';
import tailwindcss from '@tailwindcss/vite';
import { readFileSync } from 'node:fs';

// The highlighting stack is the project one (#142): TextMate grammar +
// rpic-forest dual themes, shared with tools/highlight.
const grammar = JSON.parse(readFileSync('../tools/highlight/rpic.tmLanguage.json', 'utf8'));
const forestLight = JSON.parse(readFileSync('../tools/highlight/themes/rpic-forest-light.json', 'utf8'));
const forestDark = JSON.parse(readFileSync('../tools/highlight/themes/rpic-forest-dark.json', 'utf8'));

export default defineConfig({
  site: 'https://rpic.dev',
  integrations: [mdx()],
  vite: {
    plugins: [tailwindcss()],
    // The Maps playground imports the JS binding from the repo
    // (`file:../bindings/js`, a symlink). Keep it out of the dev-time
    // pre-bundle so wasm-pack's `new URL('…_bg.wasm', import.meta.url)`
    // keeps pointing beside the glue (the production build handles that URL
    // as an asset on its own), and let the dev server read outside site/.
    optimizeDeps: { exclude: ['@strategicprojects/rpic'] },
    server: { fs: { allow: ['..'] } },
  },
  markdown: {
    shikiConfig: {
      langs: [grammar],
      themes: { light: forestLight, dark: forestDark },
    },
  },
});
