# Design decisions

One entry per non-trivial decision: what was decided, alternatives discarded,
and why. Newest last. (Started with the scene-output feature, 2026-11-01.)

## Scene output (2026-11-01)

### S1. Public names — approved by the user

`scene` (JSON field), `rpic --scene`, `--flatten <tol>`, `--json=all` (scene
rides the compile bundle only here), Python `rpic.scene(src, flatten=…)` +
`compile(...)["scene"]`, JS `compile(src, {scene, flatten})`, C flag on
`rpic_compile_json`, R `rpic_scene()`. Alternatives discarded: `--json full`
(two-token form; `=all` matched the prompt and is a single literal match in the
arg loop, no generic `=` parsing added), `drawlist` naming.

### S2. Curves: everything is cubic Béziers — approved by the user

Splines, arcs **and arc arrowheads** all serialize as cubic Bézier segments in
`paths`; a consumer needs only moveto/cubic/lineto/close. Discarded: native
`{c,r,a0,a1}` arcs (every consumer, geopic included, would need an arc
implementation; arc heads would still need cubics) and the hybrid
cubics+arc-params (redundant, two representations to keep coherent). Cost
accepted: exact arc parameters are not in the scene (phase 2 may add them if a
TikZ backend wants exactness). Implies the byte-inert refactor of the spline
string builders in `svg.rs` into cubic-list producers (proved against the
corpus, main vs branch).

### S3. Item kinds: rich vocabulary — approved by the user

`box circle ellipse arc line arrow spline text dot brace block`: `arrow` vs
`line` derived from `Arrowheads != None` (no IR change), `dot` tagged at build
time (additive flag on `Shape::Circle`, inert), synthetic `kind:"block"` items
(id `blk<N>`, no geometry, `items` list + `label`), children carry a `block`
path. The published `objects` JSON keeps its own kinds (`path`, `circle`) —
divergence documented in docs/scene.md. Discarded: reusing the `objects`
vocabulary + `arrows`/`dot` fields (consistent but inconvenient for consumers
filtering by kind), and rich kinds without block items (loses the natural home
for a block's label).

### S4. Serialization: hand-rolled JSON, zero new runtime deps — approved by the user

Same style as `objects_json`/`animations_json` (manual writer over `json_str`/
`json_num`). The JSON Schema lives in `docs/scene.schema.json` and is enforced
by a test using a validator in dev-dependencies only. Discarded: serde in core
(first serialization dep, wasm-size risk against the ≤5% budget C3) and a
serde-behind-a-feature variant (two surfaces to maintain; can be added later
without breaking anything).

### S5. Items array is in draw order; `z` = index among drawable items

(Implementation authority.) The array is sorted by the same stable
`(layer, source index)` key the SVG emitter uses, so document order in the SVG
equals array order in the scene (acceptance A3 becomes an equality test).
Synthetic block items are interleaved immediately before their earliest-drawn
child and carry no `z`; `z` on drawable items counts drawables only, so it is
dense. Discarded: source order + explicit z (consumers would re-sort — the
scene is a *draw list*), blocks at the end of the array (grouping reads worse).

### S6. Invisible shapes stay in `items`

(Implementation authority.) `move` and invisible helper spines keep their item
(ids must stay aligned with the SVG's `s<N>` groups, which exist for every
shape) marked `"hidden": true` — set by the same visibility rules the SVG
emitter applies, so "hidden" means literally "paints nothing". An `invis` box
with a fill or a label is NOT hidden (the fill/text paint); its stroke is
simply absent. Discarded: omitting invisible items (breaks the id↔index mental
model and A1's counting) and a `kind:"move"` (a move is a line that paints
nothing; `hidden` composes better).

### S7. Text: baseline anchor point + pic compass (amended during implementation)

(Implementation authority.) Each text line exports `at` = the **baseline
anchor point** (the exact point the SVG backend uses, in model space) plus
`anchor` (pic compass `c n s e w ne nw se sw` describing the justification
semantically), a resolved `font {family?, size, bold?, italic?}` (size in
points, always resolved — classic 11 pt, standalone height-derived, or
explicit) and `rotation` (about `at`). *Amendment*: the first draft made `at`
the compass point with a documented back-conversion to the baseline, but the
conversion needs the line-grid x-height, which differs from the glyph size
when `fontsize` overrides a line — the baseline point is exact in every case
and renders with zero font metrics; the compass stays as semantic info for
box-anchor consumers (TikZ). Discarded: compass-point `at` (inexact under
`fontsize`), both points (double semantics).

### S8. Colors normalized to `#rrggbb`; anything else warns

(Implementation authority.) Scene colors pass through a normalizer: hex stays,
`rgb(v,v,v)` grays fold to hex, the 148 CSS names and the already-mapped
xcolor names fold via a static table next to `color.rs`. A value that cannot
be reduced to hex (e.g. `currentColor`, `hsl(…)` strings a user wrote
verbatim) is passed through unchanged **and** raises a `scene_unsupported`
warning with the shape's span — never silent (B5). The SVG backend is
untouched (it keeps emitting what the user wrote).

### S9. `vars` snapshot semantics

(Implementation authority.) `vars` carries every user variable (final value,
f64) and every built-in env variable under its pic name (`boxwid`, `scale`, …,
effective final values). Caveat documented: values are the end-of-eval ones;
the `.PS w h` / `maxpswid` whole-picture rescale that may run afterwards does
not retro-scale them (it never did for pic semantics either). Labels: each
shape records the label it was given at statement level (`L: box`); later
rebinding of the same name does not rename earlier shapes.

### S10. Scene is computed on demand, never on the default paths

(Implementation authority — B3 is a hard requirement.) `compile_json*` output
is byte-identical to before; the scene is built only by the new entry points
(`scene_json`, CLI `--scene`, `--json=all`, binding flags). `scene_unsupported`
warnings therefore also only exist on those paths (they are produced by the
scene builder, not by eval), keeping the default `--json` warning stream — and
the warning-clean corpus guarantee — untouched.

### S11. Named places and provenance ride the existing outputs (2026-11-06)

`Drawing.places` (#415), `Drawing.passthrough` (#414) and `BlockInfo.span`/
`call` (#416) were landed as Rust-only fields with the format decision
deferred. Decision: no new flag and no new document.

- **Compile bundle (`--json`)**: `places` (SVG user units — the `objects`
  bbox frame, so editors overlay without converting — plus the flat
  `line`/`col`/`end_col`/`file` span convention `objects` already uses) and
  `passthrough` (`text` + span). Both ride **only when non-empty**, like
  `interactions`/`scroll`: a source without `Label: position` statements or
  troff lines produces a byte-identical bundle. Sources *with* places do gain a
  key, which is the same kind of additive growth `objects` brought in 0.7.0.
  Block provenance stays out of the bundle, which has no block vocabulary.
- **Scene**: top-level `places` (scene units, `block` path, `span` object) is
  always present (empty array allowed), and block items gain `span` and
  `call`. Spans in the scene are objects (`{line, col, end_col, file?}`), the
  first span-carrying field there; §7 classes all of this as additive, so
  `scene` stays **1** and the schema/golden-key test were extended. Troff
  passthrough is source text, not drawing, so it does not enter the scene.
- Bindings pass JSON through, so Python/JS/C/R see the keys with no API
  change; the JS `.d.ts` types were extended.

### S12. Items with text carry the centre their text is laid out around (#435)

Each scene item with a `text` array also carries `text_center: [x, y]` —
the `center` the emitter hands to the text layout (`text_json`), in scene
inches: a box's/circle's/ellipse's centre, a line's path text point, a
spline's midpoint, an arc's centre, a brace's label point, a text object's
`at`. The per-line baselines are not enough to recover it: blank lines are
skipped yet still advance the stack, `above`/`below` shift the baseline by
the valign offset, `ljust`/`rjust` shift `x`. A consumer that places the
label as one block — geopic's title points (G10), a MapLibre symbol —
needs the centre plus the first line's compass. Additive under §7: `scene`
stays **1**, the schema and the golden-key test gain the key, and it rides
only on items that emit text. `--svg`/`--json` are untouched.

### S13. Text lines carry their row; items carry the stack height

pic lets a label stack hold blank lines (`box "" "below"` draws "below" one
row under the centre, `box "above" ""` one row over it). The scene emits
only the visible lines, so — even with `text_center` (S12) — a consumer
that sets the label as one block could not tell where in the stack they
sit (geopic's residual in G10). Each `text` entry now carries `row`, its
0-based index in the author's stack (blank rows included), and the item
carries `text_rows`, the stack's height — exactly the `n` the layout
centres around (`text_json` and `Svg::write_text` both lay out every line,
blank ones advance `y` by one line skip). Dropping the blank lines from
`text` stays (removing it would be a breaking change of meaning); the two
keys are the additive way to restore the information: `scene` stays **1**,
the schema/golden-key test and the JS types gain them, they ride only on
items that emit text. `--svg`/`--json` are untouched; the corpus `--scene`
changes only by the two keys.

### S14. `vars` is the top-level table; `=` inside `[ ]` stays block-local (#443)

`[ geo_lon = -34.87; geo_lat = -8.06; box ]` gave `no_registration` while the
same assignments in an `if`/`for` body registered. Not an export bug: the
evaluator scopes `=` inside a block locally (`eval/build.rs::block` evaluates
in a sub-`State` and copies back only `:=`/`+=`-style updates of inherited
variables), and that is pic semantics — the oracle (dpic 2025.08.01, `-v`)
agrees on every probe, and so does rpic:

| source | dpic | rpic |
|---|---|---|
| `[ x = 3; box ]` / `box wid x` | `Variable not found … "x"` | ``variable not found `x` `` |
| `x = 1` / `[ x = 3; box ]` / `box wid x` | wid 1 in (96 px) | wid 1 in |
| `x = 1` / `[ x := 3; box ]` / `box wid x` | wid 3 in (288 px) | wid 3 in |
| `if 1 then { x = 3 }` / `box wid x` | wid 3 in | wid 3 in |
| `for i = 1 to 1 do { y = 2 }` / `box wid y` | wid 2 in | wid 2 in |

So the scene's `vars` is, consistently, the **top-level** variable table at
the end of evaluation; block-local variables die with their block and are not
exported (exporting them would invent a global that pic itself does not
have). The engine is unchanged; the fix is the contract: docs/scene.md
(`vars`), docs/geopic.md §1 and the Maps page say "assign `geo_*` at top
level (or `:=` onto a top-level variable)", and geopic's `no_registration`
hint says the same. Tests lock both behaviours (scene, geopic-core, CLI).
`--svg`/`--json`/`--scene` are byte-unchanged; only the hint text moves.

## geopic-core (2026-11-06)

### G1. Input is the scene *document*, not engine types — approved by the user

`geopic-core` deserializes the scene JSON (format 1, `docs/scene.md`) with
serde and knows nothing of `rpic-core`. Alternatives: a `rpic-scene` crate of
shared types (a new public surface to version), or consuming `Drawing` (couples
the geo layer to eval internals). Parsing the document keeps A8 true by
construction — the engine gains no dependency — and lets every binding hand
over the scene it already has. The core is a dev-dependency only, for tests
that compile real pic sources.

### G2. Geodesics via `geographiclib-rs` — approved by the user

Karney's algorithms, pure Rust, `wasm32` clean, nanometre accuracy at any
distance; `default-features = false` drops the `accurate` summation only
polygon areas need. A home-grown Vincenty would fail near-antipodal cases
(A2 includes two) and add review surface. The `Geodesic` is a `OnceLock`
singleton (its coefficient tables are computed once).

### G3. `GeoOptions` is flat, mirroring the binding kwargs — approved by the user

`frame, anchor, scale, bearing, features, flatten, view_scale,
screen_classes`; `GeoScene { features, warnings, registration }`. Python
`rpic.geo.compile` returns `{"geojson", "warnings", "registration"}`. A nested
form was rejected: every binding would flatten it again.

### G4. The scene gains a `rescale` factor (engine PR #420) — approved by the user

`vars` are pre-rescale, geometry post-rescale, so `geo_scale` (metres per
*authored* inch) must be divided by the factor `.PS <w>`/`maxps*` applied.
Refusing rescaled drawings with a warning was the alternative; exposing the
factor (additive, format stays 1) keeps the feature available and exact.

### G5. Registration conventions (implementation authority)

- `geo_lon`/`geo_lat` is the position of the **pic origin** `(0, 0)`;
  `geo_bearing` is the azimuth of the canvas +y axis, clockwise from north.
- One control point acts as the anchor at its own canvas position (declared
  anchor ignored with `ignored_with_control_points`); two or more also fix
  scale and bearing (declared ones ignored with the same warning). The two-point
  case is the N-point least-squares solution, which is exact for two.
- The ENU fit is done about a provisional anchor and repeated once about the
  fitted anchor (the control points' canvas centroid), so the plane used for
  fitting is the plane the registration uses. The reported bearing is
  therefore relative to the fitted anchor's meridian — it differs from a
  bearing declared at another anchor by the meridian convergence.
- RMS is the geodesic distance between where each control point lands and
  where it should be (frame-independent, honest metres).
- `geo_px = 1` without `view_scale` is an error (`view_scale_required`), not
  a silent 96 px/in.

### G6. Densification and output (implementation authority)

- Plane step per segment: `⌈L / 100 m⌉` pieces — the prompt's
  `min(100 m, L/100)` puts 100 vertices on *every* segment between 100 m and
  10 km (a 4096-chord circle became 409 600 points in review) for no accuracy:
  a 100 m chord lies 0.2 mm off its geodesic, and in the Mercator frame a
  straight canvas line is already straight. A 20 km line still gets a vertex
  every 100 m (A4: ≤ 1 m from the geodesic).
- Closed curves get `clamp(⌈2πr/100 m⌉, 32, 128)` vertices.
- Coordinates are rounded to 9 decimals (≈ 0.1 mm) for stable fixtures.
- Closed curves are flattened to the `flatten` tolerance (sagitta) *and* the
  100 m chord rule, 32..=4096 vertices — the prompt's fixed 32–128 would put a
  100 km circle 30 m off itself. `geopic:rotation` is CCW from the map's east
  (canvas rotation − bearing). An open stroke with a fill yields a second
  Polygon feature (`geopic:role = "fill"`) so the painted area is never lost.
  Host features beat source control points of the same name (`-D` rule).
- Resource caps, mirroring the scene's own flatten budget: 100 000 vertices
  per segment, 2 000 000 per scene (warning `densification_budget`, endpoints
  kept), effective scale ≤ 1e9 m/in (`invalid_scale`, declared or fitted),
  `flatten` validated (`invalid_flatten`). Raised by the commit security
  review and the code review of PR 1.

### G7. `geopic-core` tracks the engine's version

It inherits `version.workspace`, like `rpic-render`/`rpic-capi`: one version
to bump, one release train, and the scene format it reads is pinned by the
same tag. Its first crates.io publication will therefore be 0.14.0, not 0.1.0
— a conscious trade of a "clean" lineage for one release checklist.
- Hidden items (`move`, invisible helpers) produce no feature **and no
  warning**: they paint nothing in the SVG either, so nothing visible is lost.
  Block items produce no feature; their children carry `geopic:block`.
- Arrowheads are separate features (`geopic:kind = "arrowhead"`, same
  `geopic:id`, filled ones painted in the stroke color); a spline with several
  paths yields one feature per path (`geopic:part`).

### G8. geopic in the browser is a third wasm artifact, not part of the lean module

Measured with the release profile (`opt-level = "z"`, LTO, `wasm-opt -Oz`):
the lean `rpic_wasm_bg.wasm` is 478 825 bytes on main; linking
`geopic-core` (geodesics, registration, densification, GeoJSON writer,
serde-derived option parsing) into it gives 666 318 bytes — **+187 493
bytes (+39 %)**, 192 → 277 KB gzipped. That is over the ~150 KB budget set
for "just put it in the lean module", and the lean build is what every
rpic.dev example and every npm consumer pays for. So `crates/wasm` gains a
`geo` cargo feature and the npm package a third artifact
(`pkg/rpic_wasm_geo*`, 665 200 bytes), built by `build:wasm:geo` and loaded
by `ready(url?, { geo: true })` — the exact mechanism of the `math` build,
including the "already initialized as X" rejection. `math` and `geo` are
mutually exclusive in `ready()` (no combined build is shipped; a page that
needs typeset math *and* GeoJSON is not a use case yet). The host-side layer
(two-pass feature injection) lives in `bindings/js/index.js`, a faithful port
of `rpic.geo`, so the wasm surface stays two functions.

### G9. MapLibre styling is data emitted by geopic-core (#432)

The Maps page's MapLibre styling (#426/#428/#429/#430 — six layers, sizes
true to the ground from `geopic.registration`, the per-collection text zoom
stop under the 255 px symbol limit) lived only in `site/src/lib/geomap.ts`.
The Studio, other sites and R/mapgl need the same map, so the styling moved
into `geopic_core::maplibre` as a pure function `layers(&FeatureCollection)
-> Vec<serde_json::Value>`, ported verbatim from `geomap.ts`'s `sizing()`
(same constants, same operation order: the engine output equals the old
site's to the last bit on macOS). The source id is fixed (`pic`). Surfaces:
`rpic --geojson --maplibre` (`{geojson, layers}`; plain `--geojson`
untouched), `compileGeoJSON(src, { layers: true })` + wasm
`maplibre_layers`, Python `geo.maplibre_layers(fc)` / `compile(…,
layers=True)`, and the zero-import JS module `@strategicprojects/rpic/maplibre`
(`attach`, `createGeoMap(maplibregl, …)` — the maplibre-gl namespace is a
parameter, so no dependency). That module also holds a JS port of `layers()`
for callers without the geo wasm; the port and the engine are both pinned to
the pre-move snapshots in `examples/geo/maplibre/` (numerically: `cos φ`
comes from the platform libm). The site's `geomap.ts` is now a thin wrapper
(worker URL, popups, hover cursor) over that module; GeoMap.astro takes the
layers from the build-time `rpic --geojson --maplibre`, the playground from
`compileGeoJSON(…, { layers: true })`.

### G10. A shape's text is its own Point feature (#435)

`pic-title` was a symbol layer on the Polygon features that carry a
`title`. MapLibre places a polygon's symbol at the centroid of the polygon
*as clipped to each vector tile*, so a box straddling a tile boundary got
its title twice (measured on rpic.dev: 2 rendered `pic-title` features at
zoom 16.38 for the callout), shifted when partly clipped — and
`text-allow-overlap`/`text-ignore-placement` keep both. No layer property
fixes that, so the engine now emits, right after the features of every
non-`text` item that carries text lines (arrowheads included), a `Point`
with `geopic:kind = "title"` at the scene's `text_center` (S12) — the
centre the SVG lays the label stack out around: a box's centre, a line's
path text point, an arc's centre, a brace's label point. Its properties
are a `text` item's: `geopic:text`, `geopic:anchor` = the first line's
compass (it encodes valign/halign, so `above` sits on top of the point,
`ljust` extends right of it — the review of #436 found that inverting the
baselines instead baked those offsets in twice), `geopic:rotation` (minus
the bearing), `geopic:font_size`; plus the shape's `geopic:z`/`label`/
`block`/`class`/`link`/`placement`; the stroke/fill keys are dropped.
`geopic:id` is `<shape id>:title` (a scene id never contains `:`, so it
cannot collide; arrowheads share their line's id because they *are* part
of its drawing, a label point is a different object) and `geopic:for` names
the shape. The shape keeps `title` (simplestyle popups, `geojson.io`);
arrowheads no longer carry `title`/`geopic:text`/`geopic:font_size`.
A scene from an engine before `text_center` falls back to the item's `at`
(closed shapes keep their labels); without either, no title point.
~~Residual: the scene's `text` omits blank lines, so `box "" "below"` puts
"below" at the centre where the SVG draws it half a line skip lower.~~
Resolved (addendum, after 0.15.1): the scene now gives each text line its `row`
and the item its `text_rows` (S13), and `geopic:text` rebuilds the stack —
`text_rows` rows joined by `\n`, blank rows as empty strings (`"\nbelow"`,
`"above\n"`, `"a\n\nc"`) — on title points and free `text` items alike,
so a block centred on `text_center` puts each line on its SVG row. The
shape's `title` keeps only the visible lines (a popup has no use for blank
rows). A scene without the keys keeps the visible-lines join. MapLibre
caveat, fixed in the layer spec: its shaper (maplibre-gl 6.16,
`symbol/tagged_string.ts`) honours leading and inner blank rows but drops
the *last* trailing `\n` — a break is only taken before a following
character — so `"above\n"` would be drawn centred. `pic-text`/`pic-title`
therefore set `text-field` to `["case", ["==", ["slice", T, ["max", 0,
["-", ["length", T], 1]]], "\n"], ["concat", T, "\u00a0"], T]` (`T` =
`["get", "geopic:text"]`): the no-break space gives the shaper a character
to break before, and its per-line trim (`/^\s+$/u` includes U+00A0)
leaves that row empty. The data (`geopic:text`) stays the pure stack; the
site's `npm test` evaluates the expression with maplibre-gl's own
style-spec (`createExpression`) on the three idioms.
`pic-title` now filters `geopic:kind == "title"` and is styled exactly like
`pic-text` (anchor, rotation), still the sixth layer. Additive:
`geopic:schema` stays 1; the schema gains the kind, `geopic:for` and the
`:title` id suffix.

### G11. Layer options at the source; placement stays with the map

A user drawing on an R `mapgl::maplibre()` map asked for larger rpic labels
and for the drawing to sit under the basemap's labels. Both are answered
where every consumer gets them the same way:

- **`text_scale` is a layer option of the engine**
  (`geopic_core::maplibre::LayerOptions { text_scale }`, `layers_with`,
  `layers_json_with(fc_json, opts_json)`, `bundle_json_with`; CLI
  `--text-scale K`; JS `layers: { textScale }` / `maplibreLayers(fc,
  { textScale })` and the port; Python `text_scale=`). It multiplies the font
  size *inside the sizing*: the `text-size` metres factor becomes `m / 72 ·
  text_scale` (folded into the constant, so ×1 is the old output to the bit
  and the snapshots in `examples/geo/maplibre/` are untouched), and `z_text`
  is computed from the scaled largest font. Capping the pixels after the
  fact instead would freeze the big labels several zooms early; with the
  scale inside, ×2 lowers `z_text` by exactly one and the 2^z law holds up
  to it. Range: finite, in (0, 16] — beyond 16× an 11 pt label is a
  176 pt one, and the 255 px ceiling would pin `z_text` at its floor for any
  realistic map. Out of range, non-numeric or unknown keys: `GeoError`
  `invalid_options` (the JSON form takes objects only — serde would read a
  struct from an array). `layers_with` returns a `Result` for that reason;
  `layers(fc)` stays infallible. Strokes and dots keep ground-true sizes:
  the request was about legibility of text, and a scaled stroke would stop
  meaning "the width on the ground".
- **`before` is not part of the spec.** Where layers go in a style is a
  property of the consumer's map (`map.addLayer(spec, beforeId)`), not of
  the layer objects; the specs stay a pure function of the collection and
  the options. The zero-import JS module takes it: `attach(map, fc,
  { before })` / `createGeoMap(…, { before })`, a layer id or `'labels'` =
  a layer whose id really is `labels` when the style has one (the sentinel
  must not shadow a real id), else the first `symbol` layer of
  `map.getStyle().layers` that is not one of ours, resolved when the layers
  are added. Only the geometry layers
  (`pic-fill`, `pic-stroke`, `pic-stroke-dashed`, `pic-dot`) go before it;
  `pic-text`/`pic-title` are appended on top — the first cut moved all six,
  and review saw the basemap's "Olinda" drawn over the drawing's own
  "Olinda": the point is shapes under the place names, never the drawing's
  labels under the basemap's (a style without symbol layers,
  or a missing id, appends on top — the latter with a console warning,
  since MapLibre would otherwise refuse the layer). rpic.dev passes
  `'labels'`. rpic-r already has `rpic_geojson()`/`rpic_sf()`/`rpic_mapgl()`;
  its next release adds `rpic_mapgl(…, before = "labels", text_scale = …)`
  (signature decided), mirroring both.

## Embedder API (2026-11-07, #425)

### E1. Raw call-site arguments are sliced from the lexed text (side table)

`MacroCall.raw_args` (index-aligned with `args`) records each argument as
the author wrote it, as `RawArg {text, span, end_line}` — the same shape as
`PassLine`, one shape for "source text with a range". The text is the
slice of the source the tokens were lexed from, from the argument's first
token to its last. Rebuilding from tokens would lose the spelling
(`1e-9`), the spacing and continuations, which is the point.

Representation: every token carries a crate-private `Copy`
`SrcRange {unit, start, end}` (char offsets), and the texts one compilation
lexes (user input, `circuits`, `copy` includes, `exec` strings) live in a
`SourceTable` on the `IncludeCtx` — created fresh by each
`parse_with_prelude`, shared by its clones (eval-time expansion of deferred
bodies), dropped with the picture. A first version put an `Arc<[char]>` of
the text on every token; measured against main (release builds, medians,
macOS arm64) it cost too much memory, so it was replaced:

| | main | `Arc` per token | side table (kept) |
|---|---|---|---|
| `Spanned` size | 88 B | 112 B | 96 B |
| peak RSS `-c` figuras/fig30 | 30.7 MiB | 36.2 MiB (+18 %) | 33.1 MiB (+8 %) |
| peak RSS `-c` figuras/fig01 | 15.4 MiB | — | 16.1 MiB (+5 %) |
| `-c --svg` dpic/man36 (#373 case, 31 runs) | 4.49 ms | 5.3 ms¹ | 4.55 ms (+1 %) |
| full corpus `-c -t --svg`, 128 files (5 runs) | 1.611 s | 2.00 s¹ | 1.691 s (+5 %) |

¹ separate, noisier session (main 4.3 ms / 1.87 s there); the RSS figures
are stable across runs.

Below the ~10 % budget, so the side table stays. Consequences:

- Positions follow the diagnostics architecture: `span.line`/`span.col` of
  the first token, `span.end_col` (exclusive) and `end_line` derived from
  the slice itself, `span.file` = `None` (user input), `"circuits"`, or the
  `copy` name. `end_line` is needed because `Span` is single-line.
- User input is lexed after the backend-preamble filter, which blanks lines
  but keeps line structure, so positions match the original source; a
  blanked troff line inside an argument would read as empty there.
- Bounded retention (security review of #439): `exec` text is built at run
  time — possibly a fresh string per loop iteration — so it is lexed
  *without* registering (unit 0); keeping each one alive for the whole
  compile would grow memory linearly past the loop caps. Its tokens carry
  no raw source (raw args then fall back to the normalized text), which is
  fine: raw args are promised for the author's own units. Registration also
  de-duplicates identical texts, so a `copy` re-run in a loop body reuses
  its unit; the table grows only with *distinct* source texts. Test:
  `re_lexed_text_does_not_grow_the_source_table` (200 iterations of `exec` +
  `copy` → 2 units).
- An empty argument (`m(a,,b)`) has `text` `""` placed just past its
  `(`/`,` (`read_args` returns one separator per argument; asserted). Tokens
  without one registered lexed text (synthesized, or `exec`
  text mixed with substituted arguments) fall back to the normalized text.
- Nested calls carry the outermost call's raw arguments, exactly like
  `args`. Exposed in the scene's block `call` (additive, format 1); the
  bundle `--json` has no block vocabulary (S11), so it does not change.

### E2. `+` continuation lines join their passthrough entry

troff continues a request with a `+` in column 1; pic users write it after
a `.` passthrough line. Such a line — directly after a passthrough line or
another continuation, and not inside a string or a `\`-continued
statement — now joins the previous `PassLine`: `text` keeps the newlines
(the embedder can re-split), `span.line` is the first line, `span.end_col`
the end of the last, and `end_line` the last line. The bundle emits
`end_line` only for continued entries, so every existing output is
byte-identical. Anywhere else a column-1 `+` keeps its parse error — in
particular right after `.PS`/`.PE` (e.g. `.PS 4` then `+ x`): those are the
picture markers handled with the backend preamble, not passthrough
entries, so there is nothing to continue ("expected an object, found +";
dpic rejects it too).
dpic 2025.08.01 rejects both forms ("Syntax error, unexpected +"): this is
an rpic leniency, inert for every source dpic accepts.

### E3. A label on a flat macro call names the group it draws

`X1: m(…)` where `m`'s expansion cannot take a label the classic way now
names the group of objects the call draws. The proposal ("wrap the
expansion in an implicit `[ ]`, without changing layout") was kept, with
one restriction found at the oracle: pic expands textually, and dpic
2025.08.01 *accepts* `Z: twob()` with `define twob { box; box }` — the label
names the first box. So a labelled call is not always a group:

- **Classic first.** For a labelled call (`Label:` or `Label[i]:` directly
  before it), the preprocessor asks the parser's own statement dispatch
  (`Parser::statement_form`, which `parse_element` itself matches on) what
  the expansion's first statement is. If it is the object form — what a
  label takes today: an object (incl. strings and `[ … ]`) or anything that
  parses as a position — the tokens are left exactly as the unlabelled
  expansion's, and the statement is what it always was. That covers every
  `[ ]`-bodied library macro (the label is the block's, no new machinery)
  and the style-macro idiom `B: mybox ht 1`.
- **Group otherwise:** the first statement is empty or any other form —
  an assignment (env vars too: `margin = 0.5`), another label (`define pair
  { A: box; B: box }`), a control or keyword statement, `{ … }`, a bare
  direction, a `%` comment, or an rpic statement (`canvas from`, `animate`,
  `class`/`link`/`draggable` statements). Only then is the expansion
  bracketed with two synthetic tokens, `Token::CallOpen`/`CallClose`, which
  the parser turns into `Stmt::LabelledCall`. Because the rule *is* the
  dispatch, a future statement form is covered without a list to update
  (review of #441: the first version used a keyword allowlist and missed
  `canvas`/`animate`/…, and removed the brackets of every classic call
  from the whole token stream — two `Vec::remove`s, O(n·k) over a `-c`
  stream with many `Q1: bi_tr(…)`; now classification costs the first
  statement's length and classic calls are never bracketed; test
  `many_classic_labelled_calls_parse_in_linear_time`). A bare direction at
  the group's end ends its statement like a newline (`at_stmt_end`:
  `Newline | Eof | CallClose`). Each group form was a syntax error in rpic,
  and dpic rejects them ("Syntax error, unexpected =", "Variable not
  found", "Place name not found") — so the extension is inert for every
  source dpic accepts. Corpus proof, main vs
  branch binaries (isolated target dirs, `cmp` differ), `-c -t`: `--svg`,
  `--json` and `--scene` 128/128 identical.
- Rejected: always grouping a labelled multi-statement call. It is the more
  uniform rule, but it silently changes `Z.c`/`Z.wid` for dpic-valid input.
  Rejected too: a per-token "which call" field on `Spanned` (+8 B per token,
  the cost E1 had to fight); two marker tokens exist only around labelled
  calls.

Semantics of the group (`Stmt::LabelledCall`, evaluated in place):

- Objects, `Here`, the direction, variables and the inner labels are exactly
  the unlabelled call's — no sub-scope, no translation. The inner labels stay
  global (as in any flat expansion) *and* the ones the call binds
  **directly** become the group's members, so `X1.T` and `B.X1.T` resolve
  like a block's sub-labels; a nested labelled call's labels are that
  call's (`H.G.T`, not `H.T`), the same scoping as nested `[ ]` blocks.
- Members are **indices** into the evaluator's `placed`, collected from a
  per-call frame that `bind_label` fills (no copy of the label map, no deep
  copy of member records). Security review of #441: the first version
  cloned the whole label map per call and deep-copied every label bound
  below, again at each nesting level — super-linear in depth × labels.
  Now the work per call is its own direct labels; a record is turned into
  a self-contained copy (`State::detached`, cost = its own subtree) only
  where it crosses a `[ ]` boundary, the same place blocks already copy
  their members. Each group also counts toward `max_shapes` (structured
  error "drawing exceeded N shapes (labelled macro calls count toward the
  budget)"). Regression tests: `labelled_group_members_are_direct_and_by_index`
  (depth 8, per-level member counts) and `nested_labelled_groups_stay_linear`
  (26 depth-8 trees × 40 labels per level, 8 320 boxes: 0.05 s debug).
- Geometry: the box is the union of the group's layout extent (invisible
  helpers included, as `[ ]` measures a block — the evaluator's
  `layout_bbox` is swapped out around the body), so `.c`/`.n`/`.wid`/`.ht`
  read like a block's; `.start`/`.end` are exactly the first drawn object's
  start and the last drawn object's end. That is **not** "the terminals" in
  general: an element that draws decorations last ends elsewhere (`led`'s
  last object is its emission arrow, so `X.end` is the arrow tip, not its
  second point). Library bodies are not reordered (corpus byte-freeze);
  wire to the element's own point arguments or to labels it defines.
- Counting: a synthetic `Placed` (`kind` Block, `synthetic: true`) that
  `nth_index` skips — `last []`, `2nd []` and untyped `last` count exactly
  the objects they counted before. (Named places stay in `placed` as they
  always were; they are not objects of the group.)
- Directives on the label treat it as a block: `animate`/`draggable` act on
  its first shape, `stagger` fans across its shapes, and whatever refuses a
  block today (`class` on a block) refuses the group alike.
- Scene: a `BlockInfo` with `label`, `shapes` = the group's range, `span`/
  `call` of the **outermost** call (same convention as every block, S11/E1;
  for the usual top-level call that is the labelled call itself, raw args
  included). It adopts the parentless blocks, shapes and places the
  expansion produced. The bundle `--json` has no block vocabulary (S11) and
  is unchanged.
- Errors: a labelled call that draws nothing → `label_on_empty_call`, "label
  `X1` on a macro that draws nothing", at the label, with a hint naming the
  macro. Text after the call on its line (`X1: m() dashed`) is a parse error
  in the group form — there is no single object for it to modify.

The embedder that motivated it (dune) also copied the geometry of
`switch`, `nmos`, `pmos`, `njfet`, `pjfet` because its prelude *shadows*
those names and a shadowing macro cannot call the library's macro of the
same name. Labels alone do not fix that, so those five got `__name` geometry
twins like `__resistor`: bodies renamed in place, the public one-line
definitions appended at the end of `circuits.pic` so no line moves (object
spans point into the bodies; corpus `--json` identical).

### E4. Embedder preludes are named source units, like `-c` (#450)

`CompileOptions.preludes: Vec<Prelude { name, text }>` splices each prelude
as its own lexed unit — after `circuits` and the `texlabels` initializer, in
order, before the user's source — through the same `splice_unit` path the
circuit library takes. Consequences, all inherited rather than built:

- **User lines never shift.** The user's text is lexed separately, so every
  position-bearing key (diagnostics, `objects`, `places`, `passthrough`,
  scene `span`, `MacroCall.raw_args`) is relative to it with no rebasing.
  The Studio's hand-rolled rebase of a prepended dune prelude
  (rpic-studio#17/#18) — and every future key it would have missed — goes
  away.
- **Positions inside a prelude carry `file = name`** (the token's unit
  name), exactly as library positions carry `"circuits"`. Troff
  passthrough lines, `verbatimtex` and `\psset` lines in a prelude are set
  aside as in the user's text (the Studio's prepending did the same), their
  spans tagged with the prelude's name; prelude entries precede the user's.
- **Call-site provenance is the user's.** A prelude macro called from user
  code records the outermost call (`BlockInfo.span`/`call`, `raw_args`),
  which is the user's — the E1/E3 machinery does not care which unit the
  `define` came from.

Names are validated before parsing: non-empty, unique, no `"` or line break
(a name is echoed into JSON and `L:C:` messages), and not a reserved name:
`circuits` and `<texlabels>` (an embedder unit must not be mistaken for the
library or the `-t` initializer) and `features` — the `file` the JS and
Python geo bindings stamp on positions inside the feature lines they inject
(docs/geopic.md). That sentinel was *reserved*, not renamed to
`<features>`: it is documented, released behaviour (0.14.0) that hosts may
match on, and reserving it costs nothing. A bad name is a structured compile error with kind
`invalid_options`, the kind geopic already uses for bad host options.
`IncludePolicy` is unaffected: the text is given, not read; a `copy` *inside*
a prelude obeys the policy like any other.

Surfaces: Rust (`CompileOptions::with_prelude`, `parse_with_options`), CLI
`--prelude NAME=FILE` (repeatable; the CLI reads the file — the user named
it, so it is policy-free), JS `compile(src, { preludes })` /
`compileGeoJSON(src, { preludes })` (new wasm `compile_preludes`; the
`compile_geo` options JSON gains `preludes`), Python `preludes=` on every
entry point and `rpic.geo`, C `rpic_compile_json_opts(src, opts,
opts_json)`. The C ABI gained a function rather than a field: `RpicOptions`
is a `#[repr(C)]` struct, and growing it would break every compiled caller;
the options JSON is extensible without another ABI change. Rejected: a
single `prelude: Option<…>` (the list subsumes it) and text prepending in
the bindings (the very thing this replaces). With no preludes every path is
the old one: corpus proof, main vs branch binaries (isolated target dirs,
`cmp` differ), `-c -t`: `--svg`, `--json` and `--scene` 128/128 identical.
