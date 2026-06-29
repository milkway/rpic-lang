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

The winget manifests in `winget/` are submitted by opening a PR to
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) (e.g. with
[`wingetcreate`](https://github.com/microsoft/winget-create)); this is a manual,
externally-reviewed step.

## Manual checks

```sh
cargo install cargo-deb && cargo deb -p rpic-cli      # → target/debian/*.deb
cargo install cargo-wix && cargo wix -p rpic-cli      # → target/wix/*.msi (Windows)
```

## Alternative: cargo-dist

[`cargo-dist`](https://opensource.axo.dev/cargo-dist/) can generate the whole
release workflow plus shell/PowerShell/Homebrew/MSI installers from one config
(`dist init`). Considered as a follow-up to replace the hand-rolled workflow.
