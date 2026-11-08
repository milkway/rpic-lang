# Render, verify, deliver

## Install (once)

| Platform | Command |
|---|---|
| macOS / Linux | `brew install milkway/rpic/rpic` |
| Any (Rust) | `cargo install rpic-cli` |
| Python | `pip install rpiclang` (`import rpic`) |
| JS | `npm i @strategicprojects/rpic` (WASM, browser and Node) |
| R | `install.packages("rpic")` |
| Windows | release zip / MSI at https://github.com/milkway/rpic-lang/releases |

Check: `rpic --version`. No binary? The Python or JS binding compiles the same engine;
and https://studio.rpic.dev renders any source pasted into it.

## The loop

1. **Write** `figure.pic` (the source is the deliverable; keep it tidy and commented).
2. **Render**: `scripts/render.sh figure.pic [--theme light|dark] [-c] [-t]` → `figure.svg`,
   `figure.png` (2×) and the warning list. By hand: `rpic --svg -o f.svg f.pic`,
   `rpic --png --scale 2 -o f.png f.pic`, `rpic --json f.pic` (warnings, objects).
3. **Look at the PNG** (the Read tool shows images). Check: nothing clipped at the edges,
   no label on a stroke, no two labels colliding, arrows meet the boxes, the one accent
   is where the eye should go. An SVG that compiles is not a figure that works.
4. **Fix and re-render.** Common fixes below.
5. **Deliver** the `.pic` beside the `.svg`/`.png`; say which flags render it
   (`-c`, `-t`, `--prelude theme=…`).

Warnings are part of the contract: a corpus figure is warning-clean. `--json` lists
them with `kind` (`invalid_color`, `ignored_attribute`, `unsupported_style`, …).

## Common fixes

| Symptom | Fix |
|---|---|
| Label clipped at the page edge | `margin = 0.15` (or `leftmargin`/`topmargin`…) |
| Text wider than its box | `box "…" fit`, or widen `wid`; `small` for sublabels |
| Label sits on the arrow | place it beside the segment: (`0.5 between A.e and B.w) + (0, 0.12)` |
| Arrow enters the box body | aim at a corner: `from A.e to B.w`, not `from A to B` (or `chop`) |
| Diagonal connector | route orthogonally: `from A.e to (B, A.e) then to B.n` |
| Two arrows share an attach point | offset: `A.e + (0, 0.1)` and `A.e - (0, 0.1)` |
| Container hides its children | draw it with `behind FirstChild`, or draw it first |
| Dark theme shows a white page | the SVG has no background: call `page(w, h)` first (dark theme) |
| `invalid_color` warning | use a CSS name, `"#rrggbb"`, `0xrrggbb` or `rgb()` |
| Positions off after a macro edit | labels inside `[ ]` are reached as `Blk.Label`; variables set inside a block are local (`:=` to export) |
| Circuit element unknown | add `-c` or `copy "circuits"` as the first line |
| `$…$` shows raw TeX | add `-t` (or `texlabels = 1`); one backslash per command in the pic source |

## Contact sheet (several figures)

```python
# python3 -I sheet.py out.png a.png b.png …  — stack PNGs with their names to review at once
import sys; from PIL import Image, ImageDraw
ims=[(p, Image.open(p).convert("RGB")) for p in sys.argv[2:]]; W=560
rows=[(p, im.resize((W, int(im.height*W/im.width)))) for p, im in ims]
H=sum(im.height+24 for _, im in rows); sheet=Image.new("RGB", (W, H), "white"); d=ImageDraw.Draw(sheet); y=0
for p, im in rows:
    d.text((6, y+5), p, fill="black"); y+=24; sheet.paste(im, (0, y)); y+=im.height
sheet.save(sys.argv[1])
```

## Other outputs

- `rpic --pdf -o f.pdf f.pic` — vector PDF (fonts embedded).
- `rpic --html -o page.html f.pic` — self-contained page that plays `animate` timelines.
- `rpic --json=all` / `--scene` — the resolved draw list as JSON for other backends.
- `rpic --geojson` — see `maps.md`.
- Open in the Studio to iterate visually: paste the source at https://studio.rpic.dev
  (it detects `-c`/`-t` from the source; a geo source opens the Map tab).
