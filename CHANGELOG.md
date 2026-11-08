# Changelog

All notable changes to **rpic** are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each release is also archived on Zenodo under the concept DOI
[10.5281/zenodo.21209915](https://doi.org/10.5281/zenodo.21209915), which always
resolves to the latest version.

## [Unreleased]

## [0.18.0] - 2026-11-08

### Fixed

- A repeated size word keeps its **last** value, as in dpic: `box wid 1 wid 2`
  is 2 in wide (it kept the first), so a macro's default `wid` now yields to the
  `wid` a caller appends.
- A free text stack whose later lines carry a smaller size (`"Name" "sub" mono
  small`) kept the first line at 9 pt: the line skip now counts each further
  line by its own size factor, so the unsized name stays at 11 pt. Unsized
  stacks and single lines are unchanged (shared `stack_divisor`).

- A free text of several lines (`"two" "lines" at P`) squeezed its lines to fit
  `textht × n` (8.7 pt for two, 8.2 pt for three); dpic keeps every line at
  11 pt one em apart. The object's default height is now `textht` for the
  first line plus one em per further line, which reproduces dpic's baselines
  exactly; a single line is unchanged (corpus: one figure moves).

### Added

- Text objects take colour the dpic way: `"note" shaded "gray"` fills the
  glyphs, `outlined c` strokes their outline, `colored c` does both (colour
  names, `#hex`, `0xRRGGBB`, `rgb()` and variables, as for shapes). A shape's
  colour still never reaches its label. SVG output is byte-identical when
  unused (corpus 128/128); scene text lines gain additive `color`/`stroke`
  keys (DECISIONS S16).

## [0.17.1] - 2026-11-08

### Fixed

- Macro substitution kept no provenance for a `"$n"` string or a `$+` in the
  body: the rebuilt token lost its `file` and `end_col`, so a `"$1"` label
  inside a library or prelude macro surfaced in `objects`/`shape_spans` as a
  *user* position at the library's own line — invisible under text
  concatenation, visible with 0.17.0's named preludes (found by the Studio).
  The token now keeps the body token's `file`, `end_col` and call site. SVG
  unchanged (corpus 128/128); `--json` `objects` gain the correct `end_col`
  (26 corpus files) and `file` (#461).

### Added

- CLI `rpic --version` / `-V` prints `rpic X.Y.Z` (the crate version) to
  stdout and exits 0; listed in `--help`. Until now the binary had no version
  flag, so after a `brew upgrade` only the package manager knew what was
  installed (#458).

## [0.17.0] - 2026-11-08

### Added

- Named preludes for embedders (#450, DECISIONS E4):
  `CompileOptions.preludes: Vec<Prelude { name, text }>` splices each one as
  its own source unit — like the `-c` library — after `circuits`/`texlabels`,
  in order, before the user's source. User positions never shift; positions
  inside a prelude carry `file` = its name (passthrough included); a prelude
  macro called from user code keeps the user's call-site provenance. Bad
  names (empty, duplicate, with `"` or a line break, or the reserved
  `circuits`, `<texlabels>`, `features`) are `invalid_options` errors. Exposed as CLI `--prelude NAME=FILE`
  (repeatable; a relative FILE resolves against the input's directory), JS `compile(src, { preludes })` and `compileGeoJSON`
  (wasm `compile_preludes`, `compile_geo` options key `preludes`), Python
  `preludes=` on every compile function and `rpic.geo`, C
  `rpic_compile_json_opts` (options JSON), and Rust
  `CompileOptions::with_prelude` / `parse_with_options`. Byte-identical
  output when unused.
- `--json` bundle `places` carry `block` (#453, DECISIONS S15): a place
  defined inside `[ … ]` names its enclosing block path, outermost first,
  in the scene's vocabulary (`["blk0", "blk1"]` — exactly what `--scene`
  `places` carry). Top-level places are unchanged byte for byte. Python
  `compile()` and the JS bundle pass it through; the JS `Place` type gains
  `block?: string[]`.
### Docs

- Math labels on wasm are documented as the sanctioned path (#449): `docs/tex-labels.md` § On wasm
  covers the three npm artifacts and their measured sizes (math: 3.05 MB, 1.15 MB gzipped), the
  one-build-per-module rule of `ready()`, lazy loading, the embedder recipe (`rpic-render`'s `math`
  feature + `set_math_renderer` at init) and the literal-text fallback. The JS README matches.

### Fixed

- JS `@strategicprojects/rpic/maplibre`: `attach(map, fc, { before: 'labels' })`
  (and `createGeoMap`) resolved `'labels'` to the style's *first* `symbol`
  layer. On CARTO positron that is `waterway_label` at index 13, with 50+
  road, building and boundary layers after it, so the drawing was buried
  under them. `'labels'` is now the start of the style's trailing symbol
  block — the first layer after the last non-`symbol` layer (index 68 on
  CARTO); on top when there is none; a layer named `labels` still wins
  (DECISIONS G11 addendum, `docs/geopic.md` § 6).
- JS `@strategicprojects/rpic/maplibre`: the drawing's labels used the specs'
  placeholder font `["Noto Sans Regular"]`; CARTO's glyph server serves a Noto
  with other vertical metrics, so a label sat centred on the line it annotates
  instead of beside it. `attach`/`createGeoMap` take `font`: `'style'`
  (default) = the most common Regular/Medium `text-font` stack of the style's
  own symbol layers (with none, the most common of all; the spec's when there
  is none), `'spec'` = the placeholder, or an
  explicit stack; applied to copies of `pic-text`/`pic-title` (DECISIONS G11
  second addendum, `docs/geopic.md` § 6).

## [0.16.1] — 2026-11-08

### Fixed

- geopic: `geo_*` assigned with `=` inside a `[ … ]` block gave a bare
  `no_registration`. That is pic scoping (a `=` inside a block is local, as in
  dpic — verified against the oracle), so the engine is unchanged; the
  `no_registration` hint, docs/geopic.md, docs/scene.md (`vars` = the
  top-level table) and the Maps page now say to assign them at top level, or
  with `:=` onto an existing top-level variable (DECISIONS S14, #443).
- Python: `rpic.compile()` now carries every key of the `--json` bundle
  (#444). It built its dict by hand and dropped the objects' `"link"`
  (`link "<url>"`, #358), the top-level `"interactions"` array
  (`draggable`, #331) and the optional animation keys `"unit"` (`type … by
  word`), `"chars"` (`scramble … by`), `"wiggles"`, `"drawFrom"` and
  `"drawTo"` (partial `draw from … to …`). Each rides only when set, as in
  the CLI bundle and the JS binding; existing keys keep their shapes.

## [0.16.0] — 2026-11-08

### Added

- geopic → MapLibre: a `text_scale` layer option multiplies every label's
  font size (finite, in (0, 16]; default 1 = today's output to the bit); the
  upper `text-size` zoom stop is chosen from the scaled font, so labels still
  fit MapLibre's 255 px limit. Engine `LayerOptions` + `layers_with` /
  `layers_json_with` / `bundle_json_with`; CLI `--text-scale K` (with
  `--geojson --maplibre`); JS `compileGeoJSON(src, { layers: { textScale } })`,
  `maplibreLayers(fc, { textScale })` and the port's `layers`/`sizing(fc,
  { textScale })`; Python `geo.maplibre_layers(fc, text_scale=)` and
  `geo.compile(…, layers=True, text_scale=)` (DECISIONS G11).
- JS `attach(map, fc, { before })` / `createGeoMap(…, { before })`: add the
  pic geometry layers under a given layer, or under the basemap's labels
  with `'labels'` (the first `symbol` layer of the style); the drawing's own
  labels (`pic-text`, `pic-title`) always stay on top. rpic.dev's shapes now
  sit under the basemap's place names.
- Scene: each `text` line carries `row`, its 0-based row in the author's
  label stack (blank rows included), and the item carries `text_rows`, the
  stack's height — so a consumer can tell where the visible lines of
  `box "" "below"` sit (additive; `scene` stays 1; DECISIONS S13).
- Macro calls keep their arguments as written: `MacroCall.raw_args`
  (`Vec<RawArg>`, index-aligned with the normalized `args`) gives each
  argument's source text from its first token to its last — `1e-9` stays
  `1e-9`, spacing, quotes and `\` continuations kept — with its `span`
  (start, `end_col`, the source unit's `file`) and `end_line`, the shape
  of `PassLine`. Sliced from the lexed text, not rebuilt from tokens;
  nested calls carry the outermost call's, like `args`. The scene's block
  `call` gains `raw_args` (additive; `scene` stays 1). `--svg`/`--json`
  unchanged. DECISIONS E1 (#425).
- Labels on flat macro calls: `R1: resistor(A,B)` — a label in front of a
  call whose body cannot take it the classic way (it starts with
  assignments, a label, a control statement, or is empty) names the group
  of objects the call draws, like a block (`R1.n`, `R1.wid`, `R1.start`,
  labels it binds directly as `R1.T`) without moving anything (each group
  counts toward `max_shapes`); `last []`/ordinals skip
  it, `--scene` gets a labelled block with the call's provenance, a call
  that draws nothing is an error. Bodies that start with an object or a
  position keep pic's textual reading, so every source dpic accepts is
  unchanged (`--svg`/`--json`/`--scene` corpus 128/128). The circuit
  library's `switch`, `nmos`, `pmos`, `njfet`, `pjfet` gained `__` geometry
  twins for preludes that shadow them. DECISIONS E3 (#425).

### Changed

- troff passthrough: a `+` in column 1 right after a passthrough line (or
  after another `+` line) continues that request — it joins the entry
  (`text` keeps the newlines, `PassLine.end_line` names the last line, the
  bundle's `passthrough` entry gains `end_line`) instead of being a parse
  error. A `+` line anywhere else is still an error; dpic 2025.08.01 rejects
  both. Single-line entries are byte-identical. DECISIONS E2 (#425).
- **API:** `ir::MacroCall`, `ir::RawArg` and `ir::PassLine` are now
  `#[non_exhaustive]`: they are embedder-facing records that will keep
  growing, so code outside rpic-core can read their fields but no longer
  build them with a struct literal or match them exhaustively. Nothing in
  rpic or rpic-r builds them.
- **API:** `ast::Stmt` gains `LabelledCall` and `token::Token` gains the
  synthetic `CallOpen`/`CallClose` (labelled calls, E3); exhaustive
  matches on either outside rpic-core need an arm.
- geopic-core: `maplibre::Sizing` gained a `text_scale` field and is now
  `#[non_exhaustive]` — read its fields freely, but build it only through
  `sizing()` / `sizing_with()` (a struct literal outside the crate no longer
  compiles).

### Fixed

- geopic: blank rows of a label stack are kept in `geopic:text` (`box ""
  "below"` → `"\nbelow"`), so a map draws "below" one row under the centre
  as the SVG does instead of on it (DECISIONS G10 addendum).

## [0.15.1] — 2026-11-07

### Fixed

- geopic → MapLibre: a box's title was drawn twice when the box straddled a
  vector-tile boundary (MapLibre labels a polygon once per tile it is clipped
  into). Every shape that carries text now also yields a `Point` feature with
  `geopic:kind = "title"` at the label's centre (`geopic:for` = the shape's id,
  `geopic:id` = `<id>:title`), and the `pic-title` layer draws those points,
  styled like `pic-text` (anchor and rotation now honoured). Additive:
  `geopic:schema` stays 1; the polygon keeps its `title`, arrowheads no longer
  carry the label. DECISIONS G10 (#435).

### Added

- Scene: items that carry `text` also carry `text_center`, the centre the
  lines are laid out around (additive; `scene` stays 1; DECISIONS S12). The
  geopic title points are placed there.

## [0.15.0] — 2026-11-07

### Added

- **geopic → MapLibre: the layer spec as data** (#432). `geopic_core::maplibre`
  emits the six MapLibre style-spec layers (`pic-fill`, `pic-stroke`,
  `pic-stroke-dashed`, `pic-dot`, `pic-text`, `pic-title`, source `pic`) that
  draw a collection with every size true to the ground — the styling that
  lived only in the site's `geomap.ts`, ported verbatim (docs/geopic.md § 6,
  DECISIONS G9). Exposed as `rpic --geojson --maplibre` (`{geojson, layers}`;
  plain `--geojson` unchanged), `compileGeoJSON(src, { layers: true })` and
  `maplibreLayers(fc)` in JS, `rpic.geo.maplibre_layers(fc)` and
  `rpic.geo.compile(…, layers=True)` in Python.
- **`@strategicprojects/rpic/maplibre`**: a zero-import subpath module with
  `attach(map, fc, { layers, fit })` (source + layers on any MapLibre map →
  `{ setGeoJSON, fitTo, detach }`), `createGeoMap(maplibregl, container,
  opts)` and a JS port of the layers, pinned to the engine by the tests.
- Site: the Maps page draws through that module (build-time maps take the
  engine's layers from `rpic --geojson --maplibre`, the playground from
  `compileGeoJSON(…, { layers: true })`), checked against snapshots of the
  previous layer specs (`examples/geo/maplibre/`, `npm test` in `site`); new
  section "Use it on your own map".

## [0.14.0] — 2026-11-07

### Added

- **Site: a live GeoPic playground** on the Maps page ("Try it live"): edit
  pic source in the browser and see the SVG and the drawing on an interactive
  MapLibre map, re-georeferenced on every edit (debounced), with the four
  `examples/geo` sources, a `frame` switch, pic/geo errors, warnings and the
  registration summary. Everything runs client-side through the npm
  package's geo wasm build (665 KB), loaded lazily when the playground
  scrolls into view or the editor is focused. The map code moved to a shared
  client module (`site/src/lib/geomap.ts`) that the build-time maps use too;
  ground-true sizes now honour the `mercator` frame (its metres are already
  Web Mercator's, so no `cos φ` factor). The site CI and the Pages deploy
  build the JS binding's wasm before the site.
- **geopic: sizes true to the ground.** Features with text carry
  `geopic:font_size` (points, from the scene's resolved font) and the
  `FeatureCollection` carries a foreign member `geopic: {schema, registration}`
  with the similarity that placed the drawing, so a consumer converts any pic
  length into metres (`pt / 72 × scale_m_per_in`). The site's maps use it:
  strokes, dots and labels scale with the zoom in metres (exponential zoom
  interpolation; text stops computed per map so MapLibre's 16-bit symbol-size
  packing never overflows), keeping the SVG's proportions at any zoom.
- **`geopic-core`** (new workspace crate, publishable): georeferences an rpic
  scene into a GeoJSON `FeatureCollection` (WGS84). Registration by
  `geo_lon`/`geo_lat` + `geo_scale` + `geo_bearing`, by two control points
  (`<Label>_lon`/`<Label>_lat` on labelled places/objects; closed form) or by
  three or more (least-squares Helmert, RMS reported, `registration_residual`
  warning); frames `enu` (local tangent plane, Karney geodesics via
  `geographiclib-rs`) and `mercator` (EPSG:3857 similarity); every geometry
  densified in the plane before de-projection; simplestyle + `geopic:*`
  properties; structured warnings, never silent. Pure Rust, no I/O, builds
  for `wasm32-unknown-unknown`. The engine does not depend on it.
- **`rpic --geojson`**: the CLI front of `geopic-core`, with `--geo-anchor`,
  `--geo-scale`, `--geo-bearing`, `--geo-frame`, `--geo-view-scale` overrides
  and repeatable `--geo-feature NAME=LON,LAT` control points; warnings on
  stderr. `examples/geo/` holds four worked examples with their expected
  `.geojson`, guarded against drift by `crates/cli/tests/geo.rs`.
- **Python `rpic.geo`** (`rpiclang`, now a mixed package wrapping the native
  `rpic._native`): `compile(src, …)` → `{"geojson", "warnings",
  "registration"}`, `to_geojson()` and `to_geodataframe()` (geopandas via the
  `rpiclang[geo]` extra; an `ImportError` names it otherwise). Keyword
  overrides mirror `GeoOptions`; `features` (dict, tuples or a GeoDataFrame)
  are injected as places — or invisible boxes when they have a bbox — before
  compiling, so a source can say `arrow from Recife to Olinda` without
  defining them. Warnings are raised as `GeoWarning` and returned
  structured; `animate` timelines raise an `ignored` warning. Site page
  "Maps (GeoJSON)" with the four examples on **interactive MapLibre maps**
  (georeferenced at build time, styled from their simplestyle + `geopic:*`
  properties; screen-placed boxes as HTML markers); `examples/geo/folium.ipynb`.
- **JS/TS `compileGeoJSON` / `geoToCanvas`** (`@strategicprojects/rpic`):
  geopic in the browser. A third, lazily fetched wasm build
  (`ready(…, { geo: true })` → `pkg/rpic_wasm_geo_bg.wasm`, from the new `geo`
  feature of `crates/wasm`: `compile_geo` / `geo_to_canvas`) keeps the lean
  module unchanged — geopic would have added 187 KB to it (DECISIONS G8).
  `compileGeoJSON(src, { anchor, scale, bearing, frame, features, flatten,
  viewScale, screenClasses, circuits, texlabels })` returns `{ geojson,
  warnings, diagnostics, registration, canvas }` and ports `rpic.geo`'s
  two-pass feature injection (validated, unique names; caller line numbers;
  a position inside the injected lines is tagged `file: "features"`);
  numeric options are validated (NaN/Infinity never fall back silently);
  georeferencing failures throw `GeoError` with `info = { kind, message,
  hint }`. Full TypeScript types (`GeoOptions`, `GeoResult`, `GeoWarning`,
  `GeoRegistration`, `GeoFeatureInput`).
- Geo results (wasm, JS and Python `rpic.geo.compile`) now carry the
  engine's own warnings (`invalid_color`, `ignored_attribute`, …) ahead of
  the scene and geopic ones, and a `diagnostics` list (pic `print` lines,
  engine notices) — both were dropped before. `rpic_core::diagnostic_json`
  is now public so bindings serialize diagnostics with the engine's keys.

- Scene: a top-level `rescale` field — the uniform factor the whole-picture
  rescale (`.PS <width>`, `maxpswid`/`maxpsht`) applied to the geometry, `1`
  when none did. `vars` stay pre-rescale (classic pic), so consumers that relate
  a variable to the geometry (metres per inch, for maps) multiply by it. Scene
  format stays 1 (additive); schema and golden-key test extended.
  `Drawing.rescale` carries it in Rust.
- Scene: shape items carry their source `span` (as block items already did),
  so a consumer can order definitions the way pic does — `geopic-core` uses
  it to let a user's later `A: box` win over a host-prepended `A: (0,0)`.

## [0.13.0] — 2026-11-06

Minor bump: the pic-compat and tooling-data features #414–#417 add public
fields to `Drawing`, `BlockInfo`, `Object` and `Spanned` (additive; code that
builds them by literal needs `..`). SVG output is byte-identical to 0.12.1
over the whole corpus.

### Added

- **troff passthrough** (#414): a line with `.` in column 1 that is not
  `.PS`/`.PE` is a troff request, as in pic and dpic — it is set aside instead
  of failing to parse, and surfaces as `Drawing.passthrough` and, in the
  compile bundle, as a `passthrough` array (`text` + source span) when the
  source has any.
- **Named places** (#415): `Label: position` statements are exported on
  `Drawing.places`, in the scene as a top-level `places` array (scene units,
  block path, span) and in the compile bundle as `places` (SVG user units,
  the `objects` frame; present only when the source defines some).
- **Block provenance** (#416): `BlockInfo.span`/`call` name the outermost
  macro call (or the inline `[`) a block came from; scene block items carry
  them as `span` and `call`. Shapes keep pointing at the `define` body.

### Changed

- Scene format stays **1**: `places` and the block `span`/`call` fields are
  additive (docs/scene.md §7, DECISIONS S11); the JSON Schema and the
  golden-key test were extended accordingly (#417).
- The compile bundle (`--json`, Python `compile`, JS, C) gains `places` and
  `passthrough` only for sources that have them; every other bundle is
  byte-identical to 0.12.1 (#417).

## [0.12.1] — 2026-11-02

### Changed

- **Smaller WASM**: the npm artifacts build with `opt-level = "z"` +
  `wasm-opt -Oz` — the lean module drops to 471 KB, 5.6% below 0.11.3 even
  with the scene emitter aboard (#403/#410).
- **Portable Linux binary**: the release tarball is now a fully static
  `x86_64-unknown-linux-musl` build (the ubuntu26 runner image had left the
  gnu binary needing a newer glibc than most distros ship), and the `.deb`
  is pinned to an ubuntu-24.04 image (#405/#411). The Linux asset name
  changed accordingly.
- **Dependencies**, all proved output-inert over the corpus: ratex 0.1.15
  (SVG byte-identical incl. `-t`), resvg 0.49.0 (PNG byte-identical),
  pyo3 0.30.1.

### Fixed

- **PyPI publishing**: Linux wheels build in the `manylinux_2_28` container
  (the bare ubuntu26 runner produced `linux_x86_64` wheels PyPI rejects) and
  upload failures now fail the workflow instead of hiding behind
  `continue-on-error`; `workflow_dispatch` re-publishing supported
  (#404/#407).

## [0.12.0] — 2026-11-01

### Added

- **Scene output** — the resolved draw list as versioned, device-independent
  JSON (`docs/scene.md`, format 1): rpic's analogue of classic pic emitting
  troff `\D` commands, for building your own backend (GeoJSON/geopic, TikZ,
  Canvas 2D, visual editors) without reimplementing pic. Final geometry in
  inches (y-up), curves as cubic Béziers (`--flatten <tol>` for polylines),
  style as descriptors with colors resolved to hex, text as text with
  baseline anchor points, `vars` carrying every variable's final value, and
  items in draw order with the SVG's stable `s<N>` ids. CLI `rpic --scene`
  and `rpic --json=all`; library `scene_json`/`compile_json_all_with_options`;
  Python `rpic.scene(...)` / `compile(..., scene=True)`; JS
  `compile(src, {scene, flatten})`; C `rpic_scene_json` /
  `rpic_compile_json_all`. Fully opt-in: `rpic file.pic` and `rpic --json`
  stay byte-identical (verified over the 124-file corpus). Anything the SVG
  paints that the scene cannot represent raises a structured
  `scene_unsupported` warning. A ~100-line consumer in
  `examples/scene-consumer/` redraws figures from the scene alone; a corpus
  round-trip test keeps the reference render within 0.5% of pixels of the
  official SVG.

## [0.11.3] — 2026-09-29

### Changed

- **ratex 0.1.13 → 0.1.14** (TeX labels, `-t`). One visible change: a
  `$\dots$` label now gets its TeX box height (0.123 em instead of 0.313 em),
  so the centred dots sit about 1.4 px lower at the default size. Everything
  else in the corpus is byte-identical.
- **resvg 0.47 → 0.48.1** (PNG backend). PNG output is byte-identical over
  the whole corpus. The binary grows by about 1.8 MB, because resvg 0.48
  brings a second font stack next to the one svg2pdf still pins.

## [0.11.2] — 2026-09-24

### Fixed

- **Extension words no longer shadow dpic identifiers.** A generated
  collision corpus (every extension word placed where the dpic grammar admits
  an identifier: variable, macro name, optional attribute operand; 120
  programs, all accepted by dpic 2025.08.01) found 60 collisions. Three
  parser fixes close all but the documented environment variables:
  - `animate` and its clause words (`after`, `delay`, `repeat`, `yoyo`,
    `ease`, `along`, `stagger`, `out`, `scroll`, `into`) were reserved
    keywords; they are now contextual, recognised only inside an `animate`
    statement, so `after = 2` and `define repeat { … }` mean what they mean in
    dpic (#386).
  - `previous` (the pikchr synonym for `last`) was reserved; it is now
    contextual, so `previous = 2` and `box wid previous` keep their dpic
    reading while `previous box`, `2nd previous`, `previous.e` still work
    (#390).
  - After `dashed`, `dotted`, `chop`, `fill` or `shaded`, a name that has been
    assigned earlier keeps dpic's optional-operand reading instead of being
    taken as an rpic attribute (`fit = 0.3; box dashed fit` now dashes by
    `fit`); an unassigned extension word is still the attribute. Deferred
    `if`/`for` bodies and `exec` sources see the evaluator's live variables
    for the same decision (#388).

### Added

- The collision corpus is a CLI integration test (`crates/cli/tests/collide.rs`):
  40 words × 3 positions, rendered against a neutral-name control; the six
  environment variables of the extension surface (canvas margins, `texlabels`)
  are frozen as the documented exceptions (#389).

Every committed corpus render is byte-identical across all four changes.

## [0.11.1] — 2026-07-19

### Fixed

- **Recursive macro expansion no longer takes exponential time.** Macro
  substitution stamped every emitted token with its own deep copy of the
  call's argument token lists; since argument tokens already carry the
  frames of the enclosing call, per-token size grew roughly
  |args|^recursion-depth. `examples/dpic/manual/man36.pic` (a 17-line
  recursive binary tree) compiled in 4.4 s versus dpic's 2.7 ms, and one
  level deeper ran for hours. The argument frame is now built once per
  substitution and shared (`Arc`), bringing man36 to ~6 ms with the full
  124-file corpus byte-identical in `--svg` and `--json`. (#373)

## [0.11.0] — 2026-07-11

### Fixed

- **Corpus: 36 stale committed `.svg` renders regenerated + a CI drift
  guard.** Intentional byte-moving changes (#274 standalone-label ink, #278
  xcolor→hex) had regenerated only part of the corpus; 36 committed renders
  (1 `dpic/doc`, 12 `dpic/manual`, 23 `figuras`) no longer matched what the
  binary emits — mostly labels that were clipped at the canvas edge and now
  show in full. All 36 regenerated (`-c --svg`) and QA'd on old-vs-new
  contact sheets. A new integration test (`crates/cli/tests/corpus.rs`)
  renders every sibling pair on every CI run and fails on drift, so
  committed renders can't rot silently again. (#364)

### Added

- **`rpic --html` — a self-contained animated page from one command.** The
  new CLI mode writes a single HTML file with the SVG, the animation +
  interaction manifests and the GSAP player inlined (the `player.js` module,
  embedded at compile time and guarded byte-identical to the npm copy by a
  test), plus pinned `gsap@3.13.0` CDN `<script>` tags with SRI integrity
  hashes — and **only** the plugins the used effects require (`move` →
  MotionPathPlugin, `morph` → MorphSVG, `scramble` → ScrambleText, `wiggle`
  → CustomEase+CustomWiggle, `draggable` → Draggable (+Inertia), `animate
  scroll` → ScrollTrigger, wired to scrub with scroll room). A drawing with
  no animations becomes a plain static page with no scripts at all. (#357)
- **npm: standalone player module (`@strategicprojects/rpic/player`).** The
  GSAP player (`animate`/`interactive` + helpers) moved to `player.js`, a
  **zero-import** module — it never touches the wasm compiler, so a page with
  a pre-rendered SVG (`rpic --json` on the CLI) can import just the player
  (bundler subpath `…/player`, or `…/player.js` straight from a CDN) and
  fetch nothing wasm-related. `index.js` re-exports it, so the existing
  `import { animate } from '@strategicprojects/rpic'` is unchanged; types
  split likewise (`player.d.ts`, re-exported by `index.d.ts`). A smoke-test
  guard asserts the module stays import-free. (#356)

- **`link "<url>"` — clickable hyperlinks on objects.** A new contextual
  extension with the same two forms as `class`: an inline attribute
  (`box "docs" link "https://rpic.dev"`) and a statement reaching labels,
  ordinals and macro-drawn shapes (`link A "https://…"`). The SVG backend
  wraps the object's `<g id="sN">` group in `<a href="…">` — label included,
  stable ids untouched, so `class`/`animate` compose on the same shape.
  Re-applying replaces (last one wins). URLs are validated (non-empty, no
  whitespace/control characters, no `javascript:`/`vbscript:`/`data:`
  schemes) so hosts rendering untrusted pictures gain no XSS surface. The
  compile bundle's `objects` entries carry a `"link"` key when set. The anchor
  carries `class="rpic-link"`, opting out of `a:not([class])` host prose CSS
  (no underline/colour leaking into labels) and doubling as a styling hook
  (#362). SVG-only:
  PNG/PDF render the identical picture without the link (usvg flattens the
  anchor; verified pixel-identical). Unused, every output is byte-identical —
  corpus 124/124 for both `--svg` and `--json`. (#358)

## [0.10.0] — 2026-07-10

### Added

- **`animate … with "draw" from <p> to <p>` — reveal a stroke sub-segment.** The
  `draw` effect gains two optional clauses that narrow the trace to any window of
  the stroke, given as a fraction (`60%` or the bare `0.6`); absent ends default
  to 0 and 1. Each rides the manifest only when set (`"drawFrom":0.4,
  "drawTo":0.6`, clamped to `[0,1]`), so a plain `draw` stays byte-identical.
  Both players (npm `index.js`, site `AnimatedPic.astro`) realise the window with
  a pure `stroke-dasharray` sweep — no extra GSAP plugin. `out` retracts the
  window. For `draw`, `from`/`to` mean the reveal fraction; on `slide`/`highlight`
  those words keep their direction/colour meaning. Fractions rather than
  DrawSVGPlugin's absolute pixel range (resolution-independent, plugin-free).
  (#343)
- **`draggable <place>` — make objects interactively draggable.** A new
  directive (not an `animate` effect, because dragging is interaction, not a
  play-on-load timeline) that marks an object grabbable in the browser via GSAP
  Draggable — optionally with momentum (`inertia`, InertiaPlugin), constrained
  to another object's box (`bounds <place>`), or axis-locked (`x`/`y`). It rides
  a new top-level `interactions` array in the `--json` bundle
  (`[{id, kind:"drag", inertia?, bounds?, axis?}]`), absent when unused so plain
  bundles are byte-identical. The static SVG/geometry is untouched (dragging is a
  runtime affordance), so PNG/PDF are unaffected. The npm binding gains an
  `interactive(root, interactions, Draggable)` helper; `draggable` is contextual,
  so it stays usable as a variable name. (#331)
- **`animate … with "wiggle"` — attention shake.** A quick oscillating shake
  that returns to rest — the "look here" nudge that draws the eye without moving
  the object. `wiggles <n>` sets the oscillation count (default 6) and rides the
  manifest as `"wiggles":n` only when set; `wiggles` on a non-`wiggle` effect
  warns `wiggles_without_wiggle`. Built on GSAP's CustomWiggle ease (register
  `CustomWiggle`/`CustomEase`). Byte-inert on the SVG side. Any GSAP ease still
  passes straight through `ease "<name>"` (`ease "bounce.out"`,
  `ease "elastic.out"`, or a consumer-registered `CustomBounce` by name). (#330)
- **`animate … with "scramble"` — decode-style text reveal.** A label's glyphs
  cycle through random characters and resolve into the real text. It drives the
  `<text>` element directly through GSAP's `ScrambleTextPlugin` (which, unlike
  SplitText, works on SVG `<text>`), so there is **zero** cost on the SVG side —
  the static render and the whole corpus are byte-for-byte unchanged. A custom
  charset comes from `by "<chars>"` (e.g. `by "01"` for a binary look; default
  `upperCase`) and rides the manifest as `"chars":"…"` only when set; `out`
  scrambles the label away; `by "…"` on a non-`scramble` effect warns
  `by_without_scramble`. Needs `ScrambleTextPlugin` registered (free since
  GSAP 3.13). (#329)
- **`animate … with "type"` — typewriter text reveal.** A label appears one
  character at a time (or `by word`), staggered over the effect's duration — the
  way a caption "speaks" a step. The split is native: the SVG backend wraps each
  unit of a `type` target in a `<tspan class="rpic-ch">` (GSAP's SplitText
  doesn't support SVG `<text>`), and the browser player staggers their opacity —
  no plugin to install. The tspans carry no positioning, so the static render is
  identical and any drawing without a `type` animation is byte-for-byte
  unchanged. `out` reverses it into a staggered erase; `by` on a non-`type`
  effect warns `by_without_type`. (#328)

### Fixed

- **The `invalid_color` warning now carries a source span.** A colour typo
  (`box outlined "crimsom"`) still warns with a "did you mean" hint, but the
  warning now reports the offending token's `line`/`col`/`end_col` — so an editor
  can jump to and underline it, like every other diagnostic. The colour
  attributes (`outlined`/`shaded`/`color`, `hatchcolor`, `gradient`) thread the
  token span through to the check. (#333)

## [0.9.0] — 2026-07-09

### Added

- **C ABI: full compile options via `*_ex` entry points.** A new `RpicOptions`
  struct and `rpic_render_svg_ex` / `rpic_compile_json_ex` / `rpic_render_png_ex`
  / `rpic_render_pdf_ex` let C/R embedders enable `texlabels`, sandbox
  `copy "file"` includes (`include_policy` 0/1/2), and set the include `base` —
  the circuits-only functions (unchanged ABI) previously exposed none of these,
  so untrusted-input embedders had no way to restrict filesystem access.
- **`thin` line-thickness keyword.** A pikchr-flavoured convenience for a
  lighter stroke — `line thin` / `box thin`, no value — set to two-thirds of the
  current `linethick`, so it tracks your global line width. Complements the
  existing valued `thick <n>`.
- **dvips/xcolor colour names resolve to their RGB.** The 30 dvips names no
  browser understands (`Dandelion`, `BurntOrange`, `Periwinkle`, …) now emit
  their `dvipsnam.def` RGB (`Dandelion` → `#ffb529`) instead of passing through
  as an SVG-invalid keyword that rendered as no paint. dvips names that are
  *also* CSS keywords (`Goldenrod`, `Plum`, …) stay untouched — browsers already
  render those, and the dvips values differ.
- **Unknown colour names are flagged.** A colour string that isn't a CSS named
  colour, a `#hex` / `rgb()` / `hsl()` value, or a dvips/xcolor name (the ones
  the dpic corpus uses, like `Dandelion`) now raises an `invalid_color` warning
  in the `--json` bundle — with a "did you mean" suggestion for near-miss typos
  (`"crimsom"` → `crimson`) — instead of silently rendering blank. The colour is
  still passed through unchanged, so the warning is advisory and never blocks or
  alters rendering (SVG output is byte-for-byte unchanged).
- **Colours can be held in a variable or computed.** In colour position
  (`outlined`, `shaded`, `color`, `hatchcolor`, `gradient`, `animate … to`), a
  bareword that names a variable now resolves to its value as a `0xRRGGBB`
  colour (`accent = 0x2f855a; … outlined accent`), and a parenthesised
  expression is evaluated (`shaded (accent + 0x60)`) — so a palette defined once
  can drive a whole figure. A bareword that is *not* a variable still stays a
  literal colour name, so existing sources are byte-for-byte unaffected.

### Security

- **`font "…"` family names are now XML-attribute-escaped.** A double quote in a
  per-string font family was emitted into the `font-family="…"` attribute
  unescaped (it used the text-content escaper, not the attribute one), which
  closed the attribute early — producing malformed SVG and letting crafted input
  inject stray attributes (e.g. an event handler) onto the `<text>` element, an
  XSS vector when the SVG is embedded inline in HTML. The value now escapes `"`
  to `&quot;` like every other attribute. Byte-for-byte unchanged for any figure
  without a quote in a font name (the whole corpus). (#317)

### Changed

- **SVG escapers are named by context, closing the #317 class of bug.** The two
  near-identical helpers are now `escape_text` (element content) and
  `escape_attr` (attribute values, escaping `"`), so a text escaper used for an
  attribute reads wrong at the call site instead of silently emitting malformed
  markup. Every user-controlled attribute (colour, class, gradient stop, hatch
  colour, font family) routes through `escape_attr`, guarded by a test that
  feeds hostile text through each and asserts the quote can't break out. Also
  drops three needless `stroke` clones (`as_deref().unwrap_or("black")`).
  Output is byte-for-byte unchanged. (#324)
- **The evaluator is split into an `eval/` module.** `eval.rs` had grown to
  7.6k lines — by far the largest file in the crate. It is now `eval.rs` (the
  `State` type, entry points, statement/animation dispatch) plus `eval/build.rs`
  (primitive construction), `eval/resolve.rs` (position/place resolution and
  expression evaluation), `eval/helpers.rs` (pure free functions) and
  `eval/tests.rs`, each comfortably under 2k lines. Pure reorganization: no
  public-API change and SVG output is byte-for-byte identical (corpus 124/124).
  (#323)

### Fixed

- **A rotated, justified label no longer vanishes.** A `"…" ljust rotated 90`
  (or `rjust`) label is rotated about its text anchor, but its bounds were
  computed about the rect centre — offset by half the width — so a long one
  landed entirely outside the viewBox and rendered blank. The bounds now rotate
  about the same anchor the renderer uses.
- **`after <block>` waits for a whole staggered block** even when the block
  leads with an invisible spine (a `move`): the stagger's end time was recorded
  on the first visible child, not the block, so `after` a stagger could start
  mid-way through it.
- **Macro-argument splices space by source position, not rendered length**, so
  a normalized float or escaped string in a spliced arg (`lbl((1.50,2.50))`) no
  longer gains stray spaces (`(1.5 ,2.5 )`).
- **Hostile input can no longer crash the process.** Several unbounded paths
  that aborted (uncatchable, unlike every other error) on adversarial `.pic`
  source — reachable from the CLI and the wasm binding — now fail cleanly:
  deeply nested parentheses/blocks (`((((…))))`) hit a recursive-descent depth
  limit and return a "nested too deeply" error instead of overflowing the stack;
  flat operator chains (`1+1+…`) are capped; a `sprintf` precision like
  `"%.999999999f"` is clamped (to 512 digits) instead of allocating gigabytes;
  and four constructs the earlier passes missed — brace-group nesting
  (`{{{…}}}`), a leading corner chain (`.n.n…`), string concatenation
  (`"a"+"a"+…`), and a trailing member/corner chain (`A.B.B…`) — now hit the
  same depth/chain-length limits rather than overflowing on parser recursion or
  on the drop/evaluation of an unbounded left-deep AST. (#318)
- **The C ABI no longer aborts the host on a Rust panic.** Every `extern "C"`
  entry point now runs its body inside `catch_unwind`, returning the crate's
  null-pointer failure convention instead of letting a panic unwind across the
  FFI boundary and abort the C/C++/R host process. (A stack overflow is a hard
  abort `catch_unwind` cannot intercept — that is guarded separately by bounding
  recursion in the core; this covers ordinary panics.) The Python binding was
  already panic-safe via PyO3; wasm traps cleanly. (#319)
- **wasm `compile()` denies filesystem includes like its siblings.** It
  delegated to the Unrestricted-default core entry, so a `copy "file"` gave an
  opaque io error instead of the clean policy error `compile_circuits`/
  `compile_with` return; it now forces `Deny` (wasm has no filesystem;
  `copy "circuits"` still works).
- **JS: complete animation TypeScript types and docs-player parity.** The
  `Anim` type now declares the optional manifest keys the player actually reads
  (`repeat`/`yoyo`/`ease`/`path`/`color`/`out`/`from`/`morph`) and `Bundle`
  gains `scroll?`; the docs-site GSAP player (`AnimatedPic.astro`) gained the
  unknown-effect fade fallback and the shared `draw`-label timing so it matches
  the shipped npm player exactly. The npm runtime is unchanged.
- **A gradient-only fill now honours `opacity`.** `box gradient … opacity 0.3`
  rendered fully opaque, while solid `fill`/`shaded` and `hatch` fills (and
  gradient+hatch) honoured it — the opacity predicate didn't count a gradient as
  a fill.
- **Macro-argument splices (`"$n"` inside a string) reproduce the argument's
  source text.** A multi-token argument used to be glued without separators,
  with keywords silently dropped and string quotes stripped — a statically
  exec'd `"$2"` holding `box shaded "#00ff00"` re-lexed as `boxshaded` and the
  bare `#…` started a comment. Spacing now comes from the tokens' source spans
  (adjacent tokens like `2L` stay glued; separated words like `$\beta V$` keep
  their gap — that TeX label used to collapse into the undefined `$\betaV$`),
  keywords render, and inner strings keep their quotes. A lone quoted argument
  still splices as bare content, so the classic `box "$1"` label idiom is
  untouched.
- **`gradient` + `hatch` now paints one gradient across the whole object.** An
  SVG pattern tile is stamped once and replicated, so the gradient embedded as
  the tile background restarted in every hatch cell (a "quilted" look) instead
  of spanning the object. The gradient is now painted by a separate underlay
  element with the hatch pattern (transparent tile) on top — in every shape
  kind, and identically in browsers and the PNG/PDF rasterizer. A solid
  `fill`/`shaded` background is uniform, so it stays in the tile as before.
- **`define`s inside `exec` now persist, and the `dpicopt`/`opt*` constants
  match dpic's.** A macro defined by exec'd text landed in a discarded clone of
  the macro table, and the backend-option constants were zero-based where dpic's
  are one-based (`dpic -v` prints `dpicopt=9`, `optMFpic=1` … `optxfig=12`).
  Together these broke the dpic test suite's `DefineRGBColor(name, r, g, b)`
  machinery — its `case(dpicopt, …)` exec-dispatch picked the PSTricks branch
  and the colour macro never registered, so `shaded Custom` emitted a broken
  `fill="Custom"`. User-defined colours now resolve to the same `rgb(…)` string
  dpic emits.
- **Standalone labels (`"text" at P`) no longer clip at the page edge.** Their
  glyph ink is now included in the drawing bounds — matching what attached
  labels already did — so a wide or edge-anchored label is fully visible without
  a hand-tuned `margin`. This intentionally diverges from dpic, which bounds
  attached-label ink but leaves standalone labels zero-width (and so clips them).
- **A quoted `"0xRRGGBB"` colour string no longer slips through to invalid
  SVG.** It is normalised to `#rrggbb` (the bare `0xRRGGBB` literal already
  worked); previously the string form was emitted verbatim as `stroke="0x…"`,
  which no renderer understands.

## [0.8.1] — 2026-07-06

### Fixed

- **JS player crashed on the `move` and `morph` effects.** GSAP's MotionPath and
  MorphSVG plugins need a real `<path>`, but rpic emits primitives
  (`<line>`/`<rect>`/`<circle>`/`<polygon>`), so `@strategicprojects/rpic`'s
  `animate()` threw "Expecting a `<path>` element or an SVG path data string" and
  looped under `repeat`. The player now converts the referenced shapes to
  `<path>` up front (pure DOM, no plugin dependency). The engine, manifest and
  the other bindings were unaffected — this is a browser-player fix, so it ships
  only in the npm package; crates.io / PyPI 0.8.0 were already correct.

### Changed

- The `highlight` effect now also thickens the outline and adds a small scale
  pulse alongside the colour tween, so the emphasis reads at a glance.

## [0.8.0] — 2026-07-06

### Highlights — a complete animation subsystem

`animate` grew from three enter effects into a full, declarative storytelling
layer. Everything stays opt-in and **byte-for-byte inert when unused** (the
123-example corpus renders identically), and every timeline is emitted as a flat
JSON manifest — readable without a player, and driven in the browser by GSAP.

**Effects**

- **`slide` from a compass direction** — enter by translating in from
  `up`/`down`/`left`/`right`, offset by the object's own extent.
- **`move` along a path** — a token/dot travels along another object's geometry
  (a wire, an arrow) via GSAP MotionPathPlugin. `along <path>`.
- **`highlight`** — emphasise an object by tweening its outline to a colour
  (any rpic colour form), or a colour-free scale pulse. `to <colour>`.
- **`morph` into another shape** — tween one object's outline into another's
  geometry (box → circle, symbol → symbol) via GSAP MorphSVGPlugin.
  `into <shape>`.

**Modifiers & structure**

- **`out`** — play *any* effect as an exit instead of an entrance (fade away,
  pop out, retract a `draw`, slide off), for two-beat build-up / tear-down.
- **`repeat` / `yoyo` / `ease`** — GSAP passthroughs: loop count (`-1` = forever
  without stalling the sequence), ping-pong, and a custom easing name.
- **`stagger`** — point `animate` at a `[ … ]` block to fan the effect across
  its visible children, each offset by a fixed delay.
- **`animate scroll`** — a timeline-level hint (surfaced as top-level
  `scroll: true`) that the host should scrub the timeline on scroll; the
  consumer wires GSAP ScrollTrigger.

**Timing** stays sequential by default, or absolute (`at`), or relative
(`after <object>`), each with an optional `delay`.

Every clause that an effect requires (`along`, `into`, `from`, `to`) is a
compile error when missing and a warning (never fatal) when used on the wrong
effect; optional manifest keys are emitted only when set. The reference page and
live examples are at
[rpic.dev/docs/extensions/animate](https://rpic.dev/docs/extensions/animate).

### Added

- Animation effects `slide`, `move`, `highlight`, `morph`; the `out` exit
  modifier; `repeat`/`yoyo`/`ease` passthroughs; block `stagger`; and the
  `animate scroll` directive (see Highlights).
- New warnings: `yoyo_without_repeat`, `along_without_move`,
  `to_without_highlight`, `from_without_slide`, `into_without_morph`,
  `stagger_without_block`, plus the extended `unknown_animation_effect` set.
- `Shape::is_visible()` helper (skips invisible `move`/`invis` spines when
  staggering a block).
- README and homepage refreshed with the full animation palette and a live
  "Watch it build" showcase.

### Changed

- The JSON animation manifest gained optional `repeat` / `yoyo` / `ease` /
  `path` / `color` / `out` / `from` / `morph` per-entry keys and a top-level
  `scroll` flag — all present only when the source uses them, so existing
  manifests are unchanged.
- The JS and Astro players register GSAP's MotionPathPlugin and MorphSVGPlugin
  for the `move` and `morph` effects.

## [0.7.1] — 2026-07-06

- **#240 pikchr-compat** (opt-in, corpus 121/121): `.start`/`.end` as `with`
  anchors on closed objects, `previous` as a synonym for `last`, `aligned` text
  (rotate a label to its segment's angle), and `big`/`small` size words.

## [0.7.0] — 2026-07-05

- Per-object geometry export (`objects` in the compile bundle) for visual
  editors (#227).
- `canvas from <pos> to <pos>` — a fixed page / stable viewBox (#226).
- Per-string font attributes: `bold`/`italic`/`mono`/`font "…"`/`fontsize n`
  (#225).
- `rotated <deg>` labels and native colour literals `rgb(r,g,b)` / `0xRRGGBB`
  (#228).
- First minted DOI via the GitHub ↔ Zenodo webhook.

[Unreleased]: https://github.com/milkway/rpic-lang/compare/v0.18.0...HEAD
[0.18.0]: https://github.com/milkway/rpic-lang/compare/v0.17.1...v0.18.0
[0.17.1]: https://github.com/milkway/rpic-lang/compare/v0.17.0...v0.17.1
[0.17.0]: https://github.com/milkway/rpic-lang/compare/v0.16.1...v0.17.0
[0.16.1]: https://github.com/milkway/rpic-lang/compare/v0.16.0...v0.16.1
[0.16.0]: https://github.com/milkway/rpic-lang/compare/v0.15.1...v0.16.0
[0.15.1]: https://github.com/milkway/rpic-lang/compare/v0.15.0...v0.15.1
[0.15.0]: https://github.com/milkway/rpic-lang/compare/v0.14.0...v0.15.0
[0.14.0]: https://github.com/milkway/rpic-lang/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/milkway/rpic-lang/compare/v0.12.1...v0.13.0
[0.12.1]: https://github.com/milkway/rpic-lang/compare/v0.12.0...v0.12.1
[0.12.0]: https://github.com/milkway/rpic-lang/compare/v0.11.3...v0.12.0
[0.11.3]: https://github.com/milkway/rpic-lang/compare/v0.11.2...v0.11.3
[0.11.2]: https://github.com/milkway/rpic-lang/compare/v0.11.1...v0.11.2
[0.11.1]: https://github.com/milkway/rpic-lang/compare/v0.11.0...v0.11.1
[0.11.0]: https://github.com/milkway/rpic-lang/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/milkway/rpic-lang/releases/tag/v0.10.0
[0.9.0]: https://github.com/milkway/rpic-lang/releases/tag/v0.9.0
[0.8.1]: https://github.com/milkway/rpic-lang/releases/tag/v0.8.1
[0.8.0]: https://github.com/milkway/rpic-lang/releases/tag/v0.8.0
[0.7.1]: https://github.com/milkway/rpic-lang/releases/tag/v0.7.1
[0.7.0]: https://github.com/milkway/rpic-lang/releases/tag/v0.7.0
