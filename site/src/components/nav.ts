// Sidebar structure — mirrors the content map of issue #147. Sections grow
// as chapters land; keep paths stable.
export interface NavItem {
  title: string;
  href: string;
}
export interface NavSection {
  label: string;
  items: NavItem[];
}

export const nav: NavSection[] = [
  {
    label: 'Getting started',
    items: [
      { title: 'Installation', href: '/docs/installation' },
      { title: 'Your first picture', href: '/docs/first-picture' },
    ],
  },
  {
    label: 'The language',
    items: [
      { title: 'Primitives', href: '/docs/primitives' },
      { title: 'Positioning', href: '/docs/positioning' },
      { title: 'Labels & ordinals', href: '/docs/labels-and-ordinals' },
      { title: 'Attributes', href: '/docs/attributes' },
      { title: 'Variables & macros', href: '/docs/variables-and-macros' },
      { title: 'Blocks', href: '/docs/blocks' },
    ],
  },
  {
    label: 'rpic extensions',
    items: [
      { title: 'margin', href: '/docs/extensions/margin' },
      { title: 'canvas', href: '/docs/extensions/canvas' },
      { title: 'fit', href: '/docs/extensions/fit' },
      { title: 'font attributes', href: '/docs/extensions/font' },
      { title: 'rotated & colors', href: '/docs/extensions/rotated' },
      { title: 'behind', href: '/docs/extensions/behind' },
      { title: 'close', href: '/docs/extensions/close' },
      { title: 'dot', href: '/docs/extensions/dot' },
      { title: 'brace', href: '/docs/extensions/brace' },
      { title: 'hatch', href: '/docs/extensions/hatch' },
      { title: 'opacity', href: '/docs/extensions/opacity' },
      { title: 'gradient', href: '/docs/extensions/gradient' },
      { title: 'class', href: '/docs/extensions/class' },
      { title: 'link', href: '/docs/extensions/link' },
      { title: 'texlabels', href: '/docs/extensions/texlabels' },
      { title: 'animate', href: '/docs/extensions/animate' },
    ],
  },
  {
    label: 'Circuit library',
    items: [{ title: 'Overview', href: '/docs/circuits' }],
  },
  {
    label: 'Bindings',
    items: [{ title: 'Python · JS · R · C', href: '/docs/bindings' }],
  },
  {
    label: 'Gallery',
    items: [{ title: 'Corpus figures', href: '/docs/gallery' }],
  },
  {
    label: 'Reference',
    items: [
      { title: 'Language spec', href: '/docs/spec' },
      { title: 'The pic family', href: '/docs/pic-family' },
      { title: 'Performance', href: '/docs/performance' },
    ],
  },
];
