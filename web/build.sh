#!/usr/bin/env bash
# Build the rpic WASM package into web/pkg.
#
# Uses the rustup toolchain (which has the wasm32-unknown-unknown std), since a
# Homebrew rustc may be first on PATH without it.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"

# Prefer the rustup stable toolchain + cargo-installed wasm-pack.
RUSTUP_BIN="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin"
export PATH="$RUSTUP_BIN:$HOME/.cargo/bin:$PATH"

command -v wasm-pack >/dev/null || { echo "wasm-pack not found. Install: cargo install wasm-pack"; exit 1; }

wasm-pack build "$repo/crates/wasm" --target web --out-dir "$repo/web/pkg"
echo
echo "Done. Serve the playground with:"
echo "    cd $repo/web && python3 -m http.server 8080"
echo "Then open http://localhost:8080/"
