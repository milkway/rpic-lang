#!/usr/bin/env python3
"""Generate the equivalent benchmark diagrams for every tool.

The workload is a chain of N labelled boxes joined by arrows — the same
semantic content expressed in each tool's own language. Sizes: small (5),
medium (30), large (120). Outputs land in the working directory.
"""
import pathlib


def gen(n: int):
    pic = [".PS"]
    for i in range(n):
        if i:
            pic.append("arrow")
        pic.append(f'box "N{i}"')
    pic.append(".PE")

    dot = ["digraph G { rankdir=LR; node [shape=box];"]
    dot += [f'n{i} [label="N{i}"];' for i in range(n)]
    dot += [f"n{i} -> n{i + 1};" for i in range(n - 1)]
    dot.append("}")

    d2 = ["direction: right"]
    d2 += [f"n{i}: N{i}" for i in range(n)]
    d2 += [f"n{i} -> n{i + 1}" for i in range(n - 1)]

    mmd = ["flowchart LR"]
    mmd += [f'  n{i}["N{i}"]' for i in range(n)]
    mmd += [f"  n{i} --> n{i + 1}" for i in range(n - 1)]

    return pic, dot, d2, mmd


for name, n in [("small", 5), ("medium", 30), ("large", 120)]:
    pic, dot, d2, mmd = gen(n)
    pathlib.Path(f"{name}.pic").write_text("\n".join(pic) + "\n")
    # pikchr speaks pic without the .PS/.PE markers
    pathlib.Path(f"{name}.pikchr").write_text("\n".join(pic[1:-1]) + "\n")
    pathlib.Path(f"{name}.dot").write_text("\n".join(dot) + "\n")
    pathlib.Path(f"{name}.d2").write_text("\n".join(d2) + "\n")
    pathlib.Path(f"{name}.mmd").write_text("\n".join(mmd) + "\n")

print("generated: {small,medium,large}.{pic,pikchr,dot,d2,mmd}")
