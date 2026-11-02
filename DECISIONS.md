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

### S7. Text: compass anchors over baseline points

(Implementation authority.) Each text line exports `at` (the anchor point) +
`anchor` (pic compass: `c n s e w ne nw se sw`), derived from pic's
halign/valign, plus a resolved `font {family?, size, bold?, italic?}` (size in
points, always resolved — classic 11 pt, standalone height-derived, or
explicit) and `rotation`. The exact baseline reconstruction (xheight = 0.66 em
etc.) is documented in docs/scene.md so consumers and the A2 reference renderer
reproduce the SVG baseline bit-for-bit. Discarded: exporting raw SVG baseline
points + `start|middle|end` (exact but device-flavoured; the brief asked for
pic anchors), or both (confusing double semantics).

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
