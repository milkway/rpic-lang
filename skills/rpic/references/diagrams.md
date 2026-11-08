# Diagram recipes

Each recipe is a complete figure that renders with the light theme
(`scripts/render.sh fig.pic --theme light`); swap `--theme dark` and add `page(w, h)`
first for dark paper. Replace the content, keep the grammar. Pick the type by what
the reader must see:

| Showing | Recipe |
|---|---|
| components and their connections | Architecture |
| steps with decisions | Flowchart |
| time-ordered messages between actors | Sequence |
| states and transitions | State machine |
| events on a time axis | Timeline |
| a process across owners | Swimlane |
| containment / zones | Nested |
| parent → children | Tree / org chart |
| stacked abstraction levels | Layers |
| two-axis positioning | Quadrant |
| ranked stages, conversion | Pyramid / funnel |
| a cycle | Loop |
| quantities by category | Bars |
| set overlap | Venn |

If a three-column table says the same thing, write the table. Above nine nodes, draw an
overview and a detail.

## Architecture

Nodes in rows, orthogonal arrows, one focal node, one accent arrow, a container drawn
behind its children (external systems stay outside it), labels slid along the arrows that
cross the container's edge, a bottom legend only if a treatment needs explaining.

```pic theme=light
margin = 0.15
Web: node("Web app" "Next.js" mono small)
Api: node("API" "axum · :8080" mono small, at Web + (2.2, 0))
Db:  focal("Postgres" "primary" mono small, at Api + (2.2, 0))
Cache: store("Redis" "sessions" mono small, at Api + (0, 1.2))
Pay: ext("Stripe" "payments" mono small, at Api + (0, -1.4))
group("BACKEND", wid 4.8 ht 2.5 at (Api.x + 1.3, Api.y + 0.6) behind Cache)
flow(Web.e, Api.w, "HTTPS", 0.3)      # 0.3: the label stays outside the container
flowhot(Api.e, Db.w, "SQL")
flow(Api.n, Cache.s, "sessions")
flowapi(Api.s, Pay.n, "webhooks", 0.75)   # external system below, outside the container
```

## Flowchart

Boxes for steps, a diamond for a decision (pic has no diamond primitive: a closed
line), yes/no labels beside the exits, one path accented.

```pic theme=light
margin = 0.15
define decision { line from $1 + (0, 0.35) to $1 + (0.7, 0) then to $1 + (0, -0.35) then to $1 + (-0.7, 0) close outlined ink shaded paper; $2 shaded ink at $1 }
S: node("Receive order")
D: (S.x, S.y - 1.1); decision(D, "In stock?")
Y: node("Reserve items", at D + (2.4, 0))
N: ext("Back-order" "notify customer" mono small, at D + (0, -1.3))
E: focal("Ship", at Y + (0, -1.3))
flow(S.s, D + (0, 0.35))
flowhot(D + (0.7, 0), Y.w, "yes")
flow(D + (0, -0.35), N.n, "no")
flowhot(Y.s, E.n)
flowopt(N.e, E.w)
```

## Sequence

Actors across the top, lifelines down, messages as arrows with the label above the
arrow, a return dashed, one activation box on the focal actor.

```pic theme=light
margin = 0.15
A: node("Client", wid 1.1)
B: node("API", wid 1.1 at A + (2.2, 0))
C: store("DB", wid 1.1 at B + (2.2, 0))
define life { line from $1.s down 2.8 outlined rule dashed 0.06 }
life(A); life(B); life(C)
Act: box wid 0.12 ht 1.0 with .n at B.s + (0, -0.5) outlined ink shaded accenttint
define msg { arrow from ($1, $3) to ($2, $3) outlined muted; $4 mono small shaded soft above at (0.5 between ($1, $3) and ($2, $3)) + (0, 0.04) }
define ret { arrow from ($1, $3) to ($2, $3) outlined muted dashed 0.05; $4 mono small shaded soft above at (0.5 between ($1, $3) and ($2, $3)) + (0, 0.04) }
msg(A.x + 0.0, B.x - 0.06, A.s.y - 0.6, "POST /orders")
msg(B.x + 0.06, C.x, A.s.y - 1.0, "INSERT")
ret(C.x, B.x + 0.06, A.s.y - 1.4, "ok")
ret(B.x - 0.06, A.x, A.s.y - 1.8, "201 Created")
```

## State machine

Rounded boxes as states, a filled dot as the initial state, transitions labelled with
`event / guard`.

```pic theme=light
margin = 0.15
boxrad = 0.12                      # rounded, never a pill
I: dot at (0, 0) rad 0.06
Idle: node("Idle", wid 1.1 at I + (1.0, 0))
Run:  focal("Running", wid 1.1 at Idle + (2.2, 0))
Done: node("Done", wid 1.1 at Run + (2.2, 0))
Err:  ext("Failed", wid 1.1 at Run + (0, -1.2))
flow(I, Idle.w)
flowhot(Idle.e, Run.w, "start")
flow(Run.e, Done.w, "finish")
flowopt(Run.s, Err.n, "error")
flowopt(Err.w, (Idle, Err.w) then to Idle.s); alabel("retry", (Idle.x + 0.3, Err.y - 0.12))
```

## Timeline

A baseline, ticks, events above and below alternating so labels never collide; the one
milestone that matters in accent.

```pic theme=light
margin = 0.2
L: line from (0, 0) to (6, 0) outlined ink thick 1.2
define tick { dot at ($1, 0) rad 0.05 shaded ink; $2 mono small shaded soft at ($1, -0.22); $3 shaded ink at ($1, 0.3) }
define tickdn { dot at ($1, 0) rad 0.05 shaded ink; $2 mono small shaded soft at ($1, 0.22); $3 shaded ink at ($1, -0.3) }
tick(0.4, "2024-01", "Kickoff")
tickdn(1.8, "2024-04", "Design freeze")
tick(3.2, "2024-07", "Beta")
tickdn(4.6, "2024-10", "GA")
dot at (5.6, 0) rad 0.08 shaded accent; "v2" shaded accent bold at (5.6, 0.3)
```

## Swimlane

Lanes as wide containers behind the steps; handoffs cross lanes orthogonally.

```pic theme=light
margin = 0.15
define lane { box wid 6.4 ht 1.0 at $1 outlined rule shaded paper2; $2 mono small shaded soft ljust at last box.w + (0.1, 0.35) }
lane((3.2, 0), "CUSTOMER"); lane((3.2, -1.1), "SUPPORT"); lane((3.2, -2.2), "ENGINEERING")
A: node("Report issue", wid 1.3 at (1.4, 0))
B: node("Triage", wid 1.3 at (3.2, -1.1))
C: focal("Fix", wid 1.3 at (5.0, -2.2))
D: node("Confirm", wid 1.3 at (5.0, 0))
flow(A.e, (B, A.e) then to B.n)
flowhot(B.e, (C, B.e) then to C.n)
flowopt(C.n + (0.3, 0), (C.x + 0.3, D.s.y))
```

## Nested (containment)

Containers inside containers; the inner box is the focal one. Draw outer first.

```pic theme=light
margin = 0.15
Cloud: box wid 5.4 ht 2.6 at (0, 0) outlined rule shaded paper2 rad 0.1
"CLOUD ACCOUNT" mono small shaded soft ljust at Cloud.nw + (0.12, -0.16)
Vpc: box wid 4.6 ht 1.8 at (0, -0.2) outlined muted shaded paper rad 0.08
"VPC 10.0.0.0/16" mono small shaded soft ljust at Vpc.nw + (0.12, -0.16)
Pub: node("Load balancer" "public subnet" mono small, at (-1.2, -0.4))
Prv: focal("App servers" "private subnet" mono small, at (1.2, -0.4))
flow(Pub.e, Prv.w)
```

## Tree / org chart

A root, a row of children, orthogonal connectors through a shared bus line.

```pic theme=light
margin = 0.15
R: focal("Platform", wid 1.3)
define child { node($1, wid 1.3 at (R.x + $2, R.y - 1.2)) }
C1: child("Data", -2.2); C2: child("Infra", -0.0); C3: child("Apps", 2.2)
Bus: line from (C1.x, R.y - 0.6) to (C3.x, R.y - 0.6) outlined muted
flow(R.s, (R, Bus))
flow((C1, Bus), C1.n); flow((C2, Bus), C2.n); flow((C3, Bus), C3.n)
define leaf { node($1, wid 1.3 ht 0.45 at ($2.x, $2.y - 0.9)) }
L1: leaf("Warehouse" small, C1); flow(C1.s, L1.n)
L2: leaf("Kubernetes" small, C2); flow(C2.s, L2.n)
```

## Layers

Stacked wide boxes, bottom = foundation; the layer the text is about in accent.

```pic theme=light
margin = 0.15
define layer { box wid 4.2 ht 0.5 at (0, $2) outlined ink shaded paper; $1 shaded ink at last box }
layer("Applications", 1.2)
layer("Services  ·  APIs", 0.6)
box wid 4.2 ht 0.5 at (0, 0) outlined accent shaded accenttint thick 1.4; "Data platform" bold shaded ink at (0, 0)
layer("Infrastructure", -0.6)
"foundation ↓" mono small shaded soft rjust at (2.1, -1.0)
```

## Quadrant

Two axes, four labelled regions, items as dots with labels beside them, one in accent.

```pic theme=light
margin = 0.2
box wid 4 ht 3 at (0, 0) outlined rule shaded paper
line from (-2, 0) to (2, 0) outlined rule; line from (0, -1.5) to (0, 1.5) outlined rule
"EFFORT →" mono small shaded soft at (0, -1.7); "IMPACT ↑" mono small shaded soft rotated 90 at (-2.2, 0)
"quick wins" mono small shaded soft at (-1.0, 1.35); "big bets" mono small shaded soft at (1.0, 1.35)
"fill-ins" mono small shaded soft at (-1.0, -1.35); "money pits" mono small shaded soft at (1.0, -1.35)
define item { dot at $2 rad 0.06 shaded muted; $1 shaded ink ljust at $2 + (0.12, 0) }
item("Search index", (-1.4, 0.9)); item("Billing v2", (0.8, 0.6)); item("Dark mode", (-1.2, -0.7))
dot at (1.2, 1.0) rad 0.08 shaded accent; "Realtime sync" bold shaded accent ljust at (1.32, 1.0)
```

## Pyramid / funnel

Horizontal bands narrowing downward, drawn as closed polygons; counts in mono.

```pic theme=light
margin = 0.15
define band { line from (-$2, $1) to ($2, $1) then to ($3, $1 - 0.5) then to (-$3, $1 - 0.5) close outlined ink shaded paper2; $4 shaded ink at (0, $1 - 0.25); $5 mono small shaded soft ljust at ($2 + 0.15, $1 - 0.25) }
band(1.0, 2.0, 1.5, "Visitors", "120 000")
band(0.5, 1.5, 1.0, "Sign-ups", "8 400")
band(0.0, 1.0, 0.6, "Trials", "1 900")
line from (-0.6, -0.5) to (0.6, -0.5) then to (0.3, -1.0) then to (-0.3, -1.0) close outlined accent shaded accenttint thick 1.4
"Paying" bold shaded ink at (0, -0.75); "310" mono small shaded accent ljust at (0.75, -0.75)
```

## Loop / cycle

Four stages around a circle with arcs; the hub holds the accumulating state.

```pic theme=light
margin = 0.2
circlerad = 0.5
Hub: circle "Backlog" at (0, 0) outlined accent shaded accenttint thick 1.4
N: node("Plan", wid 1.1 at (0, 1.6)); E: node("Build", wid 1.1 at (2.2, 0))
S: node("Release", wid 1.1 at (0, -1.6)); W: node("Learn", wid 1.1 at (-2.2, 0))
arc -> from N.e to E.n rad 1.6 cw outlined muted
arc -> from E.s to S.e rad 1.6 cw outlined muted
arc -> from S.w to W.s rad 1.6 cw outlined muted
arc -> from W.n to N.w rad 1.6 cw outlined muted
```

## Bars

Horizontal bars from a baseline, values in mono at the bar end, the bar in question
in accent.

```pic theme=light
margin = 0.2
define bar { box wid $2 ht 0.32 with .w at (0, $1) outlined ink shaded paper2; $3 shaded ink rjust at (-0.15, $1); $4 mono small shaded soft ljust at ($2 + 0.1, $1) }
bar(1.2, 3.1, "North", "3.1 M")
bar(0.7, 2.4, "South", "2.4 M")
box wid 3.6 ht 0.32 with .w at (0, 0.2) outlined accent shaded accenttint thick 1.4
"East" shaded ink rjust at (-0.15, 0.2); "3.6 M" mono small shaded accent ljust at (3.7, 0.2)
bar(-0.3, 1.2, "West", "1.2 M")
line from (0, -0.55) to (0, 1.45) outlined ink
```

## Venn

Two or three circles with translucent fills; labels in the lobes, the intersection in
accent text.

```pic theme=light
margin = 0.2
circle rad 0.9 at (-0.5, 0) outlined ink shaded paper2 opacity 0.6
circle rad 0.9 at (0.5, 0) outlined ink shaded paper2 opacity 0.6
"Design" shaded ink at (-1.0, 0.2); "Engineering" shaded ink at (1.0, 0.2)
"shipped" bold shaded accent at (0, -0.1)
```

## Reading the recipes

- `node(TEXT, ATTRS)`: the text first (two strings max, the second `mono small`), then
  ONE second argument holding every box attribute: `node("API", wid 1.1 at A + (2.2, 0))`.
  A third comma would start a third argument the macro ignores.
- A label is the arrow's third argument, a quoted string like any pic text:
  `flow(A.e, B.w, "HTTPS")`. The theme puts it just above a horizontal run and just right
  of a vertical one. A fourth argument moves it along the arrow (0 = start, 1 = end):
  use it whenever the midpoint lands on a container edge or near another node —
  `flow(Web.s, Api.n, "HTTPS", 0.3)`. For an elbow or a pair of parallel arrows, place it
  yourself with `alabel("…", P)`, `alabelw` (left of a vertical stroke) or `alabele`.
- An elbow is one `then`: `flow(A.e, (B, A.e) then to B.w)` passes the whole path as
  the macro's second argument (no auto label on an elbow: add one with `alabel`).
- Theme macros are plain pic: open `assets/editorial-light.pic` and add one when a
  recipe needs a treatment twice.
