---
title: 'rpic: a memory-safe implementation of the pic diagram language with SVG, PDF and animated output'
tags:
  - Rust
  - diagrams
  - domain-specific language
  - SVG
  - WebAssembly
  - reproducible research
authors:
  - name: André Leite
    orcid: 0000-0002-4718-9766
    corresponding: true
    affiliation: 1
  - name: Raydonal Ospina
    # TODO: ORCID
    affiliation: 2
  - name: Hugo Vasconcelos
    # TODO: ORCID
    affiliation: 3
  - name: Diogo Bezerra
    # TODO: ORCID
    affiliation: 1
affiliations:
  - name: Departamento de Estatística, Universidade Federal de Pernambuco, Recife, Brazil
    index: 1
  - name: Departamento de Estatística, Universidade Federal da Bahia, Salvador, Brazil
    index: 2
  - name: Secretaria de Planejamento e Gestão, Governo do Estado de Pernambuco, Recife, Brazil
    index: 3
date: 18 July 2026
bibliography: paper.bib
---

# Summary

`pic` is a small language for describing diagrams as text: a figure is a
program, so it can be versioned, diffed, generated, and reproduced exactly
[@kernighan1982pic]. The language survives today chiefly through Aplevich's
`dpic` and the `circuit_macros` library built on it [@aplevich_cm], the
standard tool for circuit diagrams in technical publishing.

`rpic` is a new implementation of the pic language, written in Rust, that
targets the `dpic` dialect. One compiler core drives every deployment
target: a command-line binary, a C ABI, a WebAssembly module, and Python,
JavaScript and R bindings. It emits SVG, PNG and PDF, and — through an
opt-in `animate` extension — compiles animation statements into a JSON
timeline that a bundled browser player executes, turning a static diagram
language into an animated one. A native reimplementation of the
`circuit_macros` element geometry is included, so circuit figures compile
without the original m4 toolchain.

Compatibility is treated as a falsifiable property rather than a goal:
every language question is settled by running `dpic` as an oracle, and
agreement is frozen into a corpus of 124 figures whose rendered output is
byte-identical, re-checked by the test suite on every change. All rpic
extensions are contextual keywords proven byte-inert when unused — a
document that does not use them renders identically to the pure dialect.

# Statement of need

Text-described figures matter wherever documents are built like software:
statistical reports, teaching material, package vignettes and
documentation pipelines, where figures must be reviewed in diffs and
regenerated from source. The existing pic toolchain predates that world.
`dpic` and `circuit_macros` require an m4 macro processor and a TeX-centred
workflow; they cannot run in a browser, embed cleanly in a host language,
or be handed untrusted input safely; and their error reporting predates
structured diagnostics. `pikchr` [@hipp_pikchr] brought pic to the web but
deliberately breaks compatibility with the `dpic` dialect, leaving
`circuit_macros` users behind.

`rpic` serves users who need the established dialect in modern hosts: a
statistician embedding circuit or flow diagrams in an R vignette or a
Python notebook, a documentation site compiling figures at build time, a
web application rendering user-supplied diagrams under a sandboxed include
policy, or a lecturer animating the construction of a figure step by step.
Diagnostics are structured (message, source span, file provenance, hint)
and positions never shift when libraries are loaded, which makes the
compiler usable inside editors — a live web editor built on the WebAssembly
module runs at <https://studio.rpic.dev>.

The differential-testing discipline [@mckeeman1998differential] gives the
project its quality claim: full parity with the canonical `dpic` example
corpus (72/72 figures) and with a real `circuit_macros` figure collection
(48/48), byte-for-byte. Documentation, with every example compiled by the
real binary at build time, lives at <https://rpic.dev>.

<!-- TODO: one figure — a circuit example with its pic source, e.g.
![A circuit_macros figure compiled by rpic.](figure.svg) -->

# Acknowledgements

rpic stands on three predecessors: Brian W. Kernighan's original pic,
J. Dwight Aplevich's dpic and circuit_macros — whose `dpic` served as the
reference oracle throughout — and D. Richard Hipp's pikchr, which showed
that pic belongs in the browser.

# References
