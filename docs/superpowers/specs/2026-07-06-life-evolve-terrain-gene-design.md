# Life-Evolve — Slice 2b: Terrain-Tolerance Gene θ

**Date:** 2026-07-06
**Status:** approved, building
**File:** `life-evolve.html` (extend)

## Goal

A second, orthogonal evolving trait: **terrain tolerance θ ∈ [0,1]**, so cells
adapt to *place*, not just to rivals. With a generalist/specialist trade-off,
selection splits θ across the landscape — generalists hold the hostile badlands,
specialists pack the fertile basins. Two evolved strategies matching the terrain.

Complements the existing genes: τ (foe tolerance, Slice 1/3) and the static env
field (Slice 2). No change to those; θ is a new axis layered on.

## Mechanic — generalist/specialist trade-off

`theta`: a `Float32Array` parallel to `grid`/`tau`/`env`, in [0,1]. Seeds neutral
at **0.5**; sized in `rebuild()`.

θ scales the whole terrain effect **on survival only** (birth is θ-blind, as the
newborn has no gene yet — mirrors how τ shields survival while birth counts foes
full):

```
survival:  s = allies − (1−τ)·foes + envWeight·env[here]·(1−theta[here])
birth:     b = (a0 − a1)           + envWeight·env[here]        (θ-blind)
```

- θ=0 — **specialist**: full terrain sensitivity (big fertile bonus, full hostile penalty).
- θ=1 — **generalist**: terrain-blind on survival (survives hostile, forgoes the fertile bonus).

Selection: on hostile ground (env<0) high θ removes the penalty → survives where
specialists die. On fertile ground (env>0) low θ keeps the survival bonus →
out-persists generalists. So θ evolves spatially: high in the red badlands, low
in the green basins.

**Feasibility caveat (must verify):** under a band-pass survival rule the fertile
bonus can tip a cell into overcrowding, which could invert or mute the
fertile-selects-low-θ half. Verification includes a headless two-niche check;
if the split doesn't appear, note it honestly (the badlands half is the robust
part regardless).

**IMPLEMENTATION FINDING (supersedes the direction above):** the split is strong
and reproducible (Δθ ≈ 0.25 between halves) but runs **opposite** to the naive
story. Overcrowding — not starvation — is the binding constraint under weighted
band-pass rules: the fertile bonus pushes terrain-sensitive (low-θ) cells over
the birth ceiling, so they sort onto the **hostile** half (penalty relieves
crowding), while terrain-blind (high-θ) cells hold the **fertile** half. This is
the same band-pass effect that makes τ's adaptation local. Two additions to the
mechanic made it work: (1) birth's terrain penalty is shielded by the parent
clan's mean θ (so a lineage can colonise, not just survive, terrain); (2) the
verification asserts the split magnitude in the band-pass direction, and the τ
spatial-adaptation assert now checks magnitude (sign is seed-fragile).

## Inheritance + mutation

On birth, newborn `θ' = clamp(weighted-mean θ of winning-clan parents +
N(0,σ_mut), 0, 1)` — identical machinery to τ, **sharing the same Mutation σ
slider** (one knob mutates both genes). Kept alongside the existing τ
inheritance in the same birth branch.

## Visualisation

θ needs its own view. The Colour button cycle grows to four:
**Clan → Foe tolerance (τ) → Terrain tolerance (θ) → Environment.** (The τ mode
label changes from "Tolerance" to "Foe tolerance" for clarity now there are two.)

θ heatmap: a palette distinct from τ's viridis and the env red/green — a
magma-like ramp (dark → magenta → warm-yellow), specialist(0)=dark →
generalist(1)=bright. `thetaHeat(θ)` helper; `cellRGB` gains the θ argument.

Status line adds mean θ per clan next to mean τ (compact); the time-series chart
stays τ-only to avoid clutter.

## Testing

Node self-test (extend; save/restore `theta`):
1. env=0 → θ has no effect (existing behaviour unchanged).
2. θ shields hostile: a cell that dies on hostile ground at θ=0 survives at θ=1.
3. θ forgoes fertile: a cell kept alive only by the fertile bonus at θ=0 dies at
   θ=1 (bonus removed).
4. Inheritance: newborn θ = weighted mean of winning-clan parents (σ=0 exact).

Headless two-niche check (scratchpad): fertile-left / hostile-right split field,
uniform seed, mutation on, run N gens; **mean θ on the hostile half > mean θ on
the fertile half**. This is the feasibility gate for the trade-off design. Then
playwright screenshots of the θ heatmap showing the spatial split.

## Out of scope

- Dynamic terrain (consumption/regrowth) — deferred; mechanics still open.
- Separate mutation rate per gene — shared σ for now.
- Seed θ₀ slider / random-θ — seeds neutral 0.5 (evolution drives it).
