# Scene internals: what the SVG backend consumes today

This is the mapping document for the **scene output** feature (the
device-independent, versioned JSON description of the resolved draw list —
rpic's analogue of classic pic emitting troff `\D` commands). It describes the
internal structure the SVG backend consumes at the moment it serializes, what a
scene serializer can read off directly, and what needs one extra step. It was
written *before* the implementation; `docs/scene.md` holds the public format.

## 1. Pipeline and the tap point

```
source → lexer → parser (ast::Picture) → eval → ir::Drawing → svg::to_svg → String
                                                      ├──────→ rpic_render::to_png / to_pdf (via the SVG string)
                                                      └──────→ objects_json / animations_json (lib.rs)
```

The scene taps the exact same place the SVG backend does: the finished
[`ir::Drawing`](../crates/core/src/ir.rs). Two whole-picture transforms run
**inside `eval_with_limits` after shape placement and before the `Drawing` is
returned** (`eval.rs`):

- `apply_ps_size` — `.PS <width> [<height>]` uniform rescale;
- `clamp_to_maxps` — `maxpswid`/`maxpsht` page-fit downscale (never up).

Both mutate every shape's coordinates in place (`scale_shape`), plus
`prelude_thick`, `canvas_margin` and `canvas`. So by the time `to_svg` (or the
scene serializer) sees the `Drawing`, **all geometry is final** — in absolute
pic model space: units = inches, y pointing **up**. No further geometric
resolution happens in the backend except the y-flip + offset into SVG pixel
space (§6.1).

PNG and PDF are downstream of the SVG string (resvg / svg2pdf in
`crates/render`); they never see the `Drawing`. A scene emitter therefore has a
single producer to be faithful to: `svg::to_svg`.

## 2. The `Drawing` struct

`Drawing` is a flat shape list plus index-aligned parallel arrays:

| Field | Type | Scene relevance |
|---|---|---|
| `shapes` | `Vec<Shape>` | the items, in **source order** (not draw order) |
| `shape_layers` | `Vec<i32>` | `behind` levels; draw order = stable sort by `(layer, index)` |
| `shape_classes` | `Vec<Option<String>>` | `class` extension, validated CSS class string |
| `shape_links` | `Vec<Option<String>>` | `link` extension, validated URL |
| `shape_spans` | `Vec<Option<Span>>` | source span of the producing statement (line/col/end_col/file) |
| `bbox` | `Bbox` | painted content bounds (model space) |
| `prelude_thick` | `f64` | global `linethick` in **points**; backend uses it only as `pad` |
| `canvas_margin` | `CanvasMargin` | margin extension, inches per side |
| `canvas` | `Option<Bbox>` | `canvas from … to …` fixed page, model space |
| `anims`, `interactions`, `anim_scroll` | | **not** scene material — stay in `animations`/`interactions` |
| `diagnostics` | `Vec<String>` | pic `print` output |
| `warnings` | `Vec<Diagnostic>` | the channel `scene_unsupported` warnings will ride |

### 2.1 Where the `s<N>` ids are born

Nowhere in the IR: `s<N>` is simply **the index into `Drawing::shapes`**,
stamped by the SVG render loop as `<g id="s{i}">`. Every shape gets a group,
including invisible ones (`move`, `invis` spines) — `objects_json` reports
`bbox: null` for those but the group exists. The animation manifest
(`"id":"s0"`), `interactions`, and `objects` all use the same convention, and
`behind` deliberately reorders *emission* without renumbering (ids stay stable;
tested by `behind_sets_render_layer_without_changing_shape_indices`).

Scene consequence: `items[k].id = "s" + shape_index` is free; the scene's `z`
(index in draw order) is the position after the `(layer, index)` sort — the
same loop `svg.rs::render` runs.

## 3. The `Shape` variants

All coordinates inches, y-up, absolute. Text rides *inside* each shape as
`Vec<TextLine>` (attached labels), not as separate shapes.

| Variant | Geometry fields | Notes for the scene |
|---|---|---|
| `Box` | `c` center, `w`, `h`, `rad` (corner radius) | `fit` already folded into `w`/`h` (§5.1) |
| `Circle` | `c`, `r` | **`dot` is a `Circle`** built with `dotrad` and a solid fill — the IR does not remember dot-ness (§7.5) |
| `Ellipse` | `c`, `w`, `h` | |
| `Path` | `pts: Vec<Point>`, `closed: bool`, `arrows` | line / arrow / `move` (invisible); `close` already folded into `closed` |
| `Spline` | `pts`, `tension: Option<f64>`, `arrows` | **control points, not béziers** — conversion is backend work today (§6.4) |
| `Arc` | `c`, `r`, `a0`, `a1` (radians), `cw`, `arrows` | rendered as one SVG `A` command (§6.5) |
| `Brace` | `a`, `b`, `cubics: Vec<[Point;4]>`, `label_at` | cubics **precomputed in the IR** — serializes directly |
| `Text` | `at`, `bbox`, `w`, `h`, `standalone` | standalone `"text"` objects; layout semantics in §6.6 |

`shape_kind` in `svg.rs` already defines stable lowercase kind names used by
`objects`: `box circle ellipse path spline arc brace text`. The prompt's
`line`/`arrow`/`dot` distinctions do **not** exist in the IR (an arrow is a
`Path` with `arrows != None`; a dot is a filled `Circle`) — schema decision.

### 3.1 `Style` (per shape)

| Field | Unit / domain | Resolution state |
|---|---|---|
| `stroke: Option<String>` | CSS color string | **resolved but not normalized**: `rgb()`/`0x…`/xcolor forms arrive as hex (`#ff8c00`), but *named* colors (`red`, `Goldenrod`→already mapped) pass through as written and are only *validated* (`color.rs` classifies, never maps). `None` = black. Scene-wants-hex ⇒ needs a name→hex table (§6.8) |
| `fill: Option<Fill>` | `Gray(0..1, 0=black)` or `Color(String)` | gray→`rgb(v,v,v)` conversion lives in the backend (`fill_value`) |
| `hatch: Option<Hatch>` | angle °, `sep` in, `width` pt, color | descriptor-ready |
| `gradient: Option<Gradient>` | from/to colors, angle ° (pic convention: 0=→, 90=↑) | descriptor-ready |
| `fill_opacity: Option<f64>` | 0..1 | emitted only when a fill/hatch/gradient exists (`fill_opacity_attr`) |
| `fill_open: bool` | | open path/spline/arc also paints its area |
| `dash: Dash` | `Solid` / `Dashed(len_in)` / `Dotted(Option<gap_in>)` | **descriptor**; the concrete `stroke-dasharray` numbers (`7/6·w`, `5/6·w`; dotted `0.5,gap` + round caps, default gap = 5×thickness px) are computed in `Svg::stroke` (§6.7) |
| `thick: Option<f64>` | **points** (default 0.8 pt) | |
| `invis`, `invis_bounds` | | visibility rules in §6.2 |
| `arrow_ht`, `arrow_wid` | inches | arrowhead dimensions |
| `arrow_filled` | | filled triangle (`arrowhead=2`) vs two-stroke open head |

### 3.2 `TextLine` (per attached/standalone line)

`s` (string), `math: Option<MathSpan>` (texlabels: pre-typeset `<svg>` fragment
+ width/height/depth in inches), `halign` (-1/0/+1 = ljust/center/rjust),
`valign` (+1/0/-1 = above/center/below), `text_offset` (in), `bold`, `italic`,
`family: Option<String>`, `size_pt: Option<f64>` (None = classic sizing),
`rotate: Option<f64>` (degrees CCW, about the line's anchor). `aligned` is
already resolved into `rotate` by eval (§5.2) — a `true` surviving here only
means the host segment had no angle.

## 4. Resolution ledger — what is already done by `Drawing` time

| Feature | Where resolved | Scene reads it as |
|---|---|---|
| `fit` | `eval/build.rs` object build (`fitted_text_size` → folds into `w`/`h`) | final `w`/`h` — nothing to do |
| `rotated` | parser/eval → `TextLine::rotate` (degrees CCW) | `rotation` field |
| `aligned` | eval of linear objects (`eval.rs` ~620): segment angle → `TextLine::rotate` | same |
| `close` | eval → `Path::closed` | `closed` flag |
| `behind` | eval → `shape_layers`; order applied by the backend sort | `z` after the same sort |
| blocks | **flattened**: children are plain shapes; `[ … ]` translate applied during eval | block path needs new data (§7.3) |
| `.PS w h`, `maxps*`, `scale` | `apply_ps_size`/`clamp_to_maxps` mutate all coordinates | nothing to do |
| colors (`rgb()`, `0x…`, variables, xcolor names) | eval → hex strings in `Style` | pass through; CSS *names* still named (§6.8) |
| `canvas from…to`, `margin*` | `Drawing::canvas` / `canvas_margin` | header fields |
| `class` / `link` | `shape_classes` / `shape_links`, validated | pass through |
| `dot` | becomes a filled `Circle` with `dotrad` | kind is `circle` unless we record dot-ness |
| texlabels math | `TextLine::math = MathSpan{svg,width,height,depth}` | outline option can embed `svg`; metrics always available |

## 5. Notes on two of those

### 5.1 `fit`

`fit` is applied in `eval/build.rs` (~line 183): the measured text box
(`fitted_text_size`) replaces `w`/`h` before the shape is pushed. The IR keeps
no memory of it — the scene sees only the final dimensions, which is what we
want ("geometria resolvida").

### 5.2 `aligned`

`TextLine::aligned` is a *request* flag set during text collection; the linear
object's eval computes the segment angle and writes `rotate` (only when
`rotate` is `None`). By `Drawing` time `rotate` is authoritative.

## 6. What only exists in the SVG backend (the "one extra step" list)

These are computations `svg.rs` performs during emission. For each: whether the
scene serializes the IR form directly, replicates the computation, or warns.

### 6.1 Coordinate transform (scene: *not* replicated)

`Svg::p` maps model → pixel: `x' = (x − west + 2·pad + margin.left)·96`,
`y' = (north − y + pad + margin.top)·96`, where `west`/`north` come from the
canvas bbox (`d.canvas` or the painted bounds), `pad = prelude_thick/144` and
`PPI = 96`. The root `width`/`height` add `6·pad + margins`.

The scene stays in model space (inches, y-up) and instead publishes the frame
in its header: canvas `w`/`h`, margins, and the model-space origin offset — the
exact inputs of `p()` — so any consumer can reproduce the SVG mapping (and the
A2/A3 tests can cross-check against `objects`, which is in SVG user units).

### 6.2 Visibility rules (scene: replicated, trivially)

- Closed shapes: drawn iff `closed_shape_is_visible` = `!invis || fill/hatch/gradient present`
  (an `invis` box with a fill still paints the fill, stroke `none`).
- Open shapes: stroke iff `!invis`; additionally an area iff `fill_open` and a
  fill exists (emitted as a separate zero-stroke polygon/path *before* the
  stroke — the scene models this as one item with both descriptors).
- Attached text is drawn regardless of `invis` (an `invis` box's label shows).
- `move` = invisible `Path` (`invis`, `invis_bounds`): a real `s<N>` group
  exists but nothing paints. Scene: keep the item (ids must align) with a
  `hidden`/`invisible` marker — exact spelling is a schema decision.

### 6.3 Arrowheads (scene: replicated — model-space helpers already exist)

The backend derives, per decorated end: the head polygon **and** a shortened
shaft endpoint (so the stroke doesn't poke through a filled tip).
Crucially, `svg.rs` already contains **model-space twins** of every helper,
added for `objects` bounds: `filled_arrowhead_points_model`,
`open_arrowhead_points_model`, `path_stroke_points_model`,
`arc_head_geometry` (model-space by construction). The scene calls these as-is:

- `heads`: filled → triangle `[left_full, tip, right_full]` (fill = stroke
  color, no stroke); open → polyline `[l, p, r]` stroked like the shaft.
- the item's own path uses the *shortened* points (`path_stroke_points_model`),
  matching what the SVG strokes.

Arc arrowheads are heavier: the filled head is a path of arc segments
(`arc_arrowhead_path`), and the remaining arc is re-fit between adjusted
endpoints (`arc_angle_between`). The geometry struct (`ArcHead`) is
model-space; only the path *string* assembly is pixel-space. The scene needs a
small refactor to assemble the same outline in model space (cubics after
arc→bézier conversion, §6.5) — one extra step, no new math.

### 6.4 Splines → cubic béziers (scene: replicated in model space)

`Spline` stores control points + tension. `classic_spline` (dpic's fixed
fractional control construction) and `tensioned_spline` are **affine**
constructions — `spline_path` comments note they're computed directly in SVG
space *because* affine transforms commute. Running the same functions on model
points is therefore exact. Needed step: variants returning
`Vec<[Point;4]>` cubic segments instead of a `d` string (the string builders
become thin wrappers — no behavior change to the SVG backend). The `--flatten`
option then subdivides those cubics (tolerance in inches, same De Casteljau
tolerance the PNG path takes through resvg).

### 6.5 Arcs (scene: decision needed)

The backend emits one elliptical-arc command (`A r r 0 large sweep x y`).
Options for the scene: keep `{c, r, a0, a1}` as a first-class arc item, or
convert to cubics like everything else. (Conversion arc→cubic is standard,
≤ 0.5 ulp-ish error with 90° segments; it is also what the §6.3 arc-head
realisation needs.) Schema decision — see the consultation.

### 6.6 Text layout (scene: partially replicated)

`write_text` computes per line: baseline position (lineskip stack centered on
the shape's text point — `n − 1 + 0.66` line grid), x from
`halign`+`text_offset` (standalone objects use `w/2` instead of the offset),
SVG `text-anchor` from `halign`, font size (classic: 11 pt attached;
standalone: derived from the object height `h/(n−1+0.66)·72`; explicit
`size_pt` wins), and the `rotate(−deg x y)` transform. Math lines place the
`MathSpan` fragment by its ink box instead.

Scene choice (per the brief): text stays **text** — `at` + `anchor` + font
descriptor + rotation, no baseline math exported. The scene must still
replicate the *baseline/anchor computation* so `at` is the true anchor point of
each line (a multi-line label becomes either one item with per-line entries or
the anchor of the whole stack — schema decision). The `outline` flag would run
the estimator's glyphs through a font (and embed `MathSpan.svg` paths for math)
— only when asked.

### 6.7 Dash, thickness, gray fills (scene: descriptor + resolved values)

- `thick_px`: points → px at 96/72; default 0.8 pt. The scene reports width in
  **points** (device-independent) — consumers scale.
- Dash: the scene publishes the descriptor (`dashed`/`dotted` + base length)
  *and* nothing else; the concrete dasharray derivation (`7w/6, 5w/6`; dotted
  `0.5px, gap`) is documented in `docs/scene.md` so a consumer can match the
  SVG exactly. (Alternative: also emit the computed pattern — schema decision.)
- `Fill::Gray(g)` → `rgb(v,v,v)`, v = round(255·clamp(g)) — replicated (pic
  convention 0 = black).

### 6.8 Color normalization (scene: new, small)

`Style` colors are strings that may still be CSS *names* (`red`, `thistle`) —
`color.rs` validates but never maps. The brief wants hex. Needed step: a
name→hex table for the 148 CSS names (+ `none`/`transparent`; `currentColor`
has no hex — warn or pass through). dvips/xcolor names are already mapped to
hex by eval. The table lives next to `color.rs`; the SVG backend is untouched.

### 6.9 Hatch/gradient pattern defs (scene: descriptors, direct)

The backend manufactures `<pattern>`/`<linearGradient>` defs with generated ids
(`hatch0`, `grad0`) and the gradient-under-hatch underlay trick (#273). None of
that enters the scene: `Hatch`/`Gradient` serialize as descriptors
(`{cross, angle, sep, width, color}` / `{from, to, angle}`), and the underlay
combination is just both descriptors present on one item.

### 6.10 Animation-only emission (scene: excluded by design)

`type`-effect targets get their label split into `.rpic-ch` tspans; `<a>` link
wrappers ride outside the group; `class` lands on the `<g>`. Links and classes
are scene fields; the tspan splitting is animation plumbing and stays out
(`animations` unchanged).

## 7. Data the scene needs that is dropped before `Drawing` today

These never reach the IR; each needs an additive field on `Drawing` (or a side
struct) filled at the end of `eval_with_limits`. All additions are inert for
existing backends (they read named fields).

1. **`vars`** — `State::vars` (user variables, `HashMap<String, f64>`, final
   values; this is where `geo_lon`/`geo_lat` live) and `State::env` (the ~30
   built-ins of `token::EnvVar` — `boxwid`, `scale`, … — with effective final
   values). Both are dropped at the end of eval. Caveat to document: variable
   values are *pre*-`.PS`/maxps rescale (the rescale happens after eval and
   does not touch vars) — fine for geo-metadata, documented for everyone else.
2. **Labels** — `State::labels` maps label → `placed` index, and
   `Placed::shape: Option<usize>` links to the primary shape. Dropped today.
   Needs a `shape → label` export (first label wins / last wins: pick and
   document; labels can be reassigned in pic).
3. **Block structure** — blocks are flattened; `Placed::block_shapes`
   holds the child shape range `[lo, hi)` and `members` the inner labels, but
   none of it survives into `Drawing`. The scene's `block` path (and a `block`
   item kind, if adopted) needs eval to record, per shape, the stack of
   enclosing blocks (cheap: a running block-path stack at `push_shape` time).
4. **Source name** — the CLI knows the input path; `compile_*` does not. The
   scene's `source` field is a `CompileOptions` addition or a CLI-level splice.
5. **Dot-ness** — a `dot` becomes a plain filled `Circle`; if the scene wants
   `kind: "dot"` the builder must tag it (one bool on `Shape::Circle` or a
   parallel marker — schema decision; `objects` reports `circle` today and
   must not change).

## 8. Summary: direct vs one-extra-step

**Serializes directly from the IR:** box/circle/ellipse geometry, path points +
`closed`, brace cubics, arc `{c,r,a0,a1,cw}`, style descriptors (fill, hatch,
gradient, opacity, dash kind, thickness, arrow dims), text strings + align +
font attrs + rotation, class, link, spans, layers (→ `z`), canvas/margins,
bbox, warnings channel.

**One extra step (all with existing model-space code or pure functions):**
arrowhead polygons + shaft shortening (§6.3, helpers exist), spline → cubics
(§6.4, refactor string builders to cubic lists), arc heads + trimmed arc
(§6.3/6.5), text anchor computation (§6.6), color name → hex (§6.8), gray →
rgb (§6.7), flatten tolerance subdivision (§6.4).

**New recording in eval (additive, inert):** vars + env values, labels,
block paths, optional dot marker, source name (§7).

**Explicitly out:** animations, interactions, scroll hint (stay in their own
manifests); the SVG coordinate transform (header carries its inputs instead).

## 9. Compatibility constraints observed

- `Drawing` is constructed by literal only inside the crate (`eval.rs` and one
  `svg.rs` test) — additive fields are safe here, but **rpic-r** consumes
  rpic-core from crates.io and an additive struct change has broken it before
  (CompileOptions, 0.6.2): check `~/Github/rpic-r` after release as usual.
- `objects_json` / `animations_json` / the SVG emitter must stay byte-identical
  for the whole corpus (acceptance B3) — the scene only *reads* `Drawing`; the
  only backend-adjacent refactor allowed is the spline-string→cubic-list split
  (§6.4), which must be proven byte-inert (main vs branch, 124 files,
  `-c -t --svg` + `--json`).
- Warnings: `Diagnostic { kind: "scene_unsupported", … }` fits the existing
  structured-warnings channel unchanged; the corpus is warning-clean and must
  stay so for default compiles (scene warnings only materialize when a scene
  is requested — or they'd show up in `--json` warnings for everyone).
