// Token-scope tests for the rpic TextMate grammar, run through Shiki's
// engine (the same one the docs will use). Each case asserts that a given
// token in a snippet lands in the expected scope.
import { createHighlighter } from 'shiki';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const grammar = JSON.parse(readFileSync(join(here, 'rpic.tmLanguage.json'), 'utf8'));

const hl = await createHighlighter({
  themes: ['github-light'],
  langs: [grammar],
});

let failures = 0;
function scopesOf(code, token) {
  // tokenize with scope info and find the token's innermost scopes
  const { tokens } = hl.codeToTokens(code, {
    lang: 'rpic',
    theme: 'github-light',
    includeExplanation: 'scopeName',
  });
  const found = [];
  for (const line of tokens) {
    for (const t of line) {
      if (t.content.trim() === token && t.explanation) {
        for (const ex of t.explanation) {
          found.push(...ex.scopes.map((s) => s.scopeName));
        }
      }
    }
  }
  return found;
}

function expectScope(code, token, scope) {
  const scopes = scopesOf(code, token);
  if (scopes.some((s) => s.startsWith(scope))) {
    console.log(`  ok  ${JSON.stringify(token)} -> ${scope}`);
  } else {
    console.error(`FAIL  ${JSON.stringify(token)} expected ${scope}, got: ${scopes.join(', ') || '(none)'}\n      in: ${code}`);
    failures++;
  }
}

console.log('scope assertions:');
// classic pic
expectScope('box wid 1 ht 0.5', 'box', 'storage.type.primitive');
expectScope('box wid 1', 'wid', 'keyword.other.attribute');
expectScope('# a comment', '# a comment', 'comment.line');
expectScope('.PS', '.PS', 'keyword.control.directive');
expectScope('A: box', 'A', 'entity.name.section.label');
expectScope('arrow from A.n to B.s', 'from', 'keyword.other.attribute');
expectScope('box at A.ne', '.ne', 'variable.language.corner');
expectScope('line right 2nd last box', '2nd last', 'constant.language.ordinal');
expectScope('scale = 2', 'scale', 'support.constant.environment');
expectScope('x = sqrt(2)', 'sqrt', 'support.function.builtin');
expectScope('define warn { box }', 'define', 'keyword.control');
expectScope('for i = 1 to 3 do { box }', 'for', 'keyword.control');
expectScope('resistor(A,B)', 'resistor', 'entity.name.function.macro');
expectScope('arrow ->', '->', 'keyword.operator.arrowhead');
// strings and $…$ math
expectScope('box "plain label"', '"plain label"', 'string.quoted.double');
expectScope('box "$\\beta$"', '\\beta', 'support.function.tex');
// rpic extensions (distinct scope)
for (const kw of ['fit', 'behind', 'close', 'hatch', 'crosshatch', 'opacity', 'gradient', 'class', 'texlabels', 'margin']) {
  expectScope(`box ${kw} 1`, kw, 'keyword.other.extension');
}
expectScope('animate B1 with "pop"', 'animate', 'keyword.control');

// smoke: full corpus samples must tokenize without throwing
const samples = [
  '../../examples/dpic/sources/diag1.pic',
  '../../examples/dpic/manual/man16.pic',
  '../../examples/rlc.pic',
  '../../examples/hatch.pic',
  '../../examples/pipeline.pic',
];
console.log('corpus smoke:');
for (const rel of samples) {
  const src = readFileSync(join(here, rel), 'utf8');
  hl.codeToHtml(src, { lang: 'rpic', theme: 'github-light' });
  console.log(`  ok  ${rel}`);
}

if (failures > 0) {
  console.error(`\n${failures} failure(s)`);
  process.exit(1);
}
console.log('\nall passed');
