# life-stochastic.html — stability metric + config search

## Problem

Stochastic Life on a torus (`life-stochastic.html`) has a large parameter
space (curve σ/ceiling/floor for create+survive, rule, topology, density).
Most random configs collapse to near-empty or near-full grids quickly.
Configs that settle into a long-lived, roughly-constant population ratio are
the "interesting" ones (analogous to edge-of-chaos / Langton's λ behavior in
cellular automata). There's currently no way to tell interesting configs
apart from dead/full/chaotic ones without watching each run manually.

## Goals

- Live: while a sim plays, show a stability score/badge for the current config.
- Batch: run many candidate configs headless and fast, rank by stability,
  let the user jump straight to a promising one.

## Non-goals

- Searching rule/topology/density space (curve-shape params only, per user
  choice — rule/topology/density stay at whatever the UI currently has set).
- Persisting search results across page reloads.

## 1. Stability metric

Per generation, `ratio = livePop / (COLS*ROWS)`.

Maintain a fixed-size circular buffer of the last `WINDOW` (default 50)
ratio values. Once the buffer is full, compute `mean` and `stdDev` over it.

Classification (computed every generation once buffer is full):

| Condition | Status |
|---|---|
| `mean < DEAD_T` (0.02) | dead |
| `mean > FULL_T` (0.95) | full |
| else `stdDev < STABLE_T` (0.03) | stable colony |
| else | chaotic |

Score used for ranking = `stdDev` (lower is better), only considered when
status is not dead/full.

This needs one small helper — a fixed-size ring buffer with running
mean/variance (Welford or simple sum/sum-of-squares over the ring) — shared
by both the live monitor and each headless search trial.

## 2. Live monitor

Extend `#status` panel:
- stdDev value (or "—" until buffer fills)
- colored status badge: stable / chaotic / dead / full

Rolling buffer resets on Clear, Random, and Apply-size (anywhere
`freshStart()`/`rebuild()` already runs). Updated once per generation inside
the existing `step()`/`updateStatus()` path — O(1) per tick, no extra
render cost.

## 3. Batch search

New "Search" button in the UI panel.

For each of 30 trials:
- Headless simulation: plain array step loop (reuses `neighbors`/`resolve`
  logic, no `paint()`/THREE calls), grid fixed at 60×30, run for 300
  generations.
- Randomize this trial's 6 curve params (sigma/ceil/floor × create/survive)
  within the existing slider ranges (sigma 0.05–2.00, ceil/floor 0–1).
  Rule, topology, and density are read from the current live UI state and
  held fixed for every trial.
- Track the ratio ring buffer through the run; score = final-window stdDev
  (same rule as live monitor), trial discarded from ranking if status
  is dead or full at the end.

Trials run synchronously in a tight loop (60×30×300 gens × 30 trials is
small enough to stay well under a second; no need for async chunking).

## 4. Results UI

Below the Search button: a results list, one row per trial, sorted best
(lowest stdDev) first, showing score + the 6 params + final ratio.
Rows for dead/full trials are omitted from ranking but can still be listed
at the bottom (or dropped entirely — dropped is simpler, going with that).

Clicking a row:
- writes the 6 values into `bParm`/`sParm`
- updates the corresponding sliders + their readout spans
- calls `buildTables()`
- closes/clears the results list (back to normal interactive view — the
  main canvas was never touched by the headless trials, so no view change
  needed beyond hiding the results panel)

## Constants (tunable later, not exposed as UI sliders for v1)

```
WINDOW      = 50     // rolling window, live + search
DEAD_T      = 0.02
FULL_T      = 0.95
STABLE_T    = 0.03
SEARCH_TRIALS = 30
SEARCH_COLS = 60, SEARCH_ROWS = 30
SEARCH_GENS = 300
```

## Testing

Extend the existing `selfTest()` (or a sibling function) with a couple of
`console.assert`s: ring-buffer mean/stdDev correctness on a known sequence,
and dead/full/stable classification boundaries.
