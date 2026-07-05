# rpiclang (Python)

Python bindings for [rpic](https://github.com/milkway/rpic-lang) — the pic
graphics language rendered to SVG / PNG / PDF, with animation manifests.

```sh
pip install rpiclang        # distribution name; you `import rpic`
```

```python
import rpic

svg = rpic.render_svg('box "hi"; arrow; circle "x"')
open("out.png", "wb").write(rpic.render_png("box \"hi\"", scale=2.0))
open("out.pdf", "wb").write(rpic.render_pdf("box \"hi\""))

# circuit library (or write `copy "circuits"` in the source itself):
svg = rpic.render_svg('A:(0,0); B:(2,0)\nresistor(A,B)', circuits=True)

# TeX math labels, exactly like `rpic -t`:
svg = rpic.render_svg('box "$-\\frac{T}{2}$" fit', texlabels=True)

# the parsed bundle: svg + animation manifest + diagnostics + warnings
bundle = rpic.compile('box\nanimate last box with "pop"')
bundle["animations"]   # [{"id": "s0", "effect": "pop", ...}]
bundle["diagnostics"]  # lines emitted by pic `print`
bundle["warnings"]     # structured warnings (ignored attributes, ...)
bundle["objects"]      # per-object geometry: {"id": "s0", "kind": "box",
                       #   "bbox": {x,y,w,h} | None, "line", "col", ...}
# (compile_json returns the same as a JSON string)

# `copy "file"` includes resolve relative to `base`:
svg = rpic.render_svg('copy "shim.pic"\nbox', base="path/to/dir")

# compiling untrusted source? fence or disable filesystem includes
# ("sandboxed" = only files inside `base`; "deny" = none at all;
#  the embedded `copy "circuits"` library always works):
svg = rpic.render_svg(user_source, base="jobs/42", include_policy="sandboxed")
```

Compile errors raise `rpic.CompileError` (a `ValueError` subclass, so old
`except ValueError` code keeps working). `str(exc)` is the readable message;
`exc.info` is the structured diagnostic for editors:

```python
try:
    rpic.render_svg("bxo", circuits=True)
except rpic.CompileError as exc:
    exc.info  # {"message": ..., "line": 1, "col": 1, "end_col": 4,
              #  "file": None, "kind": "expected_token", "found": "`bxo`",
              #  "expected": "an object", "hint": "did you mean `box`?"}
```

Positions are always relative to **your** source — with `circuits=True` an
error on your line 1 reports line 1, and a problem inside a `copy` include
names it in `info["file"]` (`None` means your own input).

## Test

```sh
maturin develop --release && pytest tests -q
```

## Build

```sh
pip install maturin
cd bindings/python
maturin develop --release     # installs into the current environment
# or: maturin build --release  → wheels in target/wheels/
```
