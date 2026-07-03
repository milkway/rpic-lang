#!/usr/bin/env bash
# Generate the per-asset SHA256 from a published GitHub Release and fill the
# Homebrew formula + Scoop manifest, writing the results to packaging/dist/.
#
#   ./packaging/update-hashes.sh 0.0.1
#   ./packaging/update-hashes.sh --dry-run 9.9.9
#
# (The real hashes only exist once `release.yml` has uploaded the assets for the
# tag, so run this after a release is published.)
set -euo pipefail

usage() {
  echo "usage: $0 [--dry-run] <version, e.g. 0.0.1>" >&2
}

DRY_RUN=0
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=1
  shift
fi

if [[ $# -ne 1 ]]; then
  usage
  exit 2
fi

VERSION="$1"
REPO="${REPO:-milkway/rpic-lang}"
here="$(cd "$(dirname "$0")" && pwd)"
base="https://github.com/$REPO/releases/download/v$VERSION"
dist="${DIST_DIR:-$here/dist}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

sha() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    case "$1" in
      rpic-aarch64-apple-darwin.tar.gz) printf '%064d\n' 1 ;;
      rpic-x86_64-apple-darwin.tar.gz) printf '%064d\n' 2 ;;
      rpic-x86_64-unknown-linux-gnu.tar.gz) printf '%064d\n' 3 ;;
      rpic-x86_64-pc-windows-msvc.zip) printf '%064d\n' 4 ;;
      *) echo "unknown dry-run asset: $1" >&2; exit 1 ;;
    esac
    return
  fi

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

mkdir -p "$dist"
sed -e "s/version \"[0-9.]*\"/version \"$VERSION\"/" \
    -e "s/SHA256_DARWIN_ARM/$arm/" \
    -e "s/SHA256_DARWIN_INTEL/$intel/" \
    -e "s/SHA256_LINUX/$linux/" \
    "$here/homebrew/rpic.rb" > "$dist/rpic.rb"
awk -v version="$VERSION" \
    -v url="$base/rpic-x86_64-pc-windows-msvc.zip" \
    -v hash="$win" '
      /"version": "[^"]*"/ {
        sub(/"version": "[^"]*"/, "\"version\": \"" version "\"")
      }
      !url_done && /"url": "https:\/\/github.com\/[^"]*\/releases\/download\/v[^"]*\/rpic-x86_64-pc-windows-msvc.zip"/ {
        sub(/"url": "[^"]*"/, "\"url\": \"" url "\"")
        url_done = 1
      }
      !hash_done && /"hash": "[^"]*"/ {
        sub(/"hash": "[^"]*"/, "\"hash\": \"" hash "\"")
        hash_done = 1
      }
      { print }
    ' "$here/scoop/rpic.json" > "$dist/rpic.json"

require_line() {
  local needle="$1"
  local file="$2"
  local message="$3"
  if ! grep -F "$needle" "$file" >/dev/null; then
    echo "validation failed: $message" >&2
    exit 1
  fi
}

require_line "version \"$VERSION\"" "$dist/rpic.rb" "Homebrew version is not $VERSION"
require_line "sha256 \"$arm\"" "$dist/rpic.rb" "Homebrew arm64 hash was not updated"
require_line "sha256 \"$intel\"" "$dist/rpic.rb" "Homebrew x86_64 macOS hash was not updated"
require_line "sha256 \"$linux\"" "$dist/rpic.rb" "Homebrew Linux hash was not updated"
require_line "\"version\": \"$VERSION\"" "$dist/rpic.json" "Scoop version is not $VERSION"
require_line "\"url\": \"$base/rpic-x86_64-pc-windows-msvc.zip\"" "$dist/rpic.json" "Scoop URL is not v$VERSION"
require_line "\"hash\": \"$win\"" "$dist/rpic.json" "Scoop hash was not updated"
require_line "/v\$version/rpic-x86_64-pc-windows-msvc.zip" "$dist/rpic.json" "Scoop autoupdate URL lost its version token"

echo "Wrote:"
echo "  $dist/rpic.rb"
echo "  $dist/rpic.json"
echo "darwin-arm  $arm"
echo "darwin-x64  $intel"
echo "linux-x64   $linux"
echo "windows-x64 $win"
