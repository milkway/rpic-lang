#!/usr/bin/env bash
# Render a .pic to SVG and PNG and report the engine's warnings.
#   scripts/render.sh figure.pic [--theme light|dark] [-c] [-t] [--scale N]
# Output: figure.svg, figure.png next to the source; warnings on stderr; exit 1 on error.
set -eo pipefail
src=""; theme=""; flags=(); scale=2
while [ $# -gt 0 ]; do
  case "$1" in
    --theme) theme="$2"; shift 2 ;;
    --scale) scale="$2"; shift 2 ;;
    -c|-t) flags+=("$1"); shift ;;
    *) src="$1"; shift ;;
  esac
done
[ -n "$src" ] || { echo "usage: render.sh figure.pic [--theme light|dark] [-c] [-t] [--scale N]" >&2; exit 2; }
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# --prelude resolves a relative FILE against the figure's directory, so pass it absolute
if [ -n "$theme" ]; then flags+=(--prelude "theme=$here/../assets/editorial-$theme.pic"); fi
base="${src%.pic}"
# ${flags[@]+...} keeps bash 3.2 (macOS) happy when no flag was given
rpic ${flags[@]+"${flags[@]}"} --svg -o "$base.svg" "$src"
rpic ${flags[@]+"${flags[@]}"} --png --scale "$scale" -o "$base.png" "$src"
# warnings ride in the JSON bundle (a compile error too, with exit 0): the corpus convention is warning-clean
rpic ${flags[@]+"${flags[@]}"} --json "$src" | python3 -I -c '
import json, sys
d = json.load(sys.stdin)
if "error" in d:
    print("error: " + str(d["error"]), file=sys.stderr); sys.exit(1)
for w in d.get("warnings", []):
    pos = "%s:%s" % (w.get("line"), w.get("col")) if w.get("line") else "-"
    print("warning %s at %s: %s" % (w.get("kind"), pos, w.get("message")), file=sys.stderr)
print("ok: %d objects, %d warnings" % (len(d.get("objects", [])), len(d.get("warnings", []))))
'
echo "wrote $base.svg $base.png"
