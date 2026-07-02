// Builds demo.html: corpus samples highlighted by Shiki with the rpic
// grammar, on a TailwindCSS page, in light and dark themes side by side —
// the integration the documentation site will use.
import { createHighlighter } from 'shiki';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const grammar = JSON.parse(readFileSync(join(here, 'rpic.tmLanguage.json'), 'utf8'));

const hl = await createHighlighter({
  themes: ['github-light', 'github-dark'],
  langs: [grammar],
});

const EXTENSIONS_SAMPLE = `.PS
# rpic extensions showcase (each highlighted as an extension keyword)
texlabels = 1; margin = 0.1
A: box "$-\\frac{T}{2}$" fit gradient "steelblue" "white" gradientangle 90
circle "cache" rad 0.4 crosshatch hatchsep 0.05 opacity 0.6 behind A
line right 1 then up 0.7 close shaded "gold" class "hot"
class last circle "storage"
brace from A.nw to A.ne up "span" wid 0.18
animate A with "pop" for 0.4
.PE`;

const CONTROL_SAMPLE = `.PS
define node { circle rad 0.18 $1 }
if x > 2 then { node("A") } else { box "B" fit }
for i = 1 to 4 do { arrow wid 0.4; node(sprintf("%g", i)) }
print sprintf("laid out %g nodes", 4)
.PE`;

const samples = [
  ['Malha de controle (corpus dpic diag1.pic)', readFileSync(join(here, '../../examples/dpic/sources/diag1.pic'), 'utf8')],
  ['Circuito RLC (examples/rlc.pic, biblioteca -c)', readFileSync(join(here, '../../examples/rlc.pic'), 'utf8')],
  ['Extensões rpic (escopo próprio — cor distinta)', EXTENSIONS_SAMPLE],
  ['Macros e controle', CONTROL_SAMPLE],
];

function block(code, theme) {
  return hl.codeToHtml(code, { lang: 'rpic', theme });
}

const sections = samples
  .map(
    ([title, code]) => `
<section>
  <h2>${title}</h2>
  <div class="grid">
    <div class="card">${block(code, 'github-light')}</div>
    <div class="card dark">${block(code, 'github-dark')}</div>
  </div>
</section>`
  )
  .join('\n');

const html = `<!DOCTYPE html>
<html lang="pt-BR">
<head>
<meta charset="utf-8">
<title>rpic — syntax highlighting (gramática TextMate + Shiki)</title>
<style>
/* Self-contained utility CSS mirroring the Tailwind classes used below —
   the real docs site compiles Tailwind properly; the demo stays offline
   and free of external scripts (supply-chain hygiene). */
*{box-sizing:border-box} body{margin:0;font-family:-apple-system,system-ui,sans-serif}
.bg-slate-50{background:#f8fafc}.text-slate-900{color:#0f172a}
main{max-width:72rem;margin:0 auto;padding:2.5rem 1.5rem}
h1{font-size:1.25rem;font-weight:700;margin:0 0 .25rem}
h2{font-size:.875rem;font-weight:600;color:#334155;margin:0 0 .5rem}
p{font-size:.875rem;color:#475569;margin:0 0 2rem;line-height:1.6}
section{margin-bottom:2.5rem}
.grid{display:grid;gap:1rem}@media(min-width:768px){.grid{grid-template-columns:1fr 1fr}}
.card{border-radius:.5rem;overflow:hidden;border:1px solid #e2e8f0}
.card.dark{border-color:#334155}
.card pre{padding:1rem;overflow-x:auto;font-size:13px;line-height:1.6;margin:0}
code.chip{background:#e2e8f0;padding:0 .25rem;border-radius:.25rem}
footer{font-size:.75rem;color:#94a3b8;margin-top:3rem}
</style>
</head>
<body class="bg-slate-50 text-slate-900">
<main>
  <h1>rpic — syntax highlighting</h1>
  <p>Gramática TextMate (<code class="chip">rpic.tmLanguage.json</code>)
  renderizada pelo <b>Shiki</b> (engine do VS Code) com CSS utilitário (o site de docs usará TailwindCSS compilado) — o setup da futura documentação.
  Temas github-light × github-dark lado a lado; estilos inline, zero conflito com Tailwind.
  Extensões rpic (<code class="chip">fit</code>, <code class="chip">gradient</code>,
  <code class="chip">texlabels</code>…) têm escopo próprio.</p>
  ${sections}
  <footer>A mesma gramática TextMate serve para extensão VS Code e GitHub Linguist (issues futuras).</footer>
</main>
</body>
</html>`;

writeFileSync(join(here, 'demo.html'), html);
console.log('demo.html written,', html.length, 'bytes');
