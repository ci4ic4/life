# Life-Evolve — Slice 2: Environment Field (Sugarscape-style)

**Date:** 2026-07-06
**Status:** approved, building
**File:** `life-evolve.html` (extend)

## Goal

A static terrain layer that makes some places livable and others not — a
Sugarscape-style resource landscape. It biases birth/survival locally, so
colonies pool in fertile basins and shun hostile ridges, and terrain becomes
barriers and corridors. It shapes *where* life happens; the existing tolerance
gene τ still evolves *within* it. **No new gene** (a terrain-tolerance gene is a
possible later Slice 2b).

## Mechanic

`env`: a `Float32Array` parallel to `grid`/`tau`, values in **[−1, +1]**
(fertile + / hostile −), **static** (set on generate/paint, never mutated by the
sim). Resized in `rebuild()` alongside `tau`.

One term is added to the support, for both birth and survival, before the
range/set test:

```
survival:  s = allies − (1−τ)·foes  + envWeight·env[here]
birth:     b = (a0 − a1)            + envWeight·env[here]
```

- `env = 0` everywhere → identical to current behaviour.
- Fertile cell (env>0): support bonus → easier birth, harder to starve.
- Hostile cell (env<0): penalty → dead zones.

`envWeight` is a UI slider in support units (0 = terrain off). Default modest so
terrain shapes without steamrolling clan/τ dynamics (meaningful against Moore's
0–8 and weighted's 0–40). The bias is added to the *same* `surviveOK`/`birthOK`
tests already in `computeNext`.

## Creating the terrain

**Generate (procedural, terrain-like):** a *Generate terrain* button fills `env`
with the sum of ~6–10 random Gaussian bumps (mixed sign), torus-wrapped, then
normalises to roughly [−1, 1]. Yields smooth basins/ridges, not white noise.
Each press = new landscape. A *Flat* button zeroes `env`.

**Pen (hand-drawn):** a **Terrain** tool added to the Orbit/Pen/Glider row, with
a **Fertile / Hostile** toggle (same UI pattern as the Warm/Cool clan picker).
Left-drag brushes soft Gaussian bumps of the chosen sign into `env` (accumulate,
clamp to [−1,1]). Reuses the existing `cellAt()` screen→cell mapping; a
`terrainAt(cell)` writes a small radial bump instead of a single cell.

## Rendering

Terrain is always visible under empty ground; a dedicated mode shows it alone.

- **Dead-cell tint by env** (all colour modes): fertile → green, hostile →
  red-brown, neutral → near-black (diverging around 0). Live cells keep their
  clan/τ colour drawn on top, so colonies sit on the visible landscape.
- **Colour toggle gains a third mode:** Clan / Tolerance / **Environment**. In
  Environment mode *every* cell shows the diverging red↔green field (bacteria
  ignored) — the clean "where are the zones" picture.

`cellRGB(v, t, e)` gains the env argument; a `terrainRGB(e)` helper does the
diverging map. Red/green is acceptable (user confirmed).

## UI additions

An **Environment** control group: `envWeight` slider (terrain strength), a
*Generate terrain* button, a *Flat* button. A **Terrain** tool button in the
tool row + a **Fertile/Hostile** toggle row (shown when Terrain tool active).
Colour toggle cycles Clan → Tolerance → Environment.

## Testing

Node self-test (extend the existing one; save/restore `env`, `envWeight`):
1. `env = 0` everywhere reproduces current behaviour (existing asserts still
   pass unchanged).
2. Fertile bias rescues: a cell whose plain support is just below the survival
   band survives once `env·envWeight` lifts it into range.
3. Hostile bias kills: a cell comfortably in-band dies once a negative
   `env·envWeight` pushes it out.

Headless spatial-selection check (scratchpad): seed a uniform population on a
field that is fertile on one half and hostile on the other; after N gens the
live density is clearly higher on the fertile half. Then playwright screenshots:
an empty generated landscape (Environment mode), and life filling the fertile
hollows.

## Out of scope (later)

- Terrain-tolerance gene θ (second evolving axis) — Slice 2b.
- Dynamic terrain (regrowth/consumption like true Sugarscape sugar) — the field
  is static here.
- Colour-blind-safe palette toggle (one-liner if wanted later).
