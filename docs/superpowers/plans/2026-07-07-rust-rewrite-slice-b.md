# Rust Rewrite — Slice B (life-stochastic) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the stochastic Life variant to the native app: Gaussian-bump probability rules, GPU step with in-shader hash PRNG, headless stability-search (`run_trial`/`classify`) on a background thread, and a mode selector in the panel.

**Architecture:** `life-core` gains the probability machinery ported 1:1 from `life-stats.js` (its tests are the acceptance bar). `life-gpu::Sim` generalizes to a `SimRule` enum — deterministic keeps the existing shader; stochastic gets a second WGSL shader with a PCG hash PRNG seeded per (cell, generation). Self-test extends: stochastic pipeline with degenerate 0/1 tables must equal the deterministic CPU step exactly (the σ→0 trick from the browser version). `life-app` gains a mode switch, σ/floor/ceil/density controls, and a stability-search panel running `run_trial` on a `std::thread`.

**Tech Stack:** unchanged (wgpu 29, winit 0.30, egui 0.35). No new dependencies.

## Global Constraints

- All Slice A constraints hold (no async runtime, no new crates, injected randomness, existing tests stay green).
- **Randomness enters as `&mut impl FnMut() -> f64`** (matches the JS `rng()` contract exactly; `rand::Rng` adapts via a closure). Deviation from the spec's `impl Rng` noted deliberately: the closure form is what the ported tests require (forced `|| 0.99` / `|| 0.0` values).
- **Stochastic acceptance = `rng() < p` strict**, probabilities from `curve` (floor + span·max-of-Gaussian-bumps), thresholds `WINDOW=50, DEAD_T=0.02, FULL_T=0.95, STABLE_T=0.03`.
- **GPU/CPU stochastic parity is only asserted at the deterministic limit** (tables of exact 0/1); with live σ the two RNGs legitimately differ.
- Slice A's deterministic self-test must keep passing untouched.

---

### Task B1: life-core — curve, RingStats, classify

**Files:**
- Create: `crates/life-core/src/prob.rs` (bump_max, curve, CurveParams)
- Create: `crates/life-core/src/stats.rs` (RingStats, Verdict, classify, Thresholds)
- Modify: `crates/life-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct CurveParams { pub sigma: f64, pub floor: f64, pub ceil: f64 }`
  - `pub fn bump_max(bs_mask: u16, n: u8, sigma: f64) -> f64` — max Gaussian bump over the digit-set members (bitmask reuses `BS` representation).
  - `pub fn curve(mask: u16, p: CurveParams) -> [f64; 9]`
  - `pub struct RingStats` — `new(window)`, `push(v)`, `full()`, `mean()`, `std_dev()`, `reset()`
  - `pub struct Thresholds { pub window: usize, pub dead: f64, pub full: f64, pub stable: f64 }` + `Thresholds::DEFAULT`
  - `pub enum Verdict { Dead, Full, Stable, Chaotic }`
  - `pub fn classify(mean: f64, std_dev: f64, t: &Thresholds) -> Verdict`

**Steps:** port each JS test (`bumpMax peaks…`, `curve applies floor/ceiling…`, `makeRingStats…`, `classify labels…`) as `#[test]`, implement to green, commit `feat(rust): slice B task 1 — life-core curves, ring stats, classify`.

Key implementation (curve):
```rust
pub fn bump_max(mask: u16, n: u8, sigma: f64) -> f64 {
    let mut p = 0.0f64;
    for c in 0..=8u8 {
        if mask & (1 << c) == 0 { continue; }
        let d = n as f64 - c as f64;
        p = p.max((-(d * d) / (2.0 * sigma * sigma)).exp());
    }
    p
}
pub fn curve(mask: u16, prm: CurveParams) -> [f64; 9] {
    let span = (prm.ceil - prm.floor).max(0.0);
    std::array::from_fn(|n| prm.floor + span * bump_max(mask, n as u8, prm.sigma))
}
```

### Task B2: life-core — step_stochastic + run_trial

**Files:**
- Modify: `crates/life-core/src/grid.rs` (add `step_stochastic`)
- Create: `crates/life-core/src/trial.rs` (`TrialOpts`, `TrialResult`, `run_trial`)
- Modify: `crates/life-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub fn step_stochastic(g: &Grid, p_birth: &[f64; 9], p_survive: &[f64; 9], t: Topology, rng: &mut impl FnMut() -> f64) -> Grid`
  - `pub struct TrialOpts { pub w: u32, pub h: u32, pub topo: Topology, pub gens: u32, pub thresholds: Thresholds, pub birth: u16, pub survive: u16, pub b_parm: CurveParams, pub s_parm: CurveParams, pub density: f64 }`
  - `pub struct TrialResult { pub mean: f64, pub std_dev: f64, pub status: Verdict }`
  - `pub fn run_trial(o: &TrialOpts, rng: &mut impl FnMut() -> f64) -> TrialResult` — seed at density, step `gens`, push population ratio per gen, classify.

**Steps:** port the three JS behaviour tests (isolated-cell-dies with `|| 0.5`, always-fail rng → Dead, always-pass rng → Full), implement, commit `feat(rust): slice B task 2 — stochastic step + headless trial`.

### Task B3: life-gpu — SimRule enum + stochastic WGSL + extended self-test

**Files:**
- Create: `crates/life-gpu/src/step_stoch.wgsl`
- Modify: `crates/life-gpu/src/sim.rs` (SimRule, per-step params update, gen counter)
- Modify: `crates/life-gpu/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub enum SimRule { Deterministic(BS), Stochastic { p_birth: [f32; 9], p_survive: [f32; 9], seed: u32 } }`
  - `Sim::new(ctx, init, rule: SimRule, topo)` (breaking change from `bs: BS`; call sites updated in B4)
  - self-test part 2: stochastic pipeline with 0/1 tables == CPU deterministic step, exact.

Stochastic WGSL core (PCG hash, tables packed as `array<vec4<f32>, 3>` for uniform stride):
```wgsl
fn pcg(v: u32) -> u32 {
    var s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}
// rand01 in [0,1): hash(cell_index ^ hash(gen ^ seed))
// accept: rand01 < p[n]  (strict, matches CPU)
```
Params gain `generation: u32` (bumped each step; params buffer becomes COPY_DST and is rewritten per step for the stochastic rule).

**Steps:** shader, SimRule plumbing, `gpu_self_test` runs both pipelines, boot prints `gpu_self_test: OK (deterministic, stochastic@limit)`. Commit `feat(rust): slice B task 3 — stochastic GPU step + extended self-test`.

### Task B4: life-app — mode selector, stochastic controls, stability search

**Files:**
- Modify: `crates/life-app/src/app.rs`

**Interfaces:**
- Produces: `Mode { Torus, Stochastic }` selector in the panel. Stochastic controls: σ, floor, ceil, seed-density sliders + Reseed button. Stability-search section: Run button → spawns `std::thread` running N `run_trial`s (seeded from `std::time` entropy via a small xorshift), results (`Verdict` counts + mean/σ) sent back over `std::sync::mpsc`, polled non-blocking each frame.

**Steps:** mode enum + rebuild plumbing, sliders, thread + channel, verify: app runs in both modes, stochastic mode shows organic noise-driven growth, search reports without blocking the UI. Commit `feat(rust): slice B task 4 — stochastic mode, controls, stability search`.

## Completion checklist

- [ ] `cargo test --workspace` green (Slice A tests + new B tests).
- [ ] Boot self-test passes for both pipelines.
- [ ] App: mode switch works, σ slider visibly changes dynamics, stability search returns verdicts without UI freeze.

## Deferred (tracked)

- Klein/hard-edge UI toggle (core supports all topologies; panel exposes torus only until Slice C polish).
- GPU-side stability search (CPU `run_trial` on a thread is fast enough at 100²; revisit if searches at 2048² are wanted).
