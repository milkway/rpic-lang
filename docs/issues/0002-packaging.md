# Issue: Packaging & distribution (deb, Homebrew, Windows installers)

**Labels:** enhancement, packaging, ci, release
**Status:** proposal / draft

## Summary

Ship `rpic` as easy-to-install packages for end users on the three major
platforms, built automatically from tagged releases. Because the core is pure
Rust with no system dependencies, every target reduces to "build a static-ish
binary and wrap it."

## Targets

### Release binaries (foundation)
- [ ] GitHub Actions release workflow that builds `rpic` for:
  - macOS `aarch64` + `x86_64` (universal2 optional)
  - Linux `x86_64` + `aarch64` (gnu; musl for fully static)
  - Windows `x86_64`
- [ ] Attach tarballs/zip + checksums to each GitHub Release.
- [ ] Consider [`cargo-dist`] to generate the whole workflow + installers.

### crates.io
- [ ] Publish `rpic-core`, `rpic-render`, `rpic-cli` so `cargo install rpic-cli` works.

### Debian / Ubuntu (.deb)
- [ ] [`cargo-deb`] to produce a `.deb` (binary + man page + completions).
- [ ] Publish via a GitHub Releases `.deb` and/or an APT repo (e.g. a `gh-pages`
      apt repo or Cloudsmith).

### Homebrew (macOS / Linux)
- [ ] Tap repo `milkway/homebrew-rpic` with a `rpic` formula that downloads the
      release binary (or builds from source). Later: submit to homebrew-core.

### Windows
- [ ] MSI installer via [`cargo-wix`], attached to releases.
- [ ] [Scoop] manifest (bucket) and/or [winget] manifest for `winget install`.

### Linux extras (optional)
- [ ] AppImage and/or a Nix flake / AUR `PKGBUILD`.

## Man page & shell completions
- [ ] Generate a man page and bash/zsh/fish completions (e.g. via `clap_mangen`
      / `clap_complete` if the CLI moves to clap) and include them in packages.

## Acceptance criteria

- A tagged release (`vX.Y.Z`) automatically produces: GitHub Release binaries +
  checksums, a `.deb`, an MSI, and updated Homebrew/Scoop manifests.
- `brew install milkway/rpic/rpic`, `cargo install rpic-cli`,
  `winget install rpic`, and `sudo apt install rpic` (from the repo) all work.

## Open questions

- `cargo-dist` (one tool, many installers) vs hand-rolled per-target jobs.
- Static musl builds for max Linux portability vs glibc.
- Code signing / notarization for macOS and Windows (certificates needed).
