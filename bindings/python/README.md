# rpiclang (Python)

Python bindings for [rpic](https://github.com/milkway/rpic-lang) — the pic
graphics language rendered to SVG / PNG / PDF, with animation manifests.

```sh
pip install rpiclang        # distribution name; you `import rpic`
```

```python
import rpic, json

svg = rpic.render_svg('box "hi"; arrow; circle "x"')
open("out.png", "wb").write(rpic.render_png("box \"hi\"", scale=2.0))
open("out.pdf", "wb").write(rpic.render_pdf("box \"hi\""))

# circuit library:
svg = rpic.render_svg('A:(0,0); B:(2,0)\nresistor(A,B)', circuits=True)

# svg + animation manifest:
bundle = json.loads(rpic.compile_json('box\nanimate last box with "pop"'))
```

## Build

```sh
pip install maturin
cd bindings/python
maturin develop --release     # installs into the current environment
# or: maturin build --release  → wheels in target/wheels/
```
