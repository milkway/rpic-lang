# tools/bench — render-to-SVG comparison

Reproducible benchmark behind the numbers on
[rpic.dev/docs/performance](https://rpic.dev/docs/performance): the same
chain-of-N-labelled-boxes diagram expressed in each tool's language,
timed as cold CLI invocations with [hyperfine](https://github.com/sharkdp/hyperfine).

```sh
cargo build --release -p rpic-cli   # from the repo root
tools/bench/run.sh                  # skips tools that aren't installed
```

Read the caveats on the site page before quoting numbers — the pic family
does explicit placement while graphviz/d2/mermaid compute the layout, which
is genuinely more work; and `mmdc` pays a headless-Chromium startup per
invocation that the mermaid library embedded in a live page does not.
