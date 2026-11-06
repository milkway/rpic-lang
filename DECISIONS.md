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

- Plane step per segment `clamp(L/100, 1 m, 100 m)` — the prompt's
  `min(100 m, L/100)` alone would put 100 vertices on every centimetre-long
  segment; the 1 m floor keeps short segments short while a 20 km line still
  gets a vertex every 100 m (A4: ≤ 1 m from the geodesic).
- Closed curves get `clamp(⌈2πr/100 m⌉, 32, 128)` vertices.
- Coordinates are rounded to 9 decimals (≈ 0.1 mm) for stable fixtures.
- Hidden items (`move`, invisible helpers) produce no feature **and no
  warning**: they paint nothing in the SVG either, so nothing visible is lost.
  Block items produce no feature; their children carry `geopic:block`.
- Arrowheads are separate features (`geopic:kind = "arrowhead"`, same
  `geopic:id`, filled ones painted in the stroke color); a spline with several
  paths yields one feature per path (`geopic:part`).

