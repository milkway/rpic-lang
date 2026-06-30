# AGENTS.md

Orientation for working on **rpic** — a Rust reimplementation of the *pic*
picture-drawing language with SVG / PNG / PDF output. See `README.md` and
`DESIGN.md` for the full picture; this file is the quick map.

## Build & run

```sh
cargo build --release -p rpic-cli      # binary at target/release/rpic
./target/release/rpic --svg file.pic -o out.svg
./target/release/rpic --png --scale 2 -o out.png file.pic
./target/release/rpic --pdf -o out.pdf file.pic
./target/release/rpic -c --svg file.pic        # -c loads the circuit library
./target/release/rpic --tokens file.pic        # dump the token stream (pre-expansion)
```

## Test & lint (must pass before a PR)

```sh
cargo test                                  # most tests live in rpic-core
cargo clippy --all-targets                  # CI runs with -D warnings
cargo fmt --all --check                     # CI lint also checks formatting
```

## Layout

| Path | What |
|------|------|
| `crates/core/src/lexer.rs` | tokeniser |
| `crates/core/src/parser.rs` | macro preprocessor (`expand`) + parser; `parse_body_tokens`, `substitute`, `tokens_to_text` |
| `crates/core/src/eval.rs` | evaluator (`State`, `nth_index`, blocks, bbox); most unit tests |
| `crates/core/src/ir.rs`, `svg.rs` | IR + SVG backend |
| `crates/core/src/std/circuits.pic` | native circuit-element library (loaded by `-c`) |
| `crates/render` | PNG/PDF (resvg/svg2pdf); embeds the Go font (`fonts/`) |
| `crates/cli`, `crates/capi`, `crates/wasm` | binary, C ABI, WASM |
| `bindings/{python,js}` | Python & JS/TS (R lives at milkway/rpic-r) |
| `examples/dpic` | dpic corpus (curated, with STATUS.md parity matrix) |
| `examples/figuras` | André Leite's circuit_macros figures + the compat shim |
| `examples/lib3d` | 3D-projection demos + the lib3D shim |

## circuit_macros compatibility (the big recurring theme)

circuit_macros is `m4`-based and **cannot** be run by rpic directly. Instead,
rpic ships native geometry and thin **shims** that adapt the circuit_macros API
to it — *reuse the native geometry, don't reimplement it*:

- **libcct** (elements): `examples/figuras/circuit_macros.pic`. Linear elements
  draw an invisible spine and delegate to the native two-point form
  (`__resistor`, …); `bi_tr`/`opamp` are blocks exposing terminals, built on
  sign-parameterised `__bjt`/`__opamp`; plus `ebox`/`source`/labels/`ground`.
- **libgen**: the `dimension_` annotation macro lives in the same shim.
- **lib3D**: `examples/lib3d/lib3d.pic` — `setview`/`Project` axonometric
  projection (3D → 2D), rendered as flat SVG.

Element figures `copy` the shim and render with `-c`. As of this writing the
collection is complete: **dpic corpus 72/72, figuras 48/48**.

### Useful lexer/macro facts (added during parity work)
- Variables are **global** in pic (block assignments propagate out).
- `last` may be untyped (`last.c` = most recent object of any kind); `last box`
  still filters by kind.
- A path continues across a newline adjacent to `then` (`… then ⏎ …` or
  `… ⏎ then …`). Line continuation also via trailing `\`.
- A `$` not before a digit/`+`, and a non-continuation `\`, lex as **literal
  text** (`Token::Dollar`/`Token::Backslash`) — for LaTeX passed unquoted as a
  macro arg; they round-trip via `tokens_to_text`.
- Macro bodies have their edge newlines trimmed (so a labelled call to a
  multi-line-body macro parses). Multi-line `[ … ]` blocks inside a `define`
  must currently stay reasonable; assignments + dispatch go *inside* the block.

## Working conventions

- **Branches & PRs**: direct pushes to `main` are blocked — always branch, open a
  PR, let CI go green, then `gh pr merge --squash --delete-branch`.
- **After every merge**: sync `main` and run `cargo clean` (the `target/` cache
  grows multi-GB across rebuilds).
- **Dependencies**: always newest stable versions (verify online).
- **Docs must credit** Brian W. Kernighan (pic), Dwight Aplevich (dpic /
  circuit_macros), D. Richard Hipp (pikchr) — see `ACKNOWLEDGMENTS.md`.
- **Parallel Codex work**: run it in a **separate git worktree** (e.g.
  `git worktree add ../rpic-codex-NN <branch>`), never the main checkout — two
  agents in one working tree clobber each other's uncommitted changes.
- A leaked crates.io token from earlier is **compromised** — never use it.

## GitHub remote

Repo is `milkway/rpic-lang`; the local checkout is `/Users/leite/Github/rpic`
(renamed from `dpic` to stop it being confused with Aplevich's `dpic` tool).
Related repos: `milkway/rpic-r`, `milkway/homebrew-rpic`.

## Release & distribution

Cutting a release = bump the version everywhere, merge, then push a `v*` tag.
Version lives in `Cargo.toml` (`[workspace.package] version`) **plus** the
explicit inter-crate dep refs (`crates/{cli,capi}/Cargo.toml`), `crates/wasm`,
`bindings/python/{Cargo.toml,pyproject.toml}`, `bindings/js/package.json`. Leave
the Scoop manifest alone (it `autoupdate`s from the GitHub release).

A `v*` tag triggers, automatically:
- **GitHub release** (binaries / `.deb` / MSI) — `.github/workflows/release.yml`
- **PyPI** (`rpiclang`) — `python.yml` (Trusted Publishing / OIDC, no token)
- **npm** (`@strategicprojects/rpic`, the JS binding) — `npm.yml` (Trusted
  Publishing / OIDC + provenance, no token). The workflow installs the wasm
  toolchain so `prepack` builds the `.wasm` into `bindings/js/pkg`, then
  `npm publish --provenance`. Needs an npm **Trusted Publisher** configured on
  the package (Publisher: GitHub Actions, org `milkway`, repo `rpic-lang`,
  workflow `npm.yml`, no environment, action: Publish).
- **crates.io** — `crates-io.yml` publishes `rpic-core` → `rpic-render` →
  `rpic-capi` → `rpic-cli` (dep order) with `secrets.CARGO_REGISTRY_TOKEN`
  (`rpic-wasm` excluded — npm-only, path-only dep). Idempotent: it skips a crate
  whose version is already on crates.io, so a re-run after a partial failure is
  safe.
- **winget** — `winget.yml` on *release published*
- **Scoop** — `checkver` autoupdate

NOT automated (so they lag unless done by hand):
- **R** (`milkway/rpic-r`) — separate repo; bump its `DESCRIPTION` Version +
  `src/rust/Cargo.toml` rpic-core/render deps + refresh `src/rust/Cargo.lock`,
  then tag/release there. It pulls rpic-core/render **from crates.io**, so those
  must be published *first*.

### npm gotchas (learned publishing 0.1.0)
- Package is **`@strategicprojects/rpic`** (scoped). The unscoped `rpic` is
  rejected by npm ("too similar to rc/rfdc/grpc"). Install: `npm i
  @strategicprojects/rpic`.
- `prepack`'s `wasm-pack --out-dir` is **relative to the crate dir**
  (`../../crates/wasm`), so it must be `--out-dir ../../bindings/js/pkg` to land
  in the JS package (which is git-ignored / generated). `bindings/js/pkg` is
  not committed — always build before packing.
- Local manual publish: the Homebrew `rustc` (no `wasm32` target) shadows
  rustup on PATH — prefix with `PATH="$HOME/.cargo/bin:$PATH"`. And `~/.npmrc`
  must hold `//registry.npmjs.org/:_authToken=…` (not `NPM_TOKEN=…`). A token
  also needs **2FA bypass** (granular w/ bypass, or automation token) to publish.
  CI avoids all this via OIDC trusted publishing — prefer cutting a tag.

### Publishing is now fully automated by a `v*` tag
Every channel publishes from the tag: GitHub release, PyPI (OIDC), npm (OIDC),
crates.io (`CARGO_REGISTRY_TOKEN`), winget, Scoop. **User action for crates.io:**
the `CARGO_REGISTRY_TOKEN` repo secret must hold a *fresh* token (the old one is
compromised). Only **R** (`milkway/rpic-r`) is still a separate manual repo.
- Possible future tidy: `publish=false` on `rpic-wasm` so a `cargo publish
  --workspace` can't trip on its path-only dep (the per-crate loop avoids this).

### Latest release state
- **v0.1.1** — first release cut entirely through the tag-driven pipeline.
  Pushing `v0.1.1` published **all** of crates.io (core/render/capi/cli),
  npm (`@strategicprojects/rpic`), PyPI (`rpiclang`), and the GitHub release —
  all four workflows green, all versions verified live. No manual publish steps.
  CI actions use `taiki-e/install-action` for wasm-pack (jetli's was Node-20
  deprecated).
- v0.1.0 — published mostly by hand (crates.io/npm manual) before automation.
- **R** (`milkway/rpic-r`) still lags: it's a separate repo, NOT tag-automated
  here. After an rpic release, bump its `DESCRIPTION` + `src/rust/Cargo.toml`
  deps + `Cargo.lock`, then tag/release there (pulls core/render from crates.io,
  so publish those first — which the pipeline now does). Currently at v0.1.0.
