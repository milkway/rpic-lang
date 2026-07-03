# rpic docs site

The docs site renders `.pic` examples during the Astro build by calling the
real `rpic` binary. Set `RPIC_BIN` to choose the binary used by the build:

```sh
RPIC_BIN=../target/release/rpic npm run build
```

Rendered SVG/JSON outputs are cached in `node_modules/.rpic-cache`. Cache keys
include the source, render options, literal `copy "file"` dependencies, the
resolved `rpic` binary path/size/mtime, and `RPIC_CACHE_BUST`.

Use `RPIC_CACHE_BUST` to force a local rebuild without deleting dependencies:

```sh
RPIC_CACHE_BUST=$(date +%s) npm run build
```
