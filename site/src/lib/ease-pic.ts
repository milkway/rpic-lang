// Build-time generator of pic source for the ease explorer. Each (family,
// variant) is a self-contained pic: rpic plots the curve from the ease's own
// closed-form formula (`in`/`inOut` are derived from `out` — `in(t)=1-out(1-t)`,
// `inOut` piecewise), and a value marker rides `move` up the axis with the GSAP
// ease `"<family>.<variant>"`, so the animation is pic-driven too.
//
// Only closed-form eases are here — `rough`/`slow`/`expoScale` and the
// Custom* eases have no formula rpic can plot, so they're out of scope.

export const VARIANTS = ['in', 'out', 'inOut'] as const;
export type Variant = (typeof VARIANTS)[number];

// The `.out` formula O(x) per family, as a pic expression in the token `X`.
const OUT: Record<string, string> = {
  power1: '1-(1-(X))^2',
  power2: '1-(1-(X))^3',
  power3: '1-(1-(X))^4',
  power4: '1-(1-(X))^5',
  sine: 'sin((X)*PI/2)',
  expo: '1-2^(-10*(X))',
  circ: 'sqrt(1-((X)-1)^2)',
  back: '1+2.70158*((X)-1)^3+1.70158*((X)-1)^2',
  elastic: '2^(-10*(X))*sin(((X)-0.075)*2*PI/0.3)+1',
};

// Families that take a variant, in display order. `none` (linear) and `bounce`
// (piecewise, handled specially) are listed separately.
export const VARIANT_FAMILIES = [
  'power1', 'power2', 'power3', 'power4', 'sine', 'expo', 'circ', 'back', 'elastic',
];
export const FAMILIES = ['none', ...VARIANT_FAMILIES, 'bounce'];

const PI = '3.14159265358979';
const o = (fam: string, xexpr: string) => OUT[fam].replace(/X/g, `(${xexpr})`).replace(/PI/g, PI);

// bounceOut(x) as a pic block assigning `v` (GSAP's 4-segment bounce).
function bounceOut(v: string, x: string): string {
  return (
    `  bb=(${x})\n` +
    `  if bb<0.363636 then { ${v}=7.5625*bb^2 } else {\n` +
    `  if bb<0.727273 then { ${v}=7.5625*(bb-0.545455)^2+0.75 } else {\n` +
    `  if bb<0.909091 then { ${v}=7.5625*(bb-0.818182)^2+0.9375 } else { ${v}=7.5625*(bb-0.954545)^2+0.984375 } } }`
  );
}

// A pic block that assigns the eased value `v` at parameter `p`, for any family.
function valueBlock(family: string, variant: Variant, v: string, p: string): string {
  if (family === 'none') return `  ${v}=(${p})`;
  if (family === 'bounce') {
    if (variant === 'out') return bounceOut(v, p);
    if (variant === 'in') return bounceOut('bo', `1-(${p})`) + `\n  ${v}=1-bo`;
    // inOut: t<0.5 -> 0.5*bounceIn(2t) = 0.5*(1-bounceOut(1-2t)); else 0.5+0.5*bounceOut(2t-1)
    return (
      `  if (${p})<0.5 then {\n` +
      bounceOut('bo', `1-2*(${p})`) +
      `\n    ${v}=0.5*(1-bo)\n` +
      `  } else {\n` +
      bounceOut('bo', `2*(${p})-1`) +
      `\n    ${v}=0.5+0.5*bo\n` +
      `  }`
    );
  }
  // standard family from its OUT formula
  if (variant === 'out') return `  ${v}=${o(family, p)}`;
  if (variant === 'in') return `  ${v}=1-(${o(family, `1-(${p})`)})`;
  // inOut: t<0.5 -> 0.5*(1-O(1-2t)); else 0.5*(1+O(2t-1))
  return (
    `  if (${p})<0.5 then { ${v}=0.5*(1-(${o(family, `1-2*(${p})`)})) }` +
    ` else { ${v}=0.5*(1+(${o(family, `2*(${p})-1`)})) }`
  );
}

/** The GSAP ease string for the manifest — what you'd pass to `ease "…"`. */
export function easeName(family: string, variant: Variant): string {
  if (family === 'none') return 'none';
  return `${family}.${variant}`;
}

/** Full pic source: the plotted curve + a value marker animating with the ease. */
export function easePic(family: string, variant: Variant): string {
  const name = easeName(family, variant);
  const curve =
    'Curve: [\n' +
    '  for i=0 to n-1 do {\n' +
    '    t=i/n; s=(i+1)/n\n' +
    valueBlock(family, variant, 'a', 't') + '\n' +
    valueBlock(family, variant, 'b', 's') + '\n' +
    '    line from (u*t,u*a) to (u*s,u*b) thick 2.4 colored 0x2f855a\n' +
    '  }\n' +
    '] with .sw at (0,0)';
  return (
    '.PS\n' +
    'margin=0.25\n' +
    'u=2.4; n=44\n' +
    'box wid u ht u thick 0.7 outlined 0xcbd5e1 with .sw at (0,0)\n' +
    'line from (0,0) to (u,u) dashed thick 0.6 colored 0xcbd5e1\n' +
    curve + '\n' +
    'VT: line from (u+0.45,0) to (u+0.45,u) thick 0.9 colored 0xcbd5e1\n' +
    'VD: dot at VT.start rad 0.09 colored 0x0ae448\n' +
    `"${name}" at (u/2,-0.28)\n` +
    `animate VD with "move" along VT ease "${name}" for 1.6 repeat -1 yoyo\n` +
    '.PE\n'
  );
}
