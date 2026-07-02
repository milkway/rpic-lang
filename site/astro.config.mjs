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
  vite: { plugins: [tailwindcss()] },
  markdown: {
    shikiConfig: {
      langs: [grammar],
      themes: { light: forestLight, dark: forestDark },
    },
  },
});
