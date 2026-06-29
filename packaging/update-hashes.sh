#!/usr/bin/env bash
# Generate the per-asset SHA256 from a published GitHub Release and fill the
# Homebrew formula + Scoop manifest, writing the results to packaging/dist/.
#
#   ./packaging/update-hashes.sh 0.0.1
#
# (The real hashes only exist once `release.yml` has uploaded the assets for the
# tag, so run this after a release is published.)
set -euo pipefail

VERSION="${1:?usage: $0 <version, e.g. 0.0.1>}"
REPO="${REPO:-milkway/rpic-lang}"
here="$(cd "$(dirname "$0")" && pwd)"
base="https://github.com/$REPO/releases/download/v$VERSION"
tmp="$(mktemp -d)"

sha() {
  curl -fsSL "$base/$1" -o "$tmp/$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$tmp/$1" | awk '{print $1}'
  else
    shasum -a 256 "$tmp/$1" | awk '{print $1}'
  fi
}

arm="$(sha rpic-aarch64-apple-darwin.tar.gz)"
intel="$(sha rpic-x86_64-apple-darwin.tar.gz)"
linux="$(sha rpic-x86_64-unknown-linux-gnu.tar.gz)"
win="$(sha rpic-x86_64-pc-windows-msvc.zip)"

mkdir -p "$here/dist"
sed -e "s/version \"[0-9.]*\"/version \"$VERSION\"/" \
    -e "s/SHA256_DARWIN_ARM/$arm/" \
    -e "s/SHA256_DARWIN_INTEL/$intel/" \
    -e "s/SHA256_LINUX/$linux/" \
    "$here/homebrew/rpic.rb" > "$here/dist/rpic.rb"
sed -e "s/\"version\": \"[0-9.]*\"/\"version\": \"$VERSION\"/" \
    -e "s|/v0.0.1/|/v$VERSION/|g" \
    -e "s/SHA256_WIN/$win/" \
    "$here/scoop/rpic.json" > "$here/dist/rpic.json"

echo "Wrote:"
echo "  $here/dist/rpic.rb"
echo "  $here/dist/rpic.json"
echo "darwin-arm  $arm"
echo "darwin-x64  $intel"
echo "linux-x64   $linux"
echo "windows-x64 $win"
