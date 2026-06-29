# Acknowledgments

`rpic` stands entirely on the shoulders of giants. It exists only because of the
foundational work of three people, whom we gratefully and prominently credit:

## Brian W. Kernighan

Creator of **pic**, the picture-drawing language whose paradigm `rpic` preserves
faithfully — the idea of describing a drawing by "walking around a plane dropping
primitives," with relative positioning, default dimensions, compass corners,
ordinals and blocks. His paper *“PIC — A Language for Typesetting Graphics”*
(Software—Practice & Experience, vol. 12, 1–21, 1982) is the design north star
for this project. Every aspect of the language here traces back to his work.

## Dwight (J. D.) Aplevich

Author of **dpic** (https://gitlab.com/aplevich/dpic), the modern C
implementation of pic, and of **circuit_macros**
(https://gitlab.com/aplevich/circuit_macros), the renowned library for drawing
circuits and diagrams. dpic's mature SVG and PDF backends served as the
reference specification for ours, and the geometry of its decades-tested output
guided countless decisions. Our native circuit-element library is an independent
re-authoring inspired in spirit by circuit_macros. dpic is BSD-2-Clause;
circuit_macros is LPPL.

## D. Richard Hipp

Author of **pikchr** (https://pikchr.org), the modern PIC-like language that
compiles directly to SVG. pikchr demonstrated that a pic-family language can be a
clean, self-contained, SVG-first tool for the web era — exactly the direction
`rpic` takes. His work showed the path is both viable and worthwhile.

---

## Bundled font

The PNG/PDF backends embed the **Go font** by **Bigelow & Holmes**
(BSD-3-Clause) so text rasterizes identically everywhere, with no dependency on
installed system fonts. See [`crates/render/fonts/LICENSE`](crates/render/fonts/LICENSE).

---

Any merit in `rpic` is owed to them; any mistakes are ours alone.
