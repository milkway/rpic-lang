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
Residual: the scene's `text` omits blank lines, so `box "" "below"` puts
"below" at the centre where the SVG draws it half a line skip lower.
`pic-title` now filters `geopic:kind == "title"` and is styled exactly like
`pic-text` (anchor, rotation), still the sixth layer. Additive:
`geopic:schema` stays 1; the schema gains the kind, `geopic:for` and the
`:title` id suffix.

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
