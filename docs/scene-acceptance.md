# Scene output — acceptance checklist results

Run on 2026-11-01 (macOS arm64, branch `scene-output` vs `main` @ 0fc25d1).
Every claim is backed by a committed test or a command reproduced below.

## A. Fidelity

- [x] **A1 — Corpus coverage.** `crates/cli/tests/scene.rs::corpus_scene_coverage_ids_order_and_schema`:
  all 124 corpus files (`examples/` + `assets/`) compile under `--scene`;
  scene shape-items map 1:1 onto the SVG's `<g id="sN">` groups, and
  non-`hidden` items correspond exactly to groups that paint ink (empty
  `<text>` elements from blank label lines count as no ink). No silent skips —
  any failure names the figure.
- [x] **A2 — Geometric round-trip.**
  `crates/render/tests/scene_roundtrip.rs`: a naive reference renderer
  (documented scene fields only) redraws each figure from its scene; official
  and reference SVGs rasterized with the same resvg + bundled-Go-font pipeline
  at ~400 px differ by at most **0.078 % of pixels** (fig41; next: rlc 0.041 %,
  fig42 0.015 %) against the 0.5 % budget. No figure needed a waiver.
- [x] **A3 — Ids and order.** Same corpus test: scene array order equals SVG
  document order (the stable `(layer, index)` sort), ids match pairwise, `z`
  is dense; unit test pins that `behind` reorders `z` but never renumbers ids.
- [x] **A4 — Resolved style.** Corpus-wide: every scene color is `#rrggbb`
  unless the figure raised `scene_unsupported` (none in the corpus does);
  unit tests pin gray→hex, `rgb()`→hex, CSS-name→hex, dash `{kind,len/gap}`,
  width in points, and `opacity` against the SVG attributes.
- [x] **A5 — Curves.** `scene::tests::spline_cubics_match_the_svg_path_d`:
  scene cubics mapped through the documented transform reproduce the SVG `d`
  numbers (< 2e-4 px, i.e. serializer rounding). `flatten_polylines_stay_within_tolerance_of_the_cubics`:
  dense samples of the exact cubics stay ≤ 1e-3 in (the requested tolerance)
  from the `--flatten 1e-3` polyline, splines and arcs.

## B. Contract and stability

- [x] **B1 — Schema.** `docs/scene.schema.json` (JSON Schema 2020-12);
  every corpus scene validates in the same corpus test (jsonschema, dev-dep).
- [x] **B2 — Versioning.** `scene: 1`; policy in docs/scene.md §7;
  `scene::tests::scene_version_bump_guard` pins the emitted key vocabulary —
  a removed/renamed key fails until the golden list and `SCENE_VERSION` move
  together.
- [x] **B3 — Opt-in and inertness.** Dual-binary proof: main vs branch over
  all **124** corpus files, `-c -t --svg` **and** `-c -t --json` — **0 diffs**
  (`checked 124 files, 0 diffs`). The corpus drift guard and
  `json_all_bundle_merges_scene_warnings_and_plain_json_is_unchanged` keep it
  that way in CI.
- [x] **B4 — `vars`.** `scene::tests::vars_carry_user_values_at_f64_precision_and_builtins`:
  `geo_lon = -34.8811` round-trips exactly (f64, shortest round-trip
  serialization); built-ins appear under their pic names with effective values
  (`scale = 250` → `boxwid: 187.5`).
- [x] **B5 — Never silent.** The two scene gaps (docs/scene.md §9) are
  tested: irreducible colors (`currentColor`) pass through verbatim with a
  spanned `scene_unsupported` warning; texlabels math without `--outline`
  warns and carries metrics only, `--outline` embeds the fragment and clears
  the warning (CLI test over `--json=all`).

## C. Robustness and cost

- [x] **C1 — Fuzz.** The repo had **no pre-existing fuzz harness** (the
  criterion presumed one); `crates/core/tests/scene_robustness.rs` adds a
  seeded mutational fuzzer (token splice/truncate/duplicate/flip over
  pic-shaped seeds; random flatten/outline; includes denied). Run
  **640 s release** without a panic, hang or malformed JSON
  (`RPIC_FUZZ_SECS=640 … -- --ignored fuzz`).
- [x] **C2 — Hostile input.** Same file: denied `copy` → structured
  `include_denied`; 1 MB labels → valid scene; 10⁵ objects → valid scene
  (or structured budget error under a lower host cap); runaway macro
  recursion → structured error; hostile flatten tolerances bounded
  (depth cap 12, non-finite = flat).
- [x] **C3a — Compile cost.** 3 passes × 124 files × (`-c -t`): `--json`
  8.17 s vs `--json=all` 8.25 s = **+0.98 %** (budget 10 %); the default
  compile pays nothing (scene built only on demand).
- [ ] **C3b — WASM size: EXCEEDED, listed as a pendência.** Lean build
  (wasm-pack, `opt-level = "s"`, no wasm-opt — none installed locally or in
  `npm.yml`): 499 605 → 557 248 B raw (**+11.5 %**), 194 829 → 216 156 B
  gzipped (**+10.9 %**) vs the ≤ 5 % budget. The delta is the scene emitter
  itself (an experiment pinning float formatting changed nothing). Mitigation
  candidates, next PR: (1) add `wasm-opt -Oz` to the npm pipeline (shrinks
  both sides; never enabled for this package), (2) slim the emitter's
  `format!` surface, (3) a `scene` cargo feature on `rpic-wasm` so lean
  embedders can opt out. **Responsible:** next release owner; **reason:**
  C3 is not on the merge cut-line and the fix options change packaging, not
  the format.

## D. Bindings and documentation

- [x] **D1 — Parity.** All three surfaces serialize through the single core
  emitter (parity by construction); evidence: the C test pins byte-equality
  of `rpic_scene_json` against `compile_scene_with_options`, and the Python
  (pytest 21/21 locally) and JS (`npm test` green, lean + math wasm rebuilt)
  suites spot-check the same snippets — header, ids/kinds, blocks, cubics,
  flatten, gray/`rgb()` hex, dash, opacity, warning merge. **R:** gap
  documented (docs/scene.md §11) — rpic-r pins the published crates.io core
  and picks `rpic_scene()` up at its next engine bump.
- [x] **D2 — Docs.** `docs/scene.md` (spec + schema + policy),
  `docs/scene-internals.md` (pre-implementation mapping), site page
  `site/src/pages/docs/scene.mdx` (Reference section) + pointer in
  `/docs/bindings`, CHANGELOG entry, `DECISIONS.md` (S1–S10, including the
  user-approved pauses and the S7 amendment).
- [x] **D3 — Consumer example.** `examples/scene-consumer/scene2svg.py` —
  99 lines, stdlib only, scene → plain SVG; verified against simple figures
  and the rlc circuit (valid XML; same renderer family as the A2 test, which
  quantifies fidelity).

## E. Process

- [x] **E1 — Pauses.** Names, item schema (curves/kinds), the only
  backend-adjacent refactor (spline string builders → cubic lists, proved
  byte-inert), and the no-new-runtime-deps choice were presented as options
  and approved before implementation (DECISIONS.md S1–S4; S5–S10 under the
  granted implementation authority, S7 amended with rationale).
- [x] **E2 — History.** Small commits, each building with the full test
  suite green; no WIP commits.

**Cut-line:** A1–A3, B1–B3, C1, D2 all pass. The single open item is C3b
(wasm size), listed above with owner and reason.
