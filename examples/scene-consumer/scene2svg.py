#!/usr/bin/env python3
"""Draw an rpic scene (docs/scene.md) as plain SVG — no rpic required.

    rpic --scene figure.pic | python3 scene2svg.py > figure.svg

Proof that the scene format suffices to redraw a picture: stdlib only.
Style is minimal (solid colors; hatch/gradient fall back to one color).
"""
import json, sys
from xml.sax.saxutils import escape

SCENE = json.load(sys.stdin)
assert SCENE["scene"] == 1, "this consumer understands scene version 1"
CANVAS = SCENE["canvas"]
OX, OY = CANVAS["origin"]

def px(p):
    return (p[0] + OX) * 96.0, (CANVAS["h"] - (p[1] + OY)) * 96.0

def pts_attr(points):
    return " ".join(f"{x:g},{y:g}" for x, y in map(px, points))

def path_d(path):
    if "points" in path:
        pts = [px(p) for p in path["points"]]
        d = " ".join(f"{'M' if i == 0 else 'L'} {x:g} {y:g}" for i, (x, y) in enumerate(pts))
    else:
        (x, y), d = px(path["start"]), ""
        d = f"M {x:g} {y:g}"
        for cub in path["cubics"]:
            d += " C " + " ".join(f"{x:g} {y:g}" for x, y in map(px, cub))
    return d + (" Z" if path.get("closed") else "")

def stroke(item):
    s = item.get("stroke")
    if not s:
        return None
    attrs = f'stroke="{s["color"]}" stroke-width="{s["width"] * 96 / 72:g}"'
    d = s.get("dash")
    if d and d["kind"] == "dashed":
        attrs += f' stroke-dasharray="{d["len"] * 112:g},{d["len"] * 80:g}"'
    elif d:
        attrs += f' stroke-dasharray="0.5,{d.get("gap", 0.07) * 96:g}" stroke-linecap="round"'
    return attrs

def fill(item):
    f = item.get("fill")
    if not f:
        return None
    paint = f.get("color") or (f.get("hatch") or {}).get("color") or f["gradient"]["stops"][0]["color"]
    op = f' fill-opacity="{item["opacity"]:g}"' if "opacity" in item else ""
    return f'fill="{paint}"{op}'

W, H = CANVAS["w"] * 96, CANVAS["h"] * 96
print(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:g}" height="{H:g}"'
      f' viewBox="0 0 {W:g} {H:g}" font-family="sans-serif">')
for it in SCENE["items"]:
    if it["kind"] == "block" or it.get("hidden"):
        continue
    st, fi, shape, k = stroke(it), fill(it), None, it["kind"]
    if k in ("box", "circle", "dot", "ellipse"):
        x, y = px(it["at"])
        if k == "box":
            w, h = it["w"] * 96, it["h"] * 96
            rx = f' rx="{it["rad"] * 96:g}"' if "rad" in it else ""
            shape = f'<rect x="{x - w / 2:g}" y="{y - h / 2:g}" width="{w:g}" height="{h:g}"{rx}'
        elif k == "ellipse":
            shape = f'<ellipse cx="{x:g}" cy="{y:g}" rx="{it["w"] * 48:g}" ry="{it["h"] * 48:g}"'
        else:
            shape = f'<circle cx="{x:g}" cy="{y:g}" r="{it["r"] * 96:g}"'
    elif k in ("line", "arrow"):
        shape = f'<{"polygon" if it.get("closed") else "polyline"} points="{pts_attr(it["points"])}"'
    else:  # spline / arc / brace / text
        for p in it.get("paths", []):
            if fi:
                print(f'<path d="{path_d(p)}" {fi} stroke="none"/>')
            print(f'<path d="{path_d(p)}" fill="none" {st}/>')
    if shape and fi:
        print(f'{shape} {fi} stroke="none"/>')
    if shape and st:
        print(f'{shape} fill="none" {st}/>')
    for head in it.get("heads", []):  # painted in the stroke color
        color = it["stroke"]["color"]
        if "path" in head:
            print(f'<path d="{path_d(head["path"])}" fill="{color}"/>')
        elif head["filled"]:
            print(f'<polygon points="{pts_attr(head["points"])}" fill="{color}"/>')
        else:
            print(f'<polyline points="{pts_attr(head["points"])}" fill="none" {st}/>')
    for ln in it.get("text", []):  # `at` is the baseline anchor point
        x, y = px(ln["at"])
        anchor = {"w": "start", "e": "end"}.get(ln["anchor"][-1], "middle")
        f = ln["font"]
        extra = (f' font-family="{f["family"]}"' if "family" in f else "") \
            + (' font-weight="bold"' if f.get("bold") else "") \
            + (f' transform="rotate({-ln["rotation"]:g} {x:g} {y:g})"' if "rotation" in ln else "")
        print(f'<text x="{x:g}" y="{y:g}" font-size="{f["size"]:g}pt"{extra}'
              f' text-anchor="{anchor}" fill="black">{escape(ln["text"])}</text>')
print("</svg>")
