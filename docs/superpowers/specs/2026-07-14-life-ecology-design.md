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
- **Slice 2 — predator (risk gate).** RED + GREY (kept, not collapsed — avoids
  churn) + MARTEN. Conserved predation + energy, `evade = 0` so the marten eats
  both prey **equally** (no asymmetry yet). *Payoff:* predator–prey waves that
  persist and oscillate without collapsing. **This slice is where the project
  lives or dies** — a headless persistence/oscillation check is the feasibility
  gate; if the cell-based conserved model can't sustain oscillation even after
  tuning, that is the real finding (fall back to WaTor movement or a field model).

  **Slice-1 evaluation outcome (2026-07-14):** at β 0.10/0.18 grey drives red to
  *full* extinction by ~gen 500. To keep reds alive for the marten rescue the
  user chose the **gentle β gap** (cheapest, no new structure): narrow the gap so
  exclusion is slow and reds linger, letting the predator do the tipping. Terrain
  refuge and forced early reintroduction are **not** used — deferred/​dropped.
- **Slice 3 — asymmetric evasion (keystone).** Split prey back to RED/GREY, add
  `evade_red ≫ evade_grey`, add a **"Reintroduce martens"** button. *Payoff:*
  toggle martens on → greys crash → reds recover. The whole point, in one click.

  **Slice-3 outcome (2026-07-16): CASCADE CONFIRMED.** At `evade_red` 0.70 /
  `evade_grey` 0.05, martens introduced at gen 300 into a near-extinct red
  population (70 left): red → 1307, grey 1548 → 180 (87.9% red share). Asserted
  headlessly both directions, and reproduced in the shell at 80×40 unseeded.

  **The symmetric control is the real result.** With `evade_red = evade_grey`
  — same predator, same pressure, asymmetry removed — martens still crash the
  greys (1548 → 508) but **reds go extinct anyway**. A generalist predator does
  not save the native; it harvests both. Predation is not the cause, the
  asymmetry is. Exposed in the UI as *Make evasion symmetric* (one click).

  **Emergent, not coded:** marten density becomes grey-driven. Hunting an
  evading red usually wastes the turn, so martens can only sustain on greys →
  marten count tracks grey count → greys rebound as martens fade → a
  predator-prey limit cycle falls out of the evade rule alone.
- **Slice 4 — candidates, not yet chosen.** Alternative marten food (gap 1
  below) · squirrelpox (gap 2) · chart polish · GPU port (the two-phase rule
  needs an intent texture).

Each slice: build → self-test (Node core-extract asserts) → visual verify
(playwright) → evaluate with the user before the next.

## How well does this correspond to the real world?

Recorded after slice 3, since it is the question the whole exercise is meant to
answer. Honest answer: **the mechanism is right, the ecology around it is thin.**

**What matches.** Marten recovery suppressing greys and releasing reds is the
real observed effect (Sheehy & Lawton in Ireland; Vincent Wildlife Trust in
Wales/Scotland). Grey ground-naivety vs red co-evolution is the leading
mechanistic hypothesis, and it is exactly what this model encodes. The
symmetric control — predator alone is *not* enough — is a genuine prediction of
the model rather than something built into it, and it is the reason the marten
story is interesting in the first place.

**What does not.**

1. ~~**No alternative marten food**~~ — **CLOSED in slice 4**, see below. It was
   the biggest gap and it was not a 2-line fix: a flat food term is
   algebraically just a lower `δ`, so the food had to become a *shared* resource
   to add any dynamics at all.
2. **No squirrelpox** — arguably the most significant ecological omission.
   Greys carry a virus lethal to reds and largely harmless to themselves; it is
   a major driver of the real red decline. Here the grey advantage is birth rate
   *alone*, which understates how the invasion actually works.
3. **Martens are stationary**, spreading only by breeding. Real martens roam
   large territories — a deliberate trade to avoid synchronous-CA movement
   arbitration (see Mechanic 2).
4. **No habitat structure.** Reds do better in conifer, greys in broadleaf;
   terrain was dropped in slice 1.
5. **Timescale and patchiness.** Reds here recover almost everywhere within
   ~200 generations. The real recovery is patchy, contested, and takes decades.

So: good enough to demonstrate *why* the marten works, and specifically why a
non-selective predator would not. Not a forecasting tool.

## Mechanic 4 — alternative marten food (slice 4)

Real martens are generalists — voles, birds, eggs, berries, carrion — so their
numbers do not track squirrels. Gap 1 above.

**The flat term is a trap.** `e' = e − δ + food + …` is just
`e' = e − (δ − food) + …`. A constant food source is *identical* to lowering
metabolism: same model, renamed slider, no new behaviour. To do anything, the
food must be a **shared resource** — which is also what it physically is, since
alternative prey supports a bounded marten density (territoriality):

```
MARTEN cell:
    e' = energy − δ + (ate ? g : 0) + forage / (1 + nMartenNeighbours)
```

Dividing by the local marten count makes the gain saturate with crowding, so
martens acquire a **baseline density set by the countryside rather than by
squirrels**. `forage = 0` reproduces slice 3 exactly (asserted), so the slice-3
tests remain a valid no-subsidy control. With no prey at all, martens pack to
roughly the Moore-8 maximum independent set (~25% of cells) and stop.

### Two results this produced

**Hyperpredation.** A subsidised predator does not decline as its prey declines,
so the prey gets no numerical refuge. Greys go from cycling (267 ± 140) to
extirpated (0 ± 0) — which is what actually happened in Ireland, so the
subsidised model is the *more* faithful one. But past `forage ≈ 2` the martens
take the reds down too; at `forage = 3` everything but the marten is gone. Same
shape as cats subsidised by introduced rabbits exterminating island birds.

**Better evasion can hurt the reds.** At `forage = 0`, raising `evade_red` from
0.70 → 0.90 takes reds from 1005 to 349 and hands the greys the field (367 →
1235). With no other food the marten lives *only* on squirrels, so reds that
evade too well starve the predator that is protecting them. Unsubsidised, the
reds must feed their own bodyguard.

**Neither gap closed alone.** `forage=0, evade=0.90` → greys win.
`forage=1.0, evade=0.70` → greys gone, reds mediocre (623). Only both together
give the observed outcome — greys extirpated, reds abundant, martens sustained
at modest density — so the defaults are **`forage = 1.0`, `evade_red = 0.90`**
(red 70 → 1419, grey → 0, marten 106 on a seeded 60×30; red 160 → 2519,
grey 2704 → 0, marten 194 in the shell at 80×40 unseeded). The two "gaps" were
one interacting system, not two independent errors.

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
