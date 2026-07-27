# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A suite of **browser simulations**, each a self-contained single HTML file — no build step, no bundler, no server. Open one directly (`file://`) and it runs. `index.html` is the landing page that links them.

Three families, and the distinction matters when you pick a pattern to copy:

1. **Cellular automata on a torus** — `life-torus`, `life-stochastic`, `life-evolve`. Conway variants, Three.js + WebGL, GPU-accelerated.
2. **Agent simulations** — `life-ecology` (predator/prey), `life-conveyor` (factory assembly). Not CA. No B/S rules, no torus mesh in the conveyor's case.
3. **A partial Rust rewrite** in `crates/` — all of family 1, plus `life-ecology` from family 2. `life-conveyor` has no port.

The HTML files are siblings, not a component tree. They share *concepts* and deliberately copy code rather than importing it.

## The core/shell split (the current convention)

Newer simulations separate pure simulation math from the page:

| Core module (no DOM, `require()`-able) | Shell | Tests |
|---|---|---|
| `life-stats.js` | `life-stochastic.html` | `life-stats.test.js` |
| `life-ecology-core.js` | `life-ecology.html` | `life-ecology-core.test.js` |
| `life-conveyor-core.js` | `life-conveyor.html` | `life-conveyor-core.test.js` |

Cores are UMD-lite: they work as both a `<script src>` global and a Node `require()`. This is what makes the logic testable at all — the browser has no test runner here. **Any new simulation should follow this split.** `life-torus.html` and `life-evolve.html` predate it and keep their rules inline.

## Commands

```bash
node --test                                   # all JS tests (103 currently)
node --test --test-name-pattern "classify"    # one test by name
cargo build --workspace                       # the Rust port
cargo test -p life-core
```

**Gotcha: CI runs Rust only.** `.github/workflows/ci.yml` runs `cargo build --workspace` and `cargo test -p life-core` on three OSes — there is **no Node step**, so all 103 JS tests are manual. Run `node --test` yourself before committing anything touching a `*-core.js`; nothing else will.

The one exception is `life-ecology-core.js`: change its mechanics and `cargo test -p life-core` fails, because `tests/ecology_parity.rs` holds counts generated from it. That failure is correct — it means the Rust port needs the same change and its expected rows regenerating, not that the test is stale.

No `package.json`, no lint config. `node --test` auto-discovers `*.test.js`.

## Files

- `index.html` — landing page. **Adding a simulation means adding a card here**, or it is unreachable from the deployed site.
- `life-torus.html` — deterministic Life. Two-clan signed-sum machinery (warm/cool), weighted-kernel rules, cycle/still-life detection. The base the CA files build on.
- `life-stochastic.html` — birth/survival as Gaussian-bump probabilities around the deterministic B/S counts, plus a headless stability search classifying configs dead/full/stable/chaotic.
- `life-evolve.html` — heritable per-cell genes under spatial selection (see below).
- `life-ecology.html` + `-core.js` — squirrels and pine martens on a torus of trees. Contact-process prey growth + conserved WaTor-style predation. CPU only.
- `life-conveyor.html` + `-core.js` — workers assembling products from parts on parallel belts. Ported from a Python interview assignment (`~/source/cl01`). 2D canvas, no Three.js, no CDN dependency.
- `life-gpu.js` — browser-only `LifeGPU.create(renderer, cfg)`: shared WebGL2 harness (ping-pong compute + colour pass into a render target, on-demand readback). Each sim supplies its own rule/colour fragment shaders and CPU↔texture packing callbacks. **Used by the three CA sims only** — ecology and conveyor are CPU. Each has a boot-time `gpuSelfTest()` asserting the GPU step matches the CPU reference exactly (evolve/torus directly; stochastic at the deterministic limit σ→0). GPU caps grids at 2048², CPU fallback at 300². `life-torus` reads back every generation because loop detection needs the exact state; the other two throttle.
- `crates/` — the Rust workspace: `life-core` (pure math, dependency-free, RNG injected as a closure so it stays wasm-ready), `life-gpu` (wgpu compute), `life-render` (torus mesh, camera, picking), `life-app` (winit + egui). `life-core::ecology` is a port of `life-ecology-core.js`, which stays the reference; `tests/ecology_parity.rs` replays a seeded run against counts generated from the JS and is what keeps the two from drifting.
- `deploy/` — nginx config and notes for the o2 host. Not served.

## Cross-cutting invariants

**Never remove `:root { color-scheme: dark }`** from any page. Without it Chrome's auto-dark heuristic decides these already-dark pages need darkening and rewrites the entire palette — headings and card backgrounds come out wrong. Invisible when testing locally in a light-mode-off browser; very visible to whoever you share the URL with.

**Seeded runs must stay reproducible.** `mulberry32` is consumed as one sequential stream in scan order — that is exactly what makes a seed replay identically, and exactly what blocks a GPU port of `life-ecology` (parallel cells cannot agree on stream position). Any port has to move to a hash RNG keyed on position, which changes results; do not treat that as a refactor.

**A port matching JS has to match its float widths too.** The JS typed arrays are the storage (`Uint8Array` for grids and flags, `Float32Array` for continuous per-cell state), but every parameter is a plain JS number, so the arithmetic is f64 and rounds to f32 exactly once, on store. Computing in f32 throughout is the natural-looking Rust translation and it is wrong: the error accumulates until some threshold comparison flips, and the boards diverge tens of generations later, long after any short test would notice. `life-core::ecology` is the worked example.

**Three.js and OrbitControls come from the jsDelivr CDN.** The torus sims need network access to render at all. `life-conveyor` deliberately does not.

**Boundary topologies:** cells wrap per-axis with `straight` (torus), `none` (hard edge), `flip` (Klein-bottle seam — mirrors the *other* axis). `resolveCell` in `life-stats.js` is canonical; the HTML files carry their own `resolve()` copies, so a fix in one needs checking against the siblings.

**Digit-set B/S rules** parse from strings like `B3/S23` into sets of neighbour counts. In the stochastic and evolve files these become probability tables via `LifeStats.curve` — a Gaussian bump per count, clamped between a floor and ceiling.

## life-evolve genetics (the delicate part)

Two heritable per-cell genes in `Float32Array`s parallel to `grid` (meaningless where the cell is dead):

- **τ (tolerance) ∈ [0,1]** — shields a cell from opposite-clan neighbours: `s = allies − (1−τ)·foes`. τ=0 is plain life-torus behaviour; τ=1 ignores foes. τ protects only *existing* cells (birth always counts foes in full), so selection acts through differential survival at clan borders.
- **θ (terrain tolerance) ∈ [0,1]** — response to a static environment field `env ∈ [−1,1]` (Sugarscape-style basins and ridges). θ=0 specialist, θ=1 terrain-blind generalist. Populations sort by terrain into two genotypes.

Both inherit as the contribution-weighted mean of the winning-clan parents plus Gaussian mutation (`σ_mut`, a UI slider; 0 gives a frozen-genome control). **Read the design specs before changing selection math** — why birth ignores τ, why sorting is overcrowding-driven rather than starvation-driven, are deliberate and documented there.

## Deployment

The o2 host serves `/home/ci4/src/life` directly at `https://ci4o2.zapto.org/life/`, so publishing is `git pull` on o2 — nothing is copied into a web root. This means **anything committed becomes web-reachable**, except the paths denied in `deploy/nginx-life.conf` (dotfiles, `crates/`, `target/`, `docs/`, `deploy/`). See `deploy/README.md`.

## Workflow

Spec-driven: a design spec lands in `docs/superpowers/specs/` (mechanic, scope, out-of-scope) and often a plan in `docs/superpowers/plans/` before implementation. Work ships in numbered **slices**. `.superpowers/sdd/` holds per-task briefs and reviews for in-flight work.

Commits follow Conventional Commits scoped to the simulation:

```
feat(life-evolve): slice 2b — terrain-tolerance gene theta
feat(life-conveyor): parallel belts, product mixes and worker specialisation
```

Findings that a reader would otherwise mistake for bugs belong in the page's own Help dialog, not only in the commit message — e.g. the conveyor's just-in-time deadlock and its starved-tail behaviour are both explained in-page because both look broken and are not.
