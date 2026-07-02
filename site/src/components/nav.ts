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
    items: [],
  },
  {
    label: 'Circuit library',
    items: [],
  },
  {
    label: 'Bindings',
    items: [],
  },
  {
    label: 'Reference',
    items: [
      { title: 'Language spec', href: '/docs/spec' },
      { title: 'The pic family', href: '/docs/pic-family' },
    ],
  },
];
