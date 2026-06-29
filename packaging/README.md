# Packaging

Distribution config for `rpic`. Tracks issue
[#2](https://github.com/milkway/rpic-lang/issues/2).

## How it fits together

A git tag `vX.Y.Z` triggers `.github/workflows/release.yml`, which:

1. **Binaries** — builds `rpic` for macOS (arm64/x86_64), Linux (x86_64) and
   Windows (x86_64), and uploads `.tar.gz`/`.zip` archives to the GitHub Release.
2. **Debian** — `cargo deb -p rpic-cli` produces a `.deb` (config lives in
   `crates/cli/Cargo.toml` under `[package.metadata.deb]`) and uploads it.
3. **Windows MSI** — `cargo wix` produces an installer (best-effort).

After a release, fill the SHAs into the manifests below and publish them.

| Target | File | Publish to |
|--------|------|-----------|
| Homebrew | `homebrew/rpic.rb` | tap repo `milkway/homebrew-rpic` |
| Scoop | `scoop/rpic.json` | a Scoop bucket |
| winget | *(generate from release)* | `microsoft/winget-pkgs` |
| crates.io | — | `cargo publish` (rpic-core, rpic-render, rpic-cli) |

## Manual checks

```sh
cargo install cargo-deb && cargo deb -p rpic-cli      # → target/debian/*.deb
cargo install cargo-wix && cargo wix -p rpic-cli      # → target/wix/*.msi (Windows)
```

## Alternative: cargo-dist

[`cargo-dist`](https://opensource.axo.dev/cargo-dist/) can generate the whole
release workflow plus shell/PowerShell/Homebrew/MSI installers from one config
(`dist init`). Considered as a follow-up to replace the hand-rolled workflow.
