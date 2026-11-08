---
name: rpic
description: Draw diagrams, circuit schematics and map annotations as text with rpic (the pic language, Kernighan's picture language modernised — SVG/PNG/PDF, GeoJSON) in an editorial style. Use this skill whenever the user wants an architecture, flow, sequence, state, timeline, swimlane, tree, layers, quadrant, funnel, bar or Venn diagram, a circuit/schematic (resistors, op-amps, logic gates, SPICE), a drawing placed on a map (georeferenced sketch, site plan, radius, route), or asks for pic/dpic/pikchr source, "rpic", "diagrama", "circuito", "esquemático" or "mapa" — even when they just say "make me a diagram of…" without naming a tool.
license: BSD-2-Clause
metadata:
  version: "0.1"
---

# rpic — diagrams as text

You write a short `.pic` source, render it with the `rpic` binary, **look at the PNG**,
fix, and deliver the source with its SVG/PNG. The source is the artifact: it diffs,
it is reproducible, and a reader can edit it without a drawing tool.

pic is Brian Kernighan's picture language (1982); rpic is a Rust reimplementation
compatible with dpic (Dwight Aplevich) and pikchr (D. Richard Hipp), with opt-in
extensions (colour literals, fonts, layers, maps, animation). Docs: https://rpic.dev.

## 1. Decide before drawing

- **Would a table or a paragraph do the job?** If yes, don't draw.
- **One idea per node, one piece of information per arrow.** Two nodes that always
  travel together are one node. A relationship obvious from layout needs no line.
- **One focal element, two at most.** The accent colour is editorial, not a flag.
- **Budget**: 9 nodes, 12 arrows. Above that: an overview and a detail.
- **State the plan** in one line before rendering (type, what is focal, what you cut)
  unless the request already pins everything.

Then route:

| The user wants | Read |
|---|---|
| a diagram (architecture, flow, sequence, state, timeline, swimlane, nested, tree, layers, quadrant, funnel, loop, bars, Venn) | `references/diagrams.md` (recipes) + `references/style-guide.md` (tokens, treatments, rules) |
| a circuit / schematic / SPICE figure | `references/circuits.md` |
| something placed on the Earth (lon/lat, site plan, radius, route, map layer) | `references/maps.md` |
| language details (positions, attributes, macros, loops, extensions) | `references/language.md` |
| install, render, debug, QA, other outputs | `references/render-qa.md` |

Load only what the task needs; `language.md` is the fallback when a recipe does not
cover a construct.

## 2. Write the source

Start from the nearest recipe. Keep the grammar, replace the content (this one uses
the theme's macros, so it renders with `--theme light`):

```pic theme=light
margin = 0.15
Web: node("Web app" "Next.js" mono small)
Api: node("API" "axum · :8080" mono small, at Web + (2.2, 0))
Db:  focal("Postgres" "primary" mono small, at Api + (2.2, 0))
flow(Web.e, Api.w, "HTTPS")        # the label hugs the arrow: above a horizontal run, beside a vertical one
flowhot(Api.e, Db.w, "SQL")
```

Conventions that make it read well:

- **Inches on a 0.1 grid**: nodes `1.5 × 0.55`, gaps `0.5–0.7` in a row, `0.9–1.2`
  between rows, `margin = 0.15`.
- **Orthogonal arrows** between facing corners (`A.e` → `B.w`); off-axis = one elbow:
  `flow(A.s, (A, B.w) then to B.w)`. Never a diagonal.
- **Labels hug their stroke**, never on it, never floating: an arrow's label is its third
  argument (`flow(A.e, B.w, "HTTPS")`) and the theme places it; a fourth argument slides
  it along the arrow (`…, "HTTPS", 0.3`) when the midpoint lands on a container edge or
  beside another node, so it never reads as someone else's label. Node text: a name, then
  at most one `mono small` sublabel (port, protocol, version).
- Containers drawn first or `behind` their first child; arrows drawn before the nodes
  they could cross.
- Comment the source like code: a header line saying what it shows, a line per group.

The style tokens and treatments (`node`, `focal`, `store`, `ext`, `group`, `flow`,
`flowhot`, `flowapi`, `flowopt`, `alabel`, `eyebrow`, `title`, `callout`, dark `page`)
come from a **theme prelude**, `assets/editorial-light.pic` or `-dark.pic` — loaded as a
named prelude, so your line numbers never shift. A figure that must stand alone inlines
the few tokens it uses (values in `style-guide.md`).

## 3. Render and look

```sh
scripts/render.sh figure.pic --theme light      # → figure.svg, figure.png (2×), warnings
scripts/render.sh circuit.pic -c                # circuit library
scripts/render.sh paper.pic -t                  # $…$ labels as TeX math
```

By hand: `rpic --prelude theme=/abs/path/assets/editorial-light.pic --svg -o f.svg f.pic`
(the prelude path must be absolute: a relative one resolves against the figure's
directory). Then **open the PNG and look**: edges clipped? a label on a stroke? two
labels colliding? the accent where the eye should land? Fix the source, re-render.
`rpic --json f.pic` lists warnings (`invalid_color`, `ignored_attribute`): deliver
warning-clean. Fixes for the usual symptoms: `references/render-qa.md`.

No `rpic` binary: `brew install milkway/rpic/rpic`, `cargo install rpic-cli`,
`pip install rpiclang`, `npm i @strategicprojects/rpic`; or paste the source into
https://studio.rpic.dev (it detects `-c`/`-t`, and opens a Map tab for geo sources).
Text colour (`"note" shaded muted`) needs rpic ≥ 0.18.

## 4. Deliver

The `.pic` source first (it is what the user keeps), then the rendered `.svg` and/or
`.png`, and the one-line command that reproduces them (flags, theme). Mention the
assumptions you made (what you cut for the budget, what is focal). For a map, also the
GeoJSON or the R/JS call (`maps.md`). For a circuit meant for simulation, the `spice =
1` marker and the `.tran`/`.ac` card.

## 5. Pre-output checklist

- [ ] Right type? Would a table do? Plan stated?
- [ ] Every node a distinct idea; every arrow carries information; nothing removable?
- [ ] Accent on ≤ 2 elements; mono only on technical text; no shadows/glows/pills?
- [ ] Orthogonal connectors; every label hugging its own stroke, away from container edges; distinct attach points; no transit behind a non-endpoint node?
- [ ] Within budget (9 nodes / 12 arrows), or split?
- [ ] Rendered, PNG looked at, warning-clean, margin set, nothing clipped?
- [ ] Delivered the `.pic` + render + command?

## Credits

pic by Brian W. Kernighan; dpic and circuit_macros by Dwight Aplevich; pikchr by
D. Richard Hipp. The editorial palette and the "deletion first" discipline follow
diagram-design by Cathryn Lavery (MIT). rpic is BSD-2-Clause, https://github.com/milkway/rpic-lang.
