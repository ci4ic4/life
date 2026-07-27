# Life-Ecology in the Rust app — graphics and GUI

**Date:** 2026-07-27
**Status:** approved (design), to be built slice-by-slice
**Crates:** `life-gpu` (new `EcologySim`), `life-render` (new `RenderSource` variant),
`life-app` (panel extraction, then a third sim mode)
**Depends on:** `life-core::ecology`, landed in `f53f37b`

## Goal

`life-core` gained the ecology rule but nothing renders it — the Rust app still
covers only the three cellular automata. This wires the squirrel/marten
simulation into the existing `life-app` window: torus render, run/pause/step,
seeding, the core parameters, and live population counts.

The scope is deliberately the **core loop only**. Evasion, shared forage, the
mate rule, background mortality, squirrelpox and terrain all exist in
`life-core::ecology` and stay at their `Default` values with no sliders. They
are the point of the simulation, but they are also the part that is already
proven by 36 tests — what is unproven is the CPU-sim-to-texture path, and that
is what this builds.

## Decision: one app, not a second binary

The ecology mode joins `life-app` rather than becoming its own binary.

A separate binary would duplicate the torus mesh, the orbit camera, cell
picking, the egui layer and the winit shell — roughly 400 lines of
`life-render` plus `EguiLayer`, all of which exist to be shared. The HTML files
duplicate deliberately, because each must stand alone as a single file over
`file://`. The Rust crates are the opposite arrangement: `life-core`,
`life-gpu` and `life-render` are libraries with one consumer each, and adding a
second consumer is what they were shaped for.

## Decision: the CPU sim owns a GPU texture

`life-render::RenderSource` is an enum over **texture views**, not over
simulation types:

```rust
pub enum RenderSource<'a> {
    Binary(&'a wgpu::TextureView),
    Evolve { state: &'a wgpu::TextureView, env: &'a wgpu::TextureView },
}
```

The renderer's contract is "give me something to paint", not "give me a compute
pipeline", so a CPU simulation satisfies it unchanged. `EcologySim` therefore
steps on the CPU and uploads the result, exposing `front_view()` exactly as
`Sim` and `EvolveSim` do. `app.rs` then treats all three identically.

It lives in `life-gpu` despite computing nothing on the GPU. The name is a poor
fit for this one type, and that was weighed against the alternatives: putting it
in `life-app` makes it unreusable by any future headless or wasm consumer, and
adding a buffer-upload helper to `life-render` splits ownership of the sim
texture across two crates depending on which simulation is selected. The crate's
real role — owning the GPU resources a simulation renders from — still holds.

**This does not open the door to a GPU ecology port.** `mulberry32` is consumed
as one sequential stream in scan order, which is what makes a seed replay and
what a parallel port cannot preserve. See the reproducibility invariant in
`CLAUDE.md`. `EcologySim` owns that single RNG closure and calls the CPU rule.

## Architecture

| Piece | Crate | Role |
|---|---|---|
| `ecology.rs` | `life-core` | pure step; f64 params, f32 storage; already landed |
| `EcologySim` | `life-gpu` | owns `EcologyState`, the RNG closure and one texture; `step()` runs the rule and uploads |
| `RenderSource::Ecology` | `life-render` | new variant + its own WGSL fragment shader |
| `panel.rs` | `life-app` | egui panel, extracted from `app.rs` |
| `SimKind::Ecology` | `life-app` | third variant beside `Binary` and `Evolve` |

## Data flow

```
App::advance_once
  └─ EcologySim::step(ctx)
       ├─ life_core::ecology::step_ecology(&state, topo, &params, &mut rng)   CPU
       ├─ pack state into Vec<f32>                                           CPU
       └─ queue.write_texture(...)                                           → GPU
Renderer::draw  ── samples that texture on the torus mesh
```

No parallelism at any stage of the step.

## Texture and colour

One `Rgba32Float`, which reuses the float-texture bind layout `Evolve` already
declares:

| channel | contents |
|---|---|
| r | species — 0 empty, 1 red, 2 grey, 3 marten |
| g | marten energy |
| b | infected flag |
| a | terrain (`env`) |

Only `r` and `g` are read in this scope; `b` and `a` are written as zero. They
are reserved now so squirrelpox and refuges arrive without a format change — the
cost of the unused channels is 16 bytes per cell either way, since the layout
must match the existing float bind group.

Colour modes reuse the `set_color_mode` uniform already at offset 64 of the
view-projection buffer: **0** species, **1** marten energy. The palette is
copied from `life-ecology.html` so the two read as the same simulation.

## Panel and parameters

`EcologyParams` enters `App` as a single struct field. The same grouping is
applied to the existing stochastic and evolve parameters during the extraction
slice — flat per-mode fields on `App` are why `fn render` reached 334 lines, and
adding 18 more without fixing that is what makes the file unreadable.

Controls in scope: β_red, β_grey, σ, δ, g, E_breed, E₀, breed cost, E_cap, μ,
the seed, and live counts of red / grey / marten.

## Grid size

The GPU sims cap at 2048². Ecology is CPU and performs several neighbour scans
per cell per generation, so it needs its own, much lower cap. The starting
figure is **512×256** (~131k cells), to be replaced by a measured one: time
`step_ecology` at several sizes and pick the largest that holds the target
generations-per-second the app already exposes.

## Testing

`life-core::ecology` is covered — 36 ported tests plus `ecology_parity.rs`
against the JS. The new surface is the pack-and-upload path and the panel.

- One test asserting `EcologySim`'s packed buffer matches its `EcologyState`
  for a known board.
- **No GPU self-test.** The three CA sims each assert their GPU step matches a
  CPU reference, because each has two implementations that could disagree.
  Ecology has one implementation and no GPU rule, so there is nothing to
  compare — adding a self-test here would assert that an upload round-trips,
  which is a wgpu test, not a simulation test.

## Slices

1. **Extract the panel** from `app.rs` into `panel.rs`; group per-mode
   parameters into structs. No behaviour change; the app must look and act
   identically before and after.
2. **`EcologySim` + `RenderSource::Ecology`** — a seeded board renders on the
   torus, static. Proves the upload path and the shader.
3. **Run loop and panel** — stepping at the existing generations-per-second
   control, the core sliders, live population counts.

## Out of scope

Deferred to later work, all of it already implemented in `life-core::ecology`
and in `life-ecology.html`:

- evasion asymmetry, shared forage, mate rule, background mortality
- squirrelpox and its reservoir
- terrain refuges and terrain painting
- the reintroduction buttons
- the population-history chart
- seeded save/load and the sources panel
- any GPU port of the rule — blocked by the RNG stream contract, not by effort
