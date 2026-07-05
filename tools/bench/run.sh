#!/usr/bin/env bash
# Render-to-SVG benchmark: rpic vs the pic family and auto-layout tools.
#
# Requirements: hyperfine, plus whichever tools you want to compare —
#   rpic (RPIC env var or ../../target/release/rpic), dpic, pikchr,
#   dot (graphviz), d2, mmdc (@mermaid-js/mermaid-cli).
# Missing tools are skipped. Results: bench-<size>.json + stdout summary.
set -euo pipefail
cd "$(dirname "$0")"

RPIC="${RPIC:-../../target/release/rpic}"
python3 gen.py

for size in small medium large; do
  cmds=()
  [ -x "$RPIC" ]            && cmds+=("$RPIC --svg $size.pic")
  command -v dpic   >/dev/null && cmds+=("dpic -v < $size.pic")
  command -v pikchr >/dev/null && cmds+=("pikchr --svg-only $size.pikchr")
  command -v dot    >/dev/null && cmds+=("dot -Tsvg $size.dot")
  command -v d2     >/dev/null && cmds+=("d2 $size.d2 /tmp/bench-d2.svg")
  command -v mmdc   >/dev/null && cmds+=("mmdc -i $size.mmd -o /tmp/bench-mmd.svg")
  echo "===== $size (${#cmds[@]} tools)"
  hyperfine --warmup 2 --export-json "bench-$size.json" "${cmds[@]}"
done

echo "===== batch: 50 small diagrams in sequence (docs-pipeline cost)"
hyperfine --warmup 1 \
  "for i in {1..50}; do $RPIC --svg small.pic > /dev/null; done" \
  $(command -v mmdc >/dev/null && echo '"for i in {1..50}; do mmdc -i small.mmd -o /tmp/bench-mmd.svg >/dev/null 2>&1; done"')
