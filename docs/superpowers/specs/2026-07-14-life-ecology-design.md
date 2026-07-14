# Life-Ecology — Squirrels & Pine Martens (Trophic Cascade)

**Date:** 2026-07-14
**Status:** approved (design), building slice-by-slice
**File:** `life-ecology.html` (new, self-contained; fork of `life-evolve.html`)

## Goal

A watchable spatial-ecology simulation of the real England/Wales phenomenon:
a **small viable pine-marten population suppresses invasive grey squirrels and
lets native reds recover**. The payoff is a toggle — reintroduce martens and
watch greys crash while reds rebound — driven by one asymmetry: **reds evade
the marten, greys (North-American, ground-naive) do not.**

This is *inspired by* the Life suite and reuses its rendering skin, but its
**rule is not a B/S Life variant** — it is a spatial-ecology CA:
**contact-process prey growth + conserved WaTor-style predation.** The Gaussian
σ / B/S-rulestring machinery of the other three files does **not** come along.

Real-world basis (Sheehy & Lawton et al.; Vincent Wildlife Trust, mid-Wales):
grey squirrels spend more time on the ground, are larger, and don't recognise
the marten as a threat → the marten's easy, preferred meal; reds co-evolved,
stay in the canopy, flee early → low capture rate. The marten works at **low
density** — a sparse travelling predator tips the balance. Habitat (conifer
favours reds, broadleaf greys) is real but second-order — deferred.

## Species / state

```
grid:   Uint8Array   0 = EMPTY, 1 = RED, 2 = GREY, 3 = MARTEN
energy: Float32Array meaningful only where grid == MARTEN   // parallel array, like evolve's τ/θ
```

RED and GREY are both prey (contact-process); they differ only in two
per-species scalars: birth rate `β` and evasion `evade`. MARTEN is the predator
(conserved consumption + energy/hunger). Slice 2 collapses RED+GREY into a
single prey type to isolate the predator dynamics; slice 3 splits them.

## Mechanic 1 — prey (contact process)

Population growth is space-limited (habitat = carrying capacity), **not**
pattern-forming. No B/S digit-set, no overcrowding/underpopulation death.

```
EMPTY cell:
    for each prey species sp in its Moore-8, colonisation pressure ∝ β[sp]·nNeighbours[sp]
    pRed  = 1 − (1 − β_red )^nRed
    pGrey = 1 − (1 − β_grey)^nGrey
    single roll partitions [0,pRed) → RED, [pRed,pRed+pGrey) → GREY, else stays EMPTY
    (contested empties resolved by one draw; order is a fixed convention, not a knob)

PREY cell (RED or GREY):
    predation handled in Mechanic 2 (may be eaten)
    if not eaten: survives with prob σ (natural survival), else → EMPTY
```

`β_grey > β_red` is the invasive advantage — **one knob**, the user's own
framing ("greys had higher rates of birth"). Density self-limits because
colonisation needs an empty neighbour; predation then carves the saturated
field into waves. No explicit carrying-capacity death.
`// ponytail: habitat is the cap; add crowding death only if fronts look too solid.`

## Mechanic 2 — predation (conserved, WaTor-style)

Martens are **stationary** (spread by breeding, not movement — avoids
synchronous-CA movement arbitration). What is conserved is **consumption**:
each prey is eaten by exactly one marten, each marten takes at most one meal per
tick. Arbitration is a deterministic hash so it is reproducible and
GPU-portable.

```
hash(a, b, gen)  — reuse the life-gpu PRNG hash; a reproducible priority per (cells, tick).

PHASE 1 — claims (per MARTEN cell with energy < E_cap):
    prey = the RED/GREY cells in my Moore-8
    claim = NONE if prey empty, else the prey maximising hash(preyCoords ⊕ myCoords, gen)   // bite ONE

PHASE 2 — resolve (per cell):
    PREY cell:
        claimants = marten neighbours whose Phase-1 claim == me
        if claimants nonempty:
            winner = claimant maximising hash(myCoords ⊕ martenCoords, gen)
            escaped = (rand < evade[mySpecies])          // slice 3; evade = 0 in slice 2
            if not escaped:  → EMPTY (eaten); only `winner` gains energy this tick
            (a missed hunt still cost the winner its turn — it gains nothing)
        else: natural survival (σ)
    MARTEN cell:
        ateThisTick = some adjacent prey had me as winner AND did not escape   // recompute, same hashes
        e' = energy − δ + (ateThisTick ? g : 0)
        e' ≤ 0     → EMPTY (starved)
        else e' = min(e', E_cap); stay MARTEN
    EMPTY cell (breeding spill):
        martens eligible = E_breed-eligible marten neighbours
        pBreed = 1 − (1 − μ)^nEligible; competes in the same single roll as prey colonisation
        if won by marten: → MARTEN, energy = E0; the parent pays breedCost (charged marten-side)
```

**Conservation guarantee:** Phase 1 caps each marten at one bite; Phase 2 caps
each prey at one death and routes energy to one winner. Two martens on one prey →
prey dies once, only the hash-winner eats; the loser genuinely **missed** and
drains toward starvation. That missed-hunt term is the negative feedback that
keeps predators in check (the non-conserved field model lacked it).

**Cost accepted — radius-2 locality:** a prey verifying "did my winner-marten
pick me?" recomputes that marten's Phase-1 claim, which scans *its* neighbours →
reads out to distance 2. Trivial on CPU; on GPU it is a second (intent) texture
pass, which `life-gpu.js` ping-pong already accommodates. Deferred (CPU-first
through the slices).

## Mechanic 3 — evasion (the keystone, slice 3)

The whole cascade is one comparison in Phase 2: a claimed prey escapes with
per-species probability `evade[sp]`.

```
red:  evade ≈ 0.7   (flees the canopy, marten misses)
grey: evade ≈ 0.05  (naive, dawdles on the ground)
```

A missed red = a marten that spent its hunt and stays hungry → predation
pressure is redirected onto greys. Marten eats *both*; it simply catches greys
far more often. That is the real biology, in a single roll.

## Parameters (all biologically named; ~8 knobs)

| knob | meaning | start |
|---|---|---|
| β_red / β_grey | prey birth per same-species neighbour | 0.10 / 0.18 |
| σ | prey natural survival / tick | 0.92 |
| δ | marten energy drain / tick | 1.0 |
| g | marten energy per successful meal | 3.0 |
| E_breed / E0 | marten breed threshold / newborn energy | 10 / 5 |
| breedCost | energy the parent marten pays per offspring | 5 |
| E_cap | marten energy clamp | 15 |
| μ | marten breed-spill per eligible neighbour | 0.5 |
| evade_red / evade_grey | escape prob (slice 3; both 0 in slice 2) | 0.7 / 0.05 |

Sliders, like the existing files. Everything else held fixed.

## Slice plan (each independently evaluable — the user's step-by-step workflow)

- **Slice 0 — substrate spike.** Three cell states render & run with independent
  β/σ, no predation. Proves multi-state on the evolve skin. Near-free.
- **Slice 1 — competition.** RED + GREY, `β_grey > β_red`, contest empties.
  *Payoff:* greys drive reds to near-extinction, no predator. A real result
  (invasive competitive exclusion) on its own.
- **Slice 2 — predator (risk gate).** Single prey + MARTEN, conserved predation
  + energy. `evade = 0`. *Payoff:* predator–prey waves that persist and
  oscillate without collapsing. **This slice is where the project lives or dies.**
- **Slice 3 — asymmetric evasion (keystone).** Split prey back to RED/GREY, add
  `evade_red ≫ evade_grey`, add a **"Reintroduce martens"** button. *Payoff:*
  toggle martens on → greys crash → reds recover. The whole point, in one click.
- **Slice 4 — instrumentation.** 3-line population chart (red/grey/marten over
  time — the cascade money-shot) + adapt the headless search harness to
  auto-hunt parameter sets where reds actually recover.

Each slice: build → self-test (Node core-extract asserts) → visual verify
(playwright) → evaluate with the user before the next.

## Visual design

- **Cell colour:** RED `#e0533a` (native), GREY `#9aa4b0` (invasive), MARTEN a
  warm brown `#7a4a20` with brightness ramped by energy (well-fed = brighter);
  EMPTY `#0a0c10`-ish `[26,18,8]` (suite convention).
- **Population chart** (canvas, reuse the life-stochastic chart pattern): three
  lines — red / grey / marten counts over the last ~300 gens. The anti-phase
  predator–prey waves and the post-reintroduction flip are read straight off it.
- **Status:** `Gen · red / grey / marten pop · mean marten energy`.
- **Controls:** the ~8 sliders above; **Reintroduce martens** button (seeds a
  small marten patch onto live state); inherited from evolve: topology,
  Orbit/Pen tools (pen paints the currently-selected species), speed, density,
  grid size, 2D/3D, Reset, Help. (No glider tool — meaningless here.)

## Reuse / structure

Copy from `life-evolve.html` verbatim: topology `resolve`, torus + plane +
DataTexture rendering, pointer tools, resize/`rebuild`, timing loop, 2D/3D,
canvas-chart scaffold, and the `life-gpu.js` harness wiring pattern.

**New:** the two-phase conserved `computeNext` (claims → resolve, replacing the
signed-sum tally), the `energy` array (repurposing evolve's parallel-Float32Array
pattern), contact-process prey birth, per-species β/evade scalars, the 3-line
population chart, the reintroduce-martens action, and the species colour ramp.

**Dropped (not in this file):** B/S rulestring parser, Gaussian `curve`/σ,
clan signed-sum, τ/θ genes, terrain field. Sim stays self-contained (does not
use `life-stats.js`, whose `stepGrid` is single-type B/S).

GPU: CPU-first through slices 0–3. Port the two-phase rule to fragment shaders
(sim pass writes state+energy; the radius-2 claim needs an intent texture) at
slice 4, with the standard boot-time `gpuSelfTest()` asserting GPU == CPU
reference bit-for-bit on a fixed seed.

## Verification

Node self-test (extract sim core, assert), per slice:

1. **Contact process:** an EMPTY cell with `k` prey neighbours becomes that prey
   with probability `1−(1−β)^k` (statistical, fixed RNG); zero prey neighbours →
   stays empty. `β_grey > β_red` ⇒ grey wins contested empties more often.
2. **Conservation:** in any configuration, `#prey eaten == #martens that gained
   energy` this tick, and no marten gains twice — machine-checks the arbitration.
3. **Missed hunt:** two martens adjacent to one prey → prey removed once, exactly
   one marten's energy rises, the other's falls by `δ`.
4. **Starvation / breeding:** marten with no reachable prey for `⌈E/δ⌉` ticks
   dies; a marten at `energy ≥ E_breed` beside an empty cell produces a MARTEN
   there and pays `breedCost`.
5. **Evasion (slice 3):** at `evade_red = 1` a claimed red is never eaten; at
   `evade = 0` reduces to slice-2 behaviour.

Headless cascade assertion (slice 3+): seed reds+greys, run to grey dominance
(reds → near 0); inject a small marten patch; assert **grey population falls and
red population rises** over the following window at `evade_red ≫ evade_grey`,
and that removing the asymmetry (`evade_red = evade_grey`) abolishes the
recovery. This machine-checks that *evasion asymmetry*, not the mere presence of
a predator, drives the cascade. Then playwright screenshots of the 3-line chart
across the flip.

## Out of scope (later, if interesting)

- Habitat field (conifer/broadleaf) biasing β per species — reuse evolve's env
  terrain; the honest conifer-favours-reds modifier.
- Marten *movement* (full WaTor) instead of stationary+breed — only if
  stationary fronts look wrong.
- Explicit prey carrying-capacity death — only if habitat-limitation proves
  insufficient.
- Evolving evasion (make `evade` a heritable gene like τ) — closes the loop back
  to life-evolve, but the real story is a fixed species trait, so not now.
