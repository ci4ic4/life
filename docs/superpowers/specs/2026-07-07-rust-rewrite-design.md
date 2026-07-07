# Rust rewrite — native Life simulations

**Date:** 2026-07-07
**Status:** Design (approved for planning)
**Scope:** Port the three browser Life sims (`life-torus`, `life-stochastic`, `life-evolve`) to a native Rust desktop application.

## Goal

A standalone native application running the existing Life-on-a-torus simulations, targeting **Windows, macOS, and Linux** first, with **FreeBSD/NetBSD** as a later, explicitly-deferred target.

The rewrite is motivated by expansion capability outside the browser, not by a current performance ceiling — the existing 2048² grid is adequate for present investigations. Raw performance, a publishable standalone crate, and the learning value of the conversion are all **secondary targets, parked** for a later review once this stage completes. The architecture must not foreclose them; in particular the sim core is designed so it *could* be published as a reusable crate later without rework.

## Non-goals (this stage)

- Pushing grid size beyond the current 2048² cap. Zoom, not bigger fields, if resolution ever matters.
- Publishing `life-core` to crates.io. It is *designed* to be publishable, but publishing is out of scope now.
- BSD support as a shipped, tested target. Architected-for, not delivered.
- Web/WASM target. Out of scope (the browser version already exists and is satisfactory).
- Any async runtime (tokio/async-std). Not needed — headless analysis runs on a plain `std::thread`.
- Preserving the browser's "self-contained single `.html` file" property. Native means a Cargo build; that property is deliberately traded away.

## Stack

- **winit** — cross-platform windowing + input (Windows/macOS/Linux, and BSD later).
- **wgpu** — GPU compute (WGSL) + torus rendering. Same abstraction level as the current WebGL2 harness.
- **egui** — immediate-mode control panels (replaces the DOM control panels).

Chosen over Bevy: Bevy's ECS is a mismatch for cellular automata that live in flat arrays, and its large dependency tree worsens the later BSD port. wgpu+egui+winit is the lightest stack, closest to the existing hand-rolled render loop, and gives the most direct control over the GPU compute pass the sims depend on. Bevy uses wgpu underneath anyway, so this loses no GPU capability.

## Architecture — workspace of focused crates

```
life/                        (cargo workspace root)
├─ crates/
│  ├─ life-core/       Pure simulation math. No GPU, no window, no wgpu.
│  ├─ life-gpu/        wgpu harness: ping-pong compute + colour pass + readback.
│  ├─ life-render/     Torus mesh, UV paint, orbit camera, 2D/3D toggle.
│  └─ life-app/        The binary: winit loop, egui panels, pointer tools, presets.
└─ Cargo.toml          (workspace manifest)
```

The crate split *is* the reuse story. The three browser HTML files currently duplicate `resolve()` and rule-parsing code because `file://` has no import mechanism. Rust lets `life-core` hold the single canonical copy — a dedup impossible in the browser version.

### `life-core` — the reusable heart

Pure functions, no I/O, no GPU. Replaces `life-stats.js`. Fully unit-testable with `cargo test`. This is the only crate that could later be published for others to reuse.

Responsibilities:
- **Topology / boundary resolution** — `resolve(x, y, w, h, topology)` for torus (`Straight`), hard-edge (`None`), Klein seam (`Flip`). The canonical implementation; render/gpu crates never re-implement it.
- **Rule parsing** — `parse_bs("B3/S23")` → digit-sets stored as `u16` bitmasks (membership = one bitshift, no `HashSet`).
- **Rule curves** — `curve(bs, sigma, floor, ceil)` → Gaussian bump per neighbor count (0..=8), for the stochastic/evolve probability tables.
- **Step functions**, one per rule family, each returning a fresh double-buffered `Grid`:
  - `step_deterministic(grid, bs, topology)` — life-torus.
  - `step_stochastic(grid, table, rng, topology)` — life-stochastic.
  - `step_evolve(grid, genome, env, params, rng, topology)` — life-evolve; signed-sum `s = allies − (1−τ)·foes`, terrain response via θ, gene inheritance as contribution-weighted parental mean + Gaussian mutation.
- **Genes / environment** — `Genome { tau, theta }` as `Vec<f32>` lanes parallel to the grid (meaningless where dead); static `Env { field }` ∈ [−1, 1].
- **Headless analysis** — `run_trial(...)` and `classify(...) -> Verdict { Dead, Full, Stable, Chaotic }`, plus `RingStats`. Replaces `runHeadlessTrial`/`classify`.
- **Presets** — `hatch_creatures()` returns the existing LtL bug presets as plain data.

Key design decisions:
- **RNG is injected** (`&mut impl Rng`), never global. Runs become deterministic given a seed → reproducible experiments and seedable tests. Strictly better than the browser's `Math.random()`, for free.
- **One `Rule` enum**, not a trait: `Rule { Deterministic(BS), Stochastic { bs, sigma }, Evolve(EvolveParams) }`. Three variants, one `match`. No premature trait abstraction over three known implementations.
- **Step returns a new `Grid`** (double-buffer), mirroring the GPU ping-pong so CPU and GPU share signatures for the parity self-test.

### `life-gpu` — the wgpu harness

Replaces `life-gpu.js`. Owns the fragile GL/wgpu plumbing once so there is a single copy to get right (same rationale as the current shared JS module). Provides ping-pong compute textures, a colour pass into a render target, and on-demand readback. Each sim supplies its own WGSL rule shader + colour shader + CPU↔texture packing; `life-gpu` owns device init, surface config, resize, and the buffer lifecycle.

Depends on `life-core` for the CPU reference used by a boot-time `gpu_self_test()` asserting the GPU step matches the CPU step exactly — direct match for torus/evolve, and at the σ→0 deterministic limit for stochastic (identical to the current browser self-tests).

**Note — real compute, not a fragment-shader trick:** WebGL2 has no compute shaders, so `life-gpu.js` fakes the CA step by rendering a fragment shader into a target. wgpu has real compute shaders (storage textures, workgroups). The step pass is therefore *redesigned*, not transliterated — a cleaner model, but new code.

**Readback caveat:** wgpu readback is asynchronous (`map_async` + `device.poll`). `life-torus` reads back every generation for its cycle/still-life detection, so threading an async buffer map through a synchronous per-generation loop is the single fiddliest part of the project. The other two sims read back throttled/on-demand, as today.

### `life-render`

Torus mesh with UV mapping (the state texture painted onto the torus each frame), a **hand-rolled orbit camera** (spherical coordinates: yaw/pitch/radius from pointer drag + scroll; no camera-crate dependency), and the 2D/3D view toggle. Consumes a texture from `life-gpu`; contains no simulation logic.

### `life-app`

The binary. winit event loop, egui control panels, pointer tools (orbit camera / pen paint / drop glider), and preset loading. Wires the other three crates together. **The three sims ship as modes within one binary** — a `Mode { Torus, Stochastic, Evolve }` selector in the egui panel switches the active `Rule`, shader set, and controls at runtime. One window, one build, no per-sim binaries.

## Data flow (per frame)

```
egui panel edits ──▶ app state (rule, params, seed, tools)
                          │
                          ▼
                    life-gpu: compute pass (WGSL step) ──▶ ping-pong texture
                          │                                     │
              (torus: every gen; others: throttled)             ▼
                          ▼                              life-render: paint
                    readback ──▶ life-core analysis          torus + camera
                    (cycle detect / classify)                     │
                          │                                       ▼
                          └────────── egui readout ◀──────── winit present
```

## Testing

- **`cargo test`** replaces `node --test`. The existing `life-stats.test.js` assertions port directly into `life-core` `#[test]` functions and become the acceptance bar for each ported function.
- **GPU parity self-test** at boot (`gpu_self_test()`), preserving the current guarantee: GPU output == CPU reference (direct for torus/evolve; σ→0 limit for stochastic).
- **Seeded determinism**: because RNG is injected, stochastic/evolve steps are reproducible from a seed, enabling exact-match tests the browser version could not have.
- No new test framework. Rust's built-in test harness only.

## Slice plan

Each slice ships a runnable app with `cargo test` green. Later slices only *add* to `life-core`; they never edit functions an earlier slice's tests froze — matching the existing spec-driven slice discipline.

### Slice A — foundation + life-torus (proves the whole pipeline)
1. `cargo` workspace, 4 crates, CI skeleton.
2. `life-core`: `resolve`, `parse_bs`, `step_deterministic`, `Topology`. Port `life-stats.test.js` → `#[test]`.
3. `life-gpu`: winit window + wgpu init; WGSL compute for `step_deterministic`; ping-pong textures; `gpu_self_test()` on boot.
4. `life-render`: torus mesh, UV paint, orbit camera, 2D/3D toggle.
5. `life-app`: egui panel (rule string, run/pause/step, grid size, boundary mode); pen + glider tools; cycle/still-life readout.

Ships: native life-torus on Windows/macOS/Linux.

### Slice B — life-stochastic
1. `life-core`: `curve`, `step_stochastic`, `run_trial`, `classify`, `RingStats`. Port stochastic tests.
2. `life-gpu`: stochastic WGSL variant with per-cell hash PRNG (PCG/xxhash seeded per cell+gen); self-test at σ→0 == deterministic.
3. `life-app`: σ slider; stability-search panel running headless `run_trial` on a `std::thread` (no async runtime).

Ships: life-stochastic.

### Slice C — life-evolve
1. `life-core`: `Genome`, `Env`, `step_evolve`, gene inheritance + mutation, env-field generator.
2. `life-gpu`: evolve WGSL with extra texture lanes for τ/θ + env; self-test parity.
3. `life-render`: gene/clan colour mapping (warm/cool, terrain tint).
4. `life-app`: τ/θ/σ_mut sliders, clan tools, env-field controls, presets.

Ships: life-evolve. Full feature parity with the browser suite, native.

## Complexity / risk

| Component | Port kind | Effort | Risk | Watch-out |
|---|---|---|---|---|
| `life-core` math | Transliterate JS→Rust | Low | Low | Tests port 1:1 and set the bar. |
| egui panels | Re-express DOM controls | Low | Low | Immediate-mode: state lives in app struct, not widgets. |
| WGSL compute | Rewrite (not transliterate) | Med | Med | Real compute shaders replace the WebGL2 fragment-shader trick. |
| GPU readback | New model | Med | **High** | Async `map_async`+`poll` through life-torus's per-gen sync loop is the fiddliest single piece. |
| winit + wgpu init | Boilerplate | Med-High | Med | The Rust-graphics tax. Recent winit `ApplicationHandler` API; verbose surface/resize/lifecycle. Pin versions. |
| Torus mesh + orbit camera | Re-implement | Med | Med | No drop-in OrbitControls; hand-roll orbit (~40 lines). |
| Per-cell RNG in WGSL | New | Med | Med | Hash PRNG in-shader; parity only at σ→0. Pattern already used in browser. |
| Evolve gene lanes on GPU | New | High | Med | τ/θ+env texture lanes, weighted-mean inheritance in-shader. Deliberately last (Slice C). |
| BSD (FreeBSD/NetBSD) | Later | ? | **High** | wgpu Vulkan on FreeBSD ~ok; NetBSD sketchy; winit X11/Wayland under-tested on BSD. Deferred. |

Net: no single hard wall; one genuinely fiddly spot (async readback in the torus per-gen loop); a one-time boilerplate tax up front (winit/wgpu init). The simulation math — the original, valuable part — is the low-risk part. Friction is platform plumbing, isolated inside `life-gpu` so it is written once.

## Deliberate simplifications (YAGNI)

- **4 crates, no more.** No `life-config` crate (presets are data in `life-core`); no `life-traits` crate (one `Rule` enum beats a trait with three impls). Split only at real consumer boundaries.
- **No async runtime.** Headless analysis on a `std::thread`; GPU readback via `device.poll`.
- **No plugin/mod system, no scripting, no serialization format** beyond what presets need. Add when a second consumer actually exists.
- **BSD, publishing, and >2048² scale are parked**, not designed-in prematurely — but the crate boundaries keep all three cheap to pick up later.
