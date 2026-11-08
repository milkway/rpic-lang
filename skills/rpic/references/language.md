# rpic language — the working cheat-sheet

rpic is pic (Kernighan, 1982): a picture is a list of objects placed on a plane in
**inches**, each statement drawing the next object in the current direction. dpic
(Aplevich) and pikchr (Hipp) flavours are accepted; the extensions marked **(ext)** are
rpic's own and inert when unused. Full spec: https://rpic.dev/docs/spec.

## Shape of a source

```pic
# comments start with #; statements end at newline or ;
boxwid = 1.2; boxht = 0.5           # defaults for what follows
A: box "input"                      # a label (Capitalised) names the object
arrow                               # moves in the current direction (right)
B: box "process" fill 0.92          # grey fill: 0 black … 1 white
arrow
C: ellipse "output"
arc cw -> from A.n to C.n           # an arc with an arrowhead
```

`.PS` / `.PE` fences are optional. A path continues across a newline next to `then`, or
with a trailing `\`.

## Primitives

| Closed (have an interior) | Open (paths) | Other |
|---|---|---|
| `box` `circle` `ellipse` | `line` `arrow` `spline` `arc` | `move` (invisible step), `"text"` alone, `dot` **(ext)**, `brace` **(ext)**, `[ … ]` block |

Sizes: `wid` `ht` (boxes, ellipses, blocks), `rad` / `diam` (circles, arcs; `box rad 0.08` =
rounded corners), `same` copies the previous object's size. Defaults: `boxwid 0.75`,
`boxht 0.5`, `circlerad 0.25`, `linewid 0.5`, `arrowht 0.1`, `arrowwid 0.05`,
`linethick 0.8` (points), `textht 0.14`.

## Positioning

| Write | Meaning |
|---|---|
| `box at (1, 2)` | centre there |
| `box with .sw at A.ne` | that corner there |
| `box at A + (1.6, 0)` | offset from a place |
| `line from A.e to B.w` | explicit ends (corners: `.n .ne .e .se .s .sw .w .nw .c`; paths: `.start .end`) |
| `arrow right 1.2 then down 0.5 then right 1` | chained segments: orthogonal elbows |
| `arrow from A.e to (B, A.e) then to B.n` | `(p, q)` = x of p, y of q — the turning point of an elbow |
| `"x" at 0.5 between A and B` · `1/3 of the way between A and B` | interpolate — an offset needs parentheses: `(0.5 between A and B) + (0, 0.12)` (without them the offset lands on B and is halved; pic grammar, dpic agrees) |
| `last box.e` · `2nd circle` · `3rd last box` | ordinals (`last` alone = any kind) |
| `up` `down` `left` `right` | change the current direction |
| `Here` | the current point |
| `chop` · `chop 0.1` | trim a line's ends (bare `chop` = `circlerad`, for lines between circles) |

Blocks: `Svc: [ box "a"; arrow; box "b" ]` groups objects; `Svc.n`, `Svc.wid`, and inner
labels `Svc.A`. Inside `[ ]`, `x = v` is block-local; `x := v` writes the outer variable.

## Attributes

| Group | Words |
|---|---|
| Stroke | `thick 1.4` (points) · `thin` · `dashed [pitch]` · `dotted [pitch]` · `invis` |
| Colour | `outlined c` (stroke) · `shaded c` (fill) · `colored c` (both) · `fill 0.9` (grey) · `opacity 0.5` **(ext)** |
| Arrowheads | `->` `<-` `<->` on lines; `arrow` = `line ->` |
| Text | `ljust` `rjust` `above` `below` · `bold` `italic` `mono` `font "Geist"` `fontsize 9` `big` `small` **(ext, bind per string)** · `rotated 90` · `aligned` (along its line) **(ext)** |
| Fit | `fit` **(ext)** sizes a box to its text |
| Layers | `behind Place` **(ext)** paints under an object (containers, lanes) |
| Hooks | `class "hot"` (CSS class on the SVG group) · `link "https://…"` (clickable) **(ext)** |
| Fills | `hatch` `crosshatch` `hatchangle 45` · `gradient "gold" "white"` **(ext)** |
| Paths | `close` **(ext)** closes a line into a polygon that can take `shaded`/`fill` |

A *colour* is a CSS name (`"seagreen"`, bare `gold`), `"#2d3142"`, the literal
`0x2d3142`, `rgb(45,49,66)`, or a variable holding one of those (`ink = 0x2d3142;
box outlined ink`). Unknown names raise an `invalid_color` warning: check `--json`.

Text objects carry their own attributes: `"API" "port 8080" mono small` sets mono+small
on the second string only. A text at a point: `"label" at B.n + (0, 0.12)`; `above`
puts the text's bottom on the point, `ljust` its left edge.

## Page

| Word | Effect |
|---|---|
| `margin = 0.1` · `leftmargin`… **(ext)** | padding around the drawing (fixes clipped edge labels) |
| `canvas from (x0,y0) to (x1,y1)` **(ext)** | fixed page: stable viewBox, blocks can translate |
| `scale = 10` | pic units per inch (author in metres, millimetres, …) |
| `.PS 6` | fit the whole picture into 6 inches (rescale) |

## Variables, macros, loops

```pic
gap = 0.4
define step { box $1 wid 1.3 ht 0.5 rad 0.06 }    # $1 … $9 are the call's arguments, $+ their count
S1: step("parse")
S2: step("check") at S1 + (1.3 + gap, 0)
arrow from S1.e to S2.w
for i = 0 to 3 do { circle rad 0.08 fill i/4 at (i * 0.3, -0.8) }
if gap > 0.3 then { "wide" at S2.s + (0, -0.2) }
"total " + sprintf("%g", gap * 2) at S1.s + (0, -0.2)
```

Labelled macro calls name what the body draws: `R1: step("x")` → `R1.n` works even
when the body starts with assignments. Functions: `sqrt sin cos atan2 log exp max min int rand`.

## Extensions worth knowing

- `dot at P` — a junction dot; `dotrad` sets its size.
- `brace from A to B up "label"` — a curly brace annotation (`down/left/right`).
- `animate <place> with "fade"` and friends — GSAP timeline alongside the SVG (`--html`).
- `texlabels = 1` or `-t` — `$…$` labels typeset as math (single backslash in pic: `"$\frac{1}{2}$"`).
- `copy "circuits"` — the circuit library in-source (same as `-c`), see `circuits.md`.
- `geo_lon`, `geo_lat`, `geo_scale`, `geo_bearing` — place the drawing on the Earth, see `maps.md`.
- Troff passthrough: a line starting with `.` in column 1 is kept aside (`.tran 1u 1m` for SPICE decks).

## Reading errors

`rpic` reports `line:col: message` with `expected …` hints; `--json` returns
`{error, error_info}` with `kind`, `found`, `expected`, `hint`. Positions are relative
to the user's own source: a prelude or `copy` file reports its own name.
