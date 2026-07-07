# Rust Rewrite — Slice C (life-evolve) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the Darwinian variant: two clans, heritable τ (foe tolerance) and θ (terrain tolerance) genes, static env field, weighted kernels, ranged rules — full feature parity with `life-evolve.html`'s simulation core, GPU-accelerated, with clan/τ/θ/env colour modes.

**Architecture:** `life-core` gains an `evolve` module (CPU reference, `f32` arithmetic to enable exact GPU parity) plus terrain generation. `life-gpu` gains `EvolveSim` — a separate sim struct (Rgba32Float ping-pong state texture: R=clan, G=τ, B=θ; R32Float env texture; per-step params with generation counter for the mutation PRNG). `life-render` gains an evolve render path (float textures, 4 colour modes with viridis/magma ramps). `life-app` gains `Mode::Evolve` with kernel/rule/envWeight/σ_mut controls, terrain buttons, colour cycling, and throttled gene-stats readback.

**Tech Stack:** unchanged. No new dependencies.

## Global Constraints

- All prior constraints hold. Slice A+B self-tests keep passing.
- **CPU evolve math is `f32`** (not f64) and iterates kernel cells in the same order as the WGSL — required for the exact-parity self-test at σ_mut=0 (browser asserts "evolve: direct" parity; we preserve that).
- **Survival rounding matches JS**: `Math.round(s)` = `floor(s + 0.5)` — implemented identically in Rust and WGSL (`f32::round`/WGSL `round` half-even semantics differ; do NOT use them).
- **Selection semantics (verbatim from life-evolve.html, the design-spec'd subtleties):**
  - live: `s = allies − (1−τ)·foes + envWeight·env·(1−θ)`; genes carried unchanged on survival.
  - dead: birth support per clan `a_k − a_other + eb_k` where `eb_k = eFull·(1 − mean parental θ_k)` (θ-shielded terrain, τ ignored — birth counts foes full); exactly one clan qualifying → born with contribution-weighted mean parental τ/θ + `gauss(σ_mut)`, clamped to [0,1]; both/neither → stays dead.
- **Kernels:** Moore (weights 1, max 8, digit-set rules) and weighted 5×5 (sum 40, ranged rules `B14-18/S12-24` style). Kernel = `&[(i8, i8, f32)]`.
- **Terrain:** 9 signed Gaussian bumps, torus-wrapped distance, normalised to [−1,1]; flat = zeros. Terrain pen deferred with the other pointer tools.
- **The 7-part JS selfTest ports 1:1** as Rust `#[test]`s (τ rivalry, τ shield, mutation clamp, τ inheritance, weighted+ranged shield, env rescue/kill, θ shield/forgo/inheritance).

---

### Task C1: life-core — evolve module + terrain

**Files:** Create `crates/life-core/src/evolve.rs`, `crates/life-core/src/terrain.rs`; modify `lib.rs`.

**Interfaces:**
- `pub struct Kernel { pub cells: Vec<(i8, i8, f32)>, pub max: f32 }` + `Kernel::moore()`, `Kernel::weighted5x5()`
- `pub enum EvolveRule { DigitSet { birth: u16, survive: u16 }, Ranged { b_lo: f32, b_hi: f32, s_lo: f32, s_hi: f32 } }` + `parse_evolve_rule(s, ranged: bool) -> Option<EvolveRule>`
- `pub struct EvolveParams { pub rule: EvolveRule, pub env_weight: f32, pub mut_sigma: f32 }`
- `pub struct EvolveState { pub w: u32, pub h: u32, pub grid: Vec<u8>, pub tau: Vec<f32>, pub theta: Vec<f32> }`
- `pub fn step_evolve(st: &EvolveState, env: &[f32], k: &Kernel, p: &EvolveParams, t: Topology, rng: &mut impl FnMut() -> f64) -> EvolveState`
- `pub fn gauss(sigma: f32, rng: &mut impl FnMut() -> f64) -> f32` (Box–Muller, JS-identical form)
- `pub fn generate_terrain(w: u32, h: u32, rng: &mut impl FnMut() -> f64) -> Vec<f32>`

**Steps:** port the 7 JS self-test scenarios as `#[test]`s, implement to green, commit.

### Task C2: life-gpu — EvolveSim + WGSL + parity self-test

**Files:** Create `crates/life-gpu/src/step_evolve.wgsl`, `crates/life-gpu/src/evolve_sim.rs`; modify `lib.rs`, `sim.rs` (none) — self-test gains an evolve check.

**Interfaces:**
- `pub struct EvolveSim` — `new(ctx, &EvolveState, env: &[f32], kernel, rule, env_weight, mut_sigma, topo, seed)`, `step(ctx)`, `read_back(ctx) -> EvolveState`, `front_view()`, `env_view()`, `set_params(ctx, env_weight, mut_sigma)` (rule/kernel changes rebuild the sim).
- State texture Rgba32Float (R=clan 0/1/2, G=τ, B=θ), env R32Float. Kernel uploaded as a uniform array (25 vec4 slots max: dc, dr, w, pad).
- WGSL: same neighbor walk order as CPU; `floor(s + 0.5)` rounding; PCG+Box–Muller mutation drawing per newborn.
- `gpu_self_test` part 3: evolve at σ_mut=0 (mutation path inert) — GPU step equals CPU `step_evolve` exactly (f32 both sides).

### Task C3: life-render evolve path + life-app Evolve mode

**Files:** Create `crates/life-render/src/draw_evolve.wgsl`; modify `life-render/src/lib.rs` (RenderSource enum, colour-mode uniform), `life-app/src/app.rs`.

**Interfaces:**
- `pub enum RenderSource<'a> { Binary(&'a wgpu::TextureView), Evolve { state: &'a wgpu::TextureView, env: &'a wgpu::TextureView } }`; `Renderer::new(device, format, source, size)`, `set_source(...)`, `set_color_mode(mode: u32)` (0 clan, 1 τ viridis, 2 θ magma, 3 env).
- App: `SimMode::Evolve`; `SimKind` enum wrapping `Sim`/`EvolveSim`. Panel: kernel selector (Moore/5×5 with default rule swap), rule text, envWeight + σ_mut + seed-τ + density sliders, Generate/Flat terrain, colour-cycle button, clan populations + mean τ/θ stats (readback every 8 gens).

## Completion checklist

- [ ] `cargo test --workspace` green (A+B+C).
- [ ] Boot self-test: deterministic, stochastic@limit, evolve@σ0 — all pass.
- [ ] App: Evolve mode shows two-clan dynamics; terrain generate visibly biases growth; τ/θ colour modes show gene gradients; stats update.

## Deferred (tracked)

- Terrain pen + pen paint + glider drop (pointer picking, all modes).
- Hatch-creature presets (data port is trivial once pointer/preset UX lands).
- Population/τ history strip-chart (browser has it; egui plot later).
