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

After a release, regenerate the SHAs and refresh the manifests:

```sh
./packaging/update-hashes.sh 0.0.2     # fills packaging/dist/ from the release assets
```

To test the manifest rewrite without downloading release assets:

```sh
DIST_DIR=/tmp/rpic-packaging ./packaging/update-hashes.sh --dry-run 9.9.9
```

### Channels (status)

| Target | Install | Status |
|--------|---------|--------|
| **crates.io** | `cargo install rpic-cli` | ✅ published |
| **PyPI** | `pip install rpiclang` | ✅ published |
| **Homebrew** | `brew install milkway/rpic/rpic` | ✅ live — tap [milkway/homebrew-rpic](https://github.com/milkway/homebrew-rpic) |
| **Scoop** | `scoop install https://raw.githubusercontent.com/milkway/rpic-lang/main/packaging/scoop/rpic.json` | ✅ manifest filled (`scoop/rpic.json`) |
| **Debian** | download `.deb` from Releases, `sudo dpkg -i` | ✅ built per release |
| **winget** | `winget install milkway.rpic` | ⏳ manifests ready (`winget/`); needs a PR to `microsoft/winget-pkgs` |
| GitHub Releases | tarballs/zip/.deb/MSI | ✅ per tag |

Updating Homebrew on a new release: run `update-hashes.sh <ver>`, copy
`packaging/dist/rpic.rb` to `Formula/rpic.rb` in the tap repo, and push. The
Scoop manifest auto-updates via its `checkver`/`autoupdate` block.

### winget (no Windows required)

The winget PR is just YAML added to
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs); validation and
the sandbox install test run on Microsoft's CI, not your machine. Two ways, both
cross-platform:

1. **Automated** — `.github/workflows/winget.yml` runs the
   [WinGet Releaser](https://github.com/marketplace/actions/winget-releaser)
   action (Komac, on a GitHub-hosted Windows runner) on each published release.
   One-time setup: create a GitHub **PAT** (classic `public_repo`, or
   fine-grained with fork + PR) and add it as the repo secret **`WINGET_TOKEN`**.
   Then it auto-opens the PR; trigger it for v0.0.2 via *Actions → winget → Run
   workflow* (tag `v0.0.2`).
2. **From your Mac/Linux** — [Komac](https://github.com/russellbanks/Komac) is
   cross-platform:
   ```sh
   brew install komac
   komac update milkway.rpic --version 0.0.2 \
     --urls https://github.com/milkway/rpic-lang/releases/download/v0.0.2/rpic-x86_64-pc-windows-msvc.zip \
     --submit --token <YOUR_GITHUB_PAT>
   ```
   (Use `komac new milkway.rpic ...` for the very first submission.)

The `winget/` manifests here are a hand-written reference/fallback.

## Manual checks

```sh
cargo install cargo-deb && cargo deb -p rpic-cli      # → target/debian/*.deb
cargo install cargo-wix && cargo wix -p rpic-cli      # → target/wix/*.msi (Windows)
```

## Alternative: cargo-dist

[`cargo-dist`](https://opensource.axo.dev/cargo-dist/) can generate the whole
release workflow plus shell/PowerShell/Homebrew/MSI installers from one config
(`dist init`). Considered as a follow-up to replace the hand-rolled workflow.
