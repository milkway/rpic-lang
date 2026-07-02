// Builds themes.html: three green-forward theme candidates for the rpic
// docs, each shown light × dark on the same samples.
import { createHighlighter } from 'shiki';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const grammar = JSON.parse(readFileSync(join(here, 'rpic.tmLanguage.json'), 'utf8'));
const forestLight = JSON.parse(readFileSync(join(here, 'themes/rpic-forest-light.json'), 'utf8'));
const forestDark = JSON.parse(readFileSync(join(here, 'themes/rpic-forest-dark.json'), 'utf8'));

const hl = await createHighlighter({
  themes: ['everforest-light', 'everforest-dark', 'vitesse-light', 'vitesse-dark', forestLight, forestDark],
  langs: [grammar],
});

const SAMPLE = `.PS
# malha de controle com extensões rpic
texlabels = 1; margin = 0.1
define plant { box "$G(s)$" fit gradient "honeydew" "white" }
A: arrow "$u$" above
S: circle rad 0.15 class "sum"
plant()
arrow "$y$" above from last box.e
line -> down 0.5 from 2nd last arrow.c then left 1.2 then to S.s
"$-\\;$" below rjust
for i = 1 to 3 do { move right 0.1 }
animate S with "pop" for 0.4
.PE`;

const themes = [
  ['1. Everforest', 'tema pronto do ecossistema — paleta floresta, quente', 'everforest-light', 'everforest-dark'],
  ['2. Vitesse', 'tema pronto (Anthony Fu) — verdes suaves + teal, minimalista', 'vitesse-light', 'vitesse-dark'],
  ['3. rpic-forest (custom)', 'desenhado para os escopos do rpic: verdes escuros nos papéis do vermelho/azul; extensões em teal itálico; math em tons de terra', 'rpic-forest-light', 'rpic-forest-dark'],
];

const sections = themes
  .map(
    ([title, desc, light, dark]) => `
<section>
  <h2>${title}</h2>
  <p class="desc">${desc}</p>
  <div class="grid">
    <div class="card">${hl.codeToHtml(SAMPLE, { lang: 'rpic', theme: light })}</div>
    <div class="card dark">${hl.codeToHtml(SAMPLE, { lang: 'rpic', theme: dark })}</div>
  </div>
</section>`
  )
  .join('\n');

const html = `<!DOCTYPE html>
<html lang="pt-BR">
<head>
<meta charset="utf-8">
<title>rpic — candidatos de tema (verde escuro)</title>
<style>
*{box-sizing:border-box} body{margin:0;font-family:-apple-system,system-ui,sans-serif;background:#f8fafc;color:#0f172a}
main{max-width:76rem;margin:0 auto;padding:2.5rem 1.5rem}
h1{font-size:1.25rem;font-weight:700;margin:0 0 .25rem}
h2{font-size:1rem;font-weight:700;color:#14532d;margin:0 0 .15rem}
p{font-size:.875rem;color:#475569;margin:0 0 2rem;line-height:1.6}
p.desc{margin-bottom:.6rem}
section{margin-bottom:2.8rem}
.grid{display:grid;gap:1rem}@media(min-width:768px){.grid{grid-template-columns:1fr 1fr}}
.card{border-radius:.5rem;overflow:hidden;border:1px solid #e2e8f0}
.card.dark{border-color:#334155}
.card pre{padding:1rem;overflow-x:auto;font-size:13px;line-height:1.65;margin:0}
</style>
</head>
<body>
<main>
  <h1>rpic — 3 candidatos de tema com verde escuro</h1>
  <p>Mesmo trecho (extensões + math + macros) em cada tema, claro × escuro. Os dois primeiros são temas
  estabelecidos do ecossistema Shiki/VS Code; o terceiro é um tema próprio mapeado exatamente nos escopos
  da gramática rpic.</p>
  ${sections}
</main>
</body>
</html>`;

writeFileSync(join(here, 'themes.html'), html);
console.log('themes.html written,', html.length, 'bytes');
