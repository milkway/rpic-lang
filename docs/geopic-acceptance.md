# geopic — acceptance record (Prompt A)

Criteria from the geo prompt, with the evidence per PR. Checked items are
enforced by tests that run in CI (`cargo test -p geopic-core`).

## PR 1 — crate, registration, geodesy (2026-11-06)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| A1 | Round trip canvas → lon/lat → canvas < 1 mm, 6 latitudes × 4 distances × 8 azimuths, `enu` and `mercator` | ✅ | `a1_round_trip_enu_and_mercator_under_1mm` — worst error well under 1 mm (f64 round-off; the inverse geodesic is exact) |
| A2 | `enu` reproduces Karney's direct problem < 1 mm on 20 fixed cases | ✅ | `a2_enu_matches_karney_direct_problem_under_1mm`; fixture `geodtest_cases.inc` = every 500th line of Karney's `GeodTest-short.dat` (includes near-antipodal and near-polar cases) |
| A3 | 2 points recover an exact synthetic similarity; N points with 2 m Gaussian noise report RMS within ±20 %; warning above threshold | ✅ | `a3_two_control_points_recover_the_exact_similarity` (scale to 1e-6; bearing to 1e-6 in Mercator, 1e-2° in ENU — meridian convergence between the truth's anchor and the fitted one, see DECISIONS G5; the *map* agrees to < 1 mm), `a3_helmert_reports_the_residual_and_warns_above_threshold` (20 points, deterministic LCG noise) |
| A4 | 20 km `line right 80` (`scale = 250`) within 1 m (Hausdorff) of the geodesic; un-densified control > 1 m | ✅ | `a4_densified_line_follows_the_geodesic_and_the_control_does_not` — densified ≤ 1 m; the lon/lat chord deviates > 1 m (anchor Porto Alegre, bearing −45°) |
| A5 | Every scene kind has a feature or a warning; new kinds fail the test | ✅ | `every_scene_kind_is_handled_or_warned` checks the format's 11 kinds against `KINDS` and that a mixed drawing yields every mappable kind with no `unsupported_kind` |
| A6 | JSON Schema of the properties; example `.geojson` validate; `geopic:schema = 1` everywhere | ◐ | `docs/geopic.schema.json` + `a6_feature_properties_validate_against_the_schema` (13+ features of every kind); `examples/geo/*.geojson` come with PR 3 |
| A7 | Never silent: `gradient`, `animate`, `texlabels` produce warnings with spans | ◐ | gradient → `unsupported_style` with the item's id/span; math lines → `ignored`. `animate` is not in the scene (no visual content) — the Python layer (PR 3) warns from the bundle's `animations` |
| A8 | `rpic`, `--json`, `--scene`, `compile()` byte-identical over the corpus; core gains no dependency on geopic-core | ✅ | the engine is untouched in this PR (the `rescale` field landed separately in #420 with its own dual-binary proof: `--svg`/`--json` 124/124 identical). `rpic-core` has no `geopic-core` dependency; the reverse is dev-only |
| A9 | `cargo build -p geopic-core --target wasm32-unknown-unknown` passes; size recorded | ✅ | passes locally and in CI (`bindings` job). `libgeopic_core.rlib` (release, wasm32) = 1.9 MB; the final `.wasm` size is a property of the cdylib that will wrap it (PR 3 / JS binding) |
| A10 | Python `rpic.geo` with and without geopandas | ⏳ | PR 3 |
| A11 | Fuzz ≥ 10 min without panic; poles, antimeridian, `geo_scale ≤ 0` are structured errors | ◐ | `scene_parse_errors_and_invalid_anchors_are_structured` covers `|lat| ≥ 90`, `|lon| > 180`, `geo_scale = 0`/negative, `geo_px` without `view_scale`, antimeridian normalization. No fuzz harness exists in the repository yet; `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` in library code |
| A12 | Pauses recorded; no WIP commits; CI green | ✅ | DECISIONS G1–G6; decisions (b)/(rescale) asked and answered before coding, (c)/(d) in the plan the user approved |
