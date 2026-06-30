# Figuras — circuit_macros figures by André Leite

These are figures from **André Leite's** personal collection *"minhas lindas
figuras"* (Recife, February 2011) — diagrams he made over ~10 years. Part of the
collection was written for **circuit_macros** (the `m4` macro package by
J. D. Aplevich), which is part of the *pic* family; the rest were PStricks.

The circuit_macros figures that **rpic renders** are collected here, one `.pic`
per figure (the numbers match the figures in the original `Figuras.pdf`).

**Figures that draw with raw primitives** (line/circle/…) are self-contained —
render with plain rpic:

```sh
rpic --svg examples/figuras/fig01.pic -o fig01.svg
```

**Figures that use the circuit_macros element API** (`resistor(up_ dimen_)`,
`bi_tr(…)`, `opamp(…)`, …) need the native circuit library (`-c`); they pull in
the compatibility shim with `copy "circuit_macros.pic"`:

```sh
rpic -c --svg examples/figuras/fig30.pic -o fig30.svg
```

## How they were adapted

The original sources are circuit_macros `m4`. To run them in rpic, each file is
prefixed with a small **circuit_macros-compatibility shim**
([`circuit_macros.pic`](circuit_macros.pic)) that:

- neutralises `include(libcct.m4)` and `cct_init` (no-ops);
- defines the base dimension `dimen_` and the direction aliases
  `right_`/`left_`/`up_`/`down_`;
- adapts the circuit_macros **direction+length element API** to rpic's native
  **two-point** circuit library — *reusing the same native geometry*. Each
  linear element (`resistor`/`capacitor`/`inductor`/`diode`) draws an invisible
  spine along its direction and then calls the native two-point form; `bi_tr`
  and `opamp` are blocks exposing `.B/.E/.C` and `.In1/.In2/.Out` terminals,
  built on sign-parameterized versions of the native `npn`/`pnp`/`opamp` (so the
  reflected `R` and `down_`/`left_` orientations come for free).

In addition:

- **PStricks colour directives** (`\newrgbcolor`, `\psset`, …) are removed — rpic
  targets SVG, so the geometry renders but the original colours are not applied.
- **LaTeX math labels** (`"$\omega$"`, `"$Q_4$"`, …) render as **literal text**;
  rpic does not typeset math. When such a label is passed *unquoted* through a
  macro (e.g. `dimension_(…, $\beta V$, …)`), the inter-word spaces are not
  preserved (`$\beta V$` → `$\beta V$` reads as `$\betaV$`).

So these are **geometry-faithful** renderings of the originals, not pixel-perfect
reproductions.

## Coverage

**All 48** of the collection's circuit_macros figures render. 27 draw with raw
primitives; the other 21 exercise the element-API compatibility shim:

- linear elements, bipolar transistors (`bi_tr`), op-amps (`opamp`), element
  boxes (`ebox`), current sources (`source`);
- `with .start at …` element placement, lines continued across a newline around
  `then`;
- the `dimension_` annotation macro from circuit_macros' `libgen.m4`
  (`fig09 11 14 48`).

## A few highlights

![fig01](fig01.svg)
![fig30](fig30.svg)
![fig33](fig33.svg)
![fig45](fig45.svg)

## Credit

Figures © **André Leite** (`leite.andre@gmail.com`), from *"minhas lindas
figuras"*, 2011. circuit_macros © J. D. Aplevich (see the top-level
[`ACKNOWLEDGMENTS.md`](../../ACKNOWLEDGMENTS.md)).
