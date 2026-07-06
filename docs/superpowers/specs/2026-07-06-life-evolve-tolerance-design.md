# Life-Evolve — Slice 1: Foe-Tolerance Evolution

**Date:** 2026-07-06
**Status:** approved, building
**File:** `life-evolve.html` (new, self-contained; sibling of `life-torus.html` / `life-stochastic.html`)

## Goal

A first, complete Darwinian loop on the topological Life surface: a heritable,
mutable per-cell trait under spatial selection, visualised so adaptation is
watchable. Builds on `life-torus.html`'s two-clan signed-sum machinery.

Explicitly **out of scope** for this slice (future increments): the environment
field (Slice 2, orthogonal terrain-tolerance axis), weighted 5×5 kernel, 4/6
types. Slice 1 is 2 clans + Moore-8 for the cleanest evolution signal.

## Mechanic

Each live cell carries a **tolerance** gene `τ ∈ [0,1]`, stored in a
`Float32Array tau` parallel to `grid` (τ meaningless where `grid[i] === 0`).

Neighbour tally is per-clan as in life-torus (warm = clan 0, cool = clan 1).
The signed support for a cell of clan k is:

```
s = alliesWeight − (1 − τ) · foesWeight
```

where allies/foes are the summed neighbour counts of the same / other clan.
- `τ = 0` → `s = allies − foes` (exactly today's life-torus behaviour).
- `τ = 1` → `s = allies` (foes ignored; cell tolerates full contact).

Birth / survival test is unchanged: cell lives if `s` is in the B/S set (Moore-8
digit-set rule, e.g. B3/S23). Newborn clan = majority clan of the winning
neighbours; tied majority → no birth (life-torus rule, keeps determinism of
type assignment; τ makes the sim stochastic-free but mutation is the only
randomness).

**Birth uses a representative τ for the empty cell's test?** No — an empty cell
has no τ. For **birth**, foes always count full (`(1−τ)` with τ of the *newborn*
is unknown). Rule: birth test uses `s = allies − foes` (τ only shields existing
cells). This keeps birth a property of the neighbourhood, and makes τ purely a
survival advantage — selection acts through differential survival at borders,
which is sufficient and clean. (Documented so it is not read as an oversight.)

## Inheritance + mutation

On **birth**, newborn `τ' = clamp(meanTauOfWinningClanParents + N(0, σ_mut), 0, 1)`
where the mean is weighted by each parent's neighbour contribution (here all
weight 1 under Moore-8). `σ_mut` is a UI slider (0 = frozen genome = drift-free
control case). `N(0,σ)` via Box–Muller on `Math.random`.

Selection is emergent: at a clan border low-τ cells die (foes bite), high-τ
survive and breed → border colonies drift toward high τ. Interior cells (few
foes) feel no pressure → neutral drift.

## Visual design

**Cell colour = clan ramp positioned by τ** (RGB lerp dim→bright):
- Warm clan: `#5a1a10` (τ0) → `#ffd060` (τ1)
- Cool clan: `#10203a` (τ0) → `#60e0ff` (τ1)
- Dead: `#0a0c10`-ish (`[26,18,8]`, matches life-torus).

An evolving border brightens as high-τ wins — evolution visible in the field
itself.

**Colour-by toggle** (button, like 2D/3D): *Clan* (above) vs *Tolerance* —
clan-agnostic viridis-ish heatmap (indigo→green→yellow) over τ, reading the raw
genome landscape.

**Evolution chart** (canvas, reusing the life-stochastic chart pattern): mean τ
per clan over the last ~200 gens (warm line, cool line), y ∈ [0,1]. The lines
climbing from the 0.5 seed is the proof of selection.

**Status:** `Gen · pop (warm/cool) · mean τ warm/cool`.

**Controls:** Mutation σ slider (key knob); Seed τ (all cells start here, or a
"random τ" checkbox); inherited from life-torus: rule, topology, Orbit/Pen/
Glider, speed, density, grid size, 2D/3D, Reset, Help. Pen places cells at the
seed-τ value.

## Reuse / structure

Copy topology (`resolve`), rendering (torus + plane + DataTexture), pointer
tools, resize/rebuild, timing loop, and 2D/3D from `life-torus.html` verbatim.
New: the `tau` array, the modified `computeNext` (signed sum with τ shield +
inheritance/mutation), dual paint modes, the evolution chart, and the two new
sliders. Sim stays self-contained (not via `life-stats.js`, whose `stepGrid` is
single-type Moore-8 with no genome).

## Verification

Node self-test (extract sim core, assert):
1. `τ = 0` everywhere reproduces life-torus signed-sum kill (rival neighbour
   kills blinker centre).
2. A high-τ cell survives a foe-contact configuration that kills the same cell
   at low τ.
3. Mutation output stays within [0,1] over many draws.
4. Newborn τ ≈ mean of winning-clan parents (± mutation) — at σ=0, exactly the
   mean.

Headless evolution assertion: seed a two-clan interface, run 200 gens; **mean
border-τ rises at σ>0 and stays ~flat at σ=0**. This machine-checks that
selection (not a bug) drives the trait. Then playwright screenshots of both
colour modes.

## Later slices (not now)

- **Slice 2 — environment field:** static per-cell terrain scalar biasing s
  locally; second orthogonal selection axis (terrain tolerance gene).
- **Slice 3:** weighted 5×5 kernel + 4/6 types with per-clan ramps.
