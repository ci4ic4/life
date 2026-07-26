# Life

Browser simulations, each a **single self-contained HTML file**. No build step, no bundler, no server — clone the repo, open a file, it runs.

**Live: [ci4o2.zapto.org/life](https://ci4o2.zapto.org/life/)**

Most began as Conway's Life variants rendered on a 3D torus. The newer ones are not cellular automata at all, but they reuse the skin, the seeded-run machinery and the testing pattern.

| | What it is |
|---|---|
| [`life-torus.html`](life-torus.html) | Deterministic Life on a torus. Two clans, weighted-kernel rules, cycle and still-life detection. The base the others build on. |
| [`life-stochastic.html`](life-stochastic.html) | Birth and survival become Gaussian probabilities around the usual neighbour counts. Includes a headless search that classifies a configuration as dead, full, stable or chaotic. |
| [`life-evolve.html`](life-evolve.html) | Heritable per-cell genes under spatial selection — tolerance to rival clans, and to a static terrain field. Populations sort themselves by genotype. |
| [`life-ecology.html`](life-ecology.html) | Squirrels and pine martens on a torus of trees. Predator and prey chase each other around the classic cycle, or collapse. |
| [`life-conveyor.html`](life-conveyor.html) | Workers on either side of parallel belts assemble products from parts, competing for each cell. Throughput, fairness, and starvation. |

Every simulation has a **Help** button explaining its rules and what is worth watching.

## What's interesting in them

Each exists to make one specific effect visible, and the Help dialogs document the results rather than just the controls.

**Pine martens suppress grey squirrels** — the real England/Wales phenomenon. `life-ecology` reproduces it from a single asymmetry: reds co-evolved with the marten, stay in the canopy and flee early; greys are North-American and ground-naive, so they are the easy, preferred meal. Reintroduce the marten and greys crash while reds rebound. A sparse travelling predator is enough — it works at low density. (Basis: Sheehy & Lawton et al.; Vincent Wildlife Trust, mid-Wales. Habitat preference — conifer favouring reds, broadleaf greys — is real but second-order, and deliberately left out.)

**Selection acts through survival, not birth** — in `life-evolve`, the tolerance gene τ shields an existing cell from rival neighbours but is ignored when a cell is born. That one asymmetry is what makes clan borders the place where evolution actually happens.

**A longer conveyor does almost nothing** — in `life-conveyor`, every worker is a sieve on one shared supply, so output decays geometrically down the belt and the far workers sit starved, hoarding parts they can never pair. The panel marks where 90% of production has already happened. Slower assembly *extends* the useful length rather than capping it, and a gapless just-in-time feed deadlocks the line outright, because a finished product needs an empty cell to leave on.

## Running the tests

The simulation rules live in plain modules with no DOM — `life-stats.js`, `life-ecology-core.js`, `life-conveyor-core.js` — shared between the browser pages and Node's built-in test runner:

```sh
node --test
```

No dependencies to install. The GPU-accelerated simulations additionally self-test at startup, asserting that the GPU step matches the CPU reference exactly.

## Rust rewrite

`crates/` holds a native port of the three cellular-automata simulations — `life-core` (pure math, dependency-free), `life-gpu` (wgpu compute), `life-render` (torus mesh and camera), `life-app` (winit + egui).

```sh
cargo run -p life-app
```

The ecology and conveyor simulations are browser-only. Their arbitration steps are sequential by construction, which is exactly what lets a seeded run replay identically — and what a parallel port would have to give up.

## Licence

Dual-licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT licence ([`LICENSE-MIT`](LICENSE-MIT))

at your option. This covers the whole repository — the browser simulations as well as the Rust crates, which already declared `MIT OR Apache-2.0`.

Unless you state otherwise, any contribution you intentionally submit for inclusion shall be dual-licensed as above, with no additional terms.
