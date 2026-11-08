# Style guide — the editorial look, in pic

One skin, two papers. Borrowed from diagram-design (Cathryn Lavery, MIT): white-smoke
paper, jet-black ink, one burnt-tangerine accent, blue-slate for everything secondary.
No shadows, no glows, no gradients on nodes, no pills. The drawing is done when nothing
can be removed.

The tokens live in `assets/editorial-light.pic` and `assets/editorial-dark.pic` as pic
variables and macros. Load one as a **named prelude** so the figure's own line numbers
never shift (an absolute path: `--prelude` resolves a relative FILE against the
*figure's* directory, not the shell's):

```sh
rpic --prelude theme=/abs/path/skills/rpic/assets/editorial-light.pic --svg -o fig.svg fig.pic
scripts/render.sh fig.pic --theme light          # the same, plus PNG and warnings
```

A figure that must stand alone (a gist, a docs example) inlines the tokens it uses
instead; the light values are below.

## Tokens

| Role | Light | Dark | Use |
|---|---|---|---|
| `paper` | `0xf5f5f5` | `0x2d3142` | page, default node fill |
| `paper2` | `0xececec` | `0x393e53` | containers, secondary fill |
| `ink` | `0x2d3142` | `0xf5f5f5` | primary text and stroke |
| `muted` | `0x4f5d75` | `0xbfc0c0` | secondary text, default arrows |
| `soft` | `0x5a6580` | `0x949eb2` | sublabels, eyebrows, arrow labels |
| `rule` | `0xd9dadf` | `0x454959` | hairline borders of containers |
| `accent` | `0xbf4520` | `0xf08a59` | the focal element — 1, at most 2 |
| `accenttint` | `0xf1e7e4` | `0x413a44` | fill behind an accent-stroked box |
| `link` | `0x2e5aa8` | `0x739fdf` | HTTP/API/external arrows |

Every pair clears WCAG 4.5:1 as text on its paper. Colours are plain variables:
`box outlined ink shaded paper`, `"note" shaded muted`.

**Focal rule.** `accent` goes on one or two elements: the headline path, the node the
story is about. Four accents is no decision at all. Everything else is `ink`, `muted`,
`soft`.

## Treatments (macros of the theme)

| Macro | Draws | For |
|---|---|---|
| `node("Name" "sublabel" mono small)` | ink stroke, paper fill, `rad 0.06` | the default node |
| `focal(…)` | accent stroke `thick 1.4`, accent tint fill | the one focal node |
| `store(…)` | muted stroke, `paper2` fill | database, cache, queue, state |
| `ext(…)` | muted dashed stroke | external system, third party, user |
| `group("Label" wid w ht h at P behind X)` | hairline `rule` container, `paper2`, drawn under `X` | boundary, zone, lane |
| `flow(A.e, B.w [, "LABEL" [, AT]])` · `flowhot` · `flowapi` · `flowopt` | muted / accent / link / dashed arrow; the quoted LABEL hugs the stroke — just above a horizontal run, just right of a vertical one — at fraction AT of the way (default 0.5) | relation / headline / API call / optional, async, return |
| `alabel("…", P)` · `alabelw("…", P)` · `alabele("…", P)` | mono small soft text at P / hugging a vertical stroke on its left / right | labels on elbows and on the left arrow of a parallel pair, 14 chars max |
| `eyebrow("SERVICE", Box)` | mono small soft, above the box's top-left | type tag |
| `title("…", P)` · `callout("…", P)` | big bold ink · italic muted | page title · one editorial aside |

A macro takes its text and any extra attributes inside the parentheses:
`node("Queue" wid 1.2 at A + (0, -1))`. Two lines in a node = name, then a `mono small`
sublabel (port, protocol, version). Never a third.

Dark paper: call `page(w, h)` first — the SVG has no background of its own — then draw.

## Typography

| Role | pic | Size |
|---|---|---|
| node name | the first string, plain (or `bold` when it must carry the eye) | 11 pt |
| sublabel, eyebrow, arrow label | `mono small` | 7.7 pt |
| title | `big bold` | 16.5 pt |
| callout | `italic` | 11 pt |

Mono is for technical content — ports, commands, URLs, field types — never a blanket
"dev" font. `font "Geist"` / `font "Instrument Serif"` are allowed in SVG output when the
reader has the fonts; PNG and PDF embed the Go family, so decide hierarchy with size,
weight and mono, not with families.

## Geometry

- Inches; everything on a **0.1 in grid**. Node `1.5 × 0.55`; gaps `0.5–0.7` between
  nodes in a row, `0.9–1.2` between rows; `margin = 0.15`.
- **Orthogonal connectors.** Same row or column: a straight arrow between facing corners
  (`A.e` → `B.w`). Off-axis: one elbow, `from A.s to (A, B.w) then to B.w`
  (`(p, q)` = x of p, y of q). Never a diagonal.
- **Attach points.** Two arrows on one edge get their own points: `A.e + (0, 0.12)` and
  `A.e - (0, 0.12)`.
- **Labels hug their stroke**, never sit on it and never float: the arrow's third
  argument does it, `flow(A.e, B.w, "HTTPS")`. The label must read as the arrow's, not as
  a neighbour's: when the midpoint falls on a container edge, on another stroke or next to
  another node's text, move it along the arrow with the fourth argument —
  `flow(Web.s, Api.n, "HTTPS", 0.3)` keeps it on the segment outside the container,
  `flowapi(Api.s, Pay.n, "REST", 0.78)` on the last quarter. By hand: `alabele`/`alabelw`
  beside a vertical stroke, `alabel("…", P)` anywhere. A pair of parallel arrows is offset
  `±0.15` and labelled on its outer sides (`alabelw` for the left one).
- **Containers and labels.** A container's own name is an UPPERCASE eyebrow in its top-left
  corner; nothing else may sit near that corner, and no arrow label may touch the
  container's border — the reader would read it as the container's.
- **No transit** behind a non-endpoint node: route around, or dashed when unavoidable.
- Draw containers first or `behind` their first child; draw arrows before the nodes
  they could cross.
- Corners `rad 0.06`; radius 0 for data (bars, cells), never larger than `0.1`.

## Budget

| Limit | Value |
|---|---|
| nodes | 9 (above: overview + detail) |
| arrows | 12 |
| accent elements | 2 |
| callouts | 2 |
| text lines per node | 2 |

## Anti-patterns

Dark mode with cyan/purple glows · shadows (`gradient` on a node) · identical boxes
for every concept · a legend floating inside the drawing · labels sitting on arrows ·
diagonal connectors · `rounded` pills · accent on five "important" things · mono as
the default face · a box per word (if a paragraph says it better, write the paragraph).
