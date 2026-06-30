# lib3D — 3D drawing, projected to 2D

[`lib3d.pic`](lib3d.pic) is a small compatibility layer for circuit_macros'
**`lib3D.m4`** (J. D. Aplevich). lib3D is *not* a 3D renderer: you give 3D
coordinates and a viewpoint, and it **projects** each 3D point onto the 2D
drawing plane (an axonometric projection), then draws with ordinary pic
primitives. The result is flat 2D line art — exactly what rpic renders to SVG —
so no 3D backend is needed, only the projection math.

This shim reimplements that core in rpic's own expression language, reusing the
native 2D primitives (the same "reuse, don't reinvent" approach as the
[circuit_macros element shim](../figuras/circuit_macros.pic)).

## Use

```pic
copy "lib3d.pic"
.PS
setview(35, 20)          # azimuth, elevation in degrees (optional 3rd: roll)
axis3D(2)                # labelled x / y / z axes
box3D(1.4, 1, 0.8)       # wireframe box from the origin to (a, b, c)
.PE
```

Render with plain rpic (no `-c` needed — lib3D uses no circuit elements):

```sh
rpic --svg examples/lib3d/frame.pic -o frame.svg
```

## Macros

| Macro | Meaning |
|-------|---------|
| `setview(az, el [, roll])` | set the viewing angles (degrees) |
| `Project(x, y, z)` | the 3D point as a 2D coordinate `(u, v)` — usable anywhere a position is |
| `axis3D(len)` | labelled x/y/z axes from the origin |
| `box3D(a, b, c)` | wireframe box, origin to `(a, b, c)` |

`Project` is the heart of it; everything else is built from it. To draw your own
3D shapes, project each vertex and connect them:

```pic
line from Project(0,0,0) to Project(1,0,0) to Project(1,1,1)
```

## Examples

![frame](frame.svg)

The same cube from three viewpoints (`setview` reused):

![views](views.svg)

## Scope

This covers the **projection core** (`setview` / `Project`) plus simple wireframe
helpers — enough to place and connect 3D points and render the result as SVG.
The projection follows lib3D's convention (rotate by −azimuth about *z*, by
elevation about *y*, by roll about *x*; the 2D plane is the resulting *y–z*).
lib3D's heavier machinery (perspective, surface shading / hidden-line removal,
parametric surfaces) is not implemented.

## Credit

lib3D © **J. D. Aplevich**, part of
[circuit_macros](https://gitlab.com/aplevich/circuit_macros); see the top-level
[`ACKNOWLEDGMENTS.md`](../../ACKNOWLEDGMENTS.md).
