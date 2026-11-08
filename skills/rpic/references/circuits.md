# Circuits with rpic

rpic ships a native circuit-element library (the `-c` flag, or `copy "circuits"` as the
first line of the source — the in-source spelling, works everywhere including the
browser). It is the circuit_macros vocabulary (Aplevich) drawn natively: two-terminal
elements take two points and draw the wiring, multi-terminal devices take a centre.

Render: `rpic -c --svg fig.pic` (add `-t` for `$R_1$` math labels). Studio:
https://studio.rpic.dev detects the library from the source.

## Two-terminal elements — `element(P, Q)`

`resistor capacitor inductor diode led zener schottky battery fuse switch spdt
pushbutton lamp meter voltmeter ammeter ohmmeter crystal speaker antenna motor
generator bell thermistor varistor ldr potentiometer polcap varcap varind
voltage_source current_source ac_source vsource_ctrl isource_ctrl isource tline bus
wire hop` — each draws from P to Q, the symbol centred, the leads filling the rest.

```pic -c
A: (0,0); B: (1.4,0); C: (2.8,0); D: (2.8,-1.2); E: (0,-1.2)
resistor(A, B);      "R1" above at (0.5 between A and B) + (0, 0.26)
capacitor(B, C);     "C1" above at (0.5 between B and C) + (0, 0.26)
wire(C, D)
battery(D, E);       "V1" below at (0.5 between D and E) + (0, -0.3)
wire(E, A)
dot at B
ground(E)
```

Rules that keep a schematic readable:

- Lay the circuit on a **0.5 in grid**: nodes at multiples of 0.5, elements 1.0–1.5 in long.
- Label every element once, offset 0.18–0.22 in from the lead, never on the symbol.
- `dot at P` marks a junction; a wire crossing without a dot is no connection.
- `ground(P)` hangs the ground symbol below P; `terminal(P)` draws an open terminal.
- The default stroke is right for print; use `thick` only for a highlighted path.

## Multi-terminal devices — `device(Center)`

Transistors `npn pnp nmos pmos njfet pjfet phototransistor`, `opamp`, gates
`and_gate or_gate nand_gate nor_gate xor_gate xnor_gate not_gate buffer` (+ IEEE
`ieee_and ieee_or ieee_xor ieee_buf`, `iecgate`), `transformer relay mux ic_block`.
Each exports its pins as **variables** after the call: `gBase_x/gBase_y gColl_* gEmit_*`
(BJT), `gGate_* gDrain_* gSrc_*` (FET), `gA_* gB_* gY_*` (gates; `gA`/`gY` for `not_gate`
and `buffer`), `gIm_* gIp_* gY_*` (opamp: inverting, non-inverting, output), `gP1_* gP2_*
gS1_* gS2_*` (transformer), `gW_*` (potentiometer wiper). Wire from them:

```pic -c
margin = 0.1
Q1: npn((0,0))
line left 0.4 from (gBase_x, gBase_y); B: last line.end; "B" above at B + (0, 0.08)
line up 0.4 from (gColl_x, gColl_y);   "C" above at last line.end
line down 0.4 from (gEmit_x, gEmit_y); ground(last line.end)
Bin: B - (1.2, 0)                       # name the far end: macro arguments are places, not expressions
resistor(B, Bin); "Rb" above at (0.5 between B and Bin) + (0, 0.18)
```

For pins as **labels** (`Q1.B`, `Q1.C`) use the circuit_macros compatibility shim
(`examples/figuras/circuit_macros.pic` in the engine repo), which wraps devices as blocks.

## Composition patterns

- **Series chain**: name the nodes first (`A: (0,0); B: A + (1.5,0); …`), then elements
  between them; the wiring follows for free.
- **Parallel branch**: two nodes, several elements between offset points joined by `wire`.
- **Loops/arrays**: `for i = 0 to 3 do { resistor((i*1.5, 0), (i*1.5 + 1.5, 0)) }`.
- **Value labels**: `"4.7 kΩ" mono small` beside the name, or `-t` with `"$4.7\,\mathrm{k}\Omega$"`.
- **Highlighting**: one `thick 1.6 outlined accent` path (a current loop, the feedback
  path), everything else default — same focal rule as the diagrams.

## Simulation (Studio)

A figure marked `spice = 1` that uses the dune prelude's elements (`vsource`, `isource`,
`acsource`, plus the `-c` names) gets a **Simulate** tab in the Studio: `.tran`, `.ac`,
`.dc` cards as troff lines (`.tran 1u 10m` in column 1), sliders on element values,
probes on nets, `.measure` results. The deck is extracted from the drawing: a wire
touching a lead is a connection, so draw honest topology.

```pic skip
spice = 1
.tran 10u 5m
In: (0,0); Out: (1.5,0); Gnd: (0,-1.2)
V1: vsource(In, Gnd, SIN(0 1 1k))
R1: resistor(In, Out, 1k)
C1: capacitor(Out, (1.5,-1.2), 1u)
wire((1.5,-1.2), Gnd); ground(Gnd)
```
