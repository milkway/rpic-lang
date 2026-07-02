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
    items: [],
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
];
