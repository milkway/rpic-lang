# Contributing to rpic

Thank you for your interest in rpic. Bug reports, feature discussions and
pull requests are all welcome.

## Reporting bugs and asking questions

Open an issue at <https://github.com/milkway/rpic-lang/issues>. For a bug,
include:

- the smallest `.pic` source that reproduces the problem,
- the exact command line (`rpic --svg …`, flags matter — `-c` and `-t`
  change the pipeline),
- the output you got and the output you expected,
- `rpic --version` and your platform.

If the figure renders differently from Aplevich's `dpic`, say so — dialect
compatibility reports are especially valuable, and `dpic` output is our
reference oracle.

For questions that are not bugs, open an issue as well; there is no
separate forum. Private contact: <leite@de.ufpe.br>.

## Building and testing

```sh
cargo build --release -p rpic-cli        # binary at target/release/rpic
cargo test                               # unit tests + corpus drift guard
cargo clippy --all-targets               # CI runs with -D warnings
cargo fmt --all --check
```

`bindings/python` and `crates/wasm` are standalone workspaces; lint them
with explicit `--manifest-path`. JS: `cd bindings/js && npm run build:wasm
&& npm test`. Python: `maturin develop` then `pytest tests`.

All four commands above must pass before a pull request is merged.

## The compatibility rules (please read before touching the language)

rpic's central promise is compatibility, kept by two mechanical checks:

1. **Corpus byte-identity.** Every `.pic` under `examples/` and `assets/`
   has a committed sibling `.svg` rendered with `-c`. `cargo test` re-renders
   every pair and fails on any byte difference. If your change legitimately
   moves bytes, regenerate the siblings and *look at the figures* before
   committing — a diff you have not seen is not a fix.
2. **Extension inertness.** New language features must be contextual
   keywords, and a binary with the feature must produce byte-identical
   output to a binary without it on every corpus input that does not use
   the feature (checked over all examples with `-c -t --svg` and `--json`).

For dialect questions (what should this construct do?), run `dpic` and
believe it over documentation or intuition.

## Pull requests

- Branch from `main`; direct pushes to `main` are blocked.
- Keep PRs focused; one feature or fix per PR.
- New features need tests at every affected layer and a documentation
  page under `site/` (examples there are compiled by the real binary at
  build time, so they cannot rot).
- CI must be green; PRs are squash-merged.

## Licensing

rpic is licensed under BSD-2-Clause. By contributing you agree that your
contributions are licensed under the same terms.
