# Life-Star — The Life of a Star (2D cross-section + HR track)

**Date:** 2026-08-14
**Status:** approved (design), not yet sliced
**Files:** `life-star-core.js`, `life-star.html`, `life-star-core.test.js` (all new)

## Goal

A watchable toy of stellar evolution: a star of chosen initial mass ignites,
sits on the main sequence, swells, builds an onion of fusion ash, and dies as a
white dwarf, a neutron star or a black hole. The observer should come away with
a feel for three things they probably did not have before:

1. **A star is a fight between gravity and pressure**, and every stage is that
   fight being re-fought with a different fuel.
2. **Mass decides everything.** The same physics with a different starting mass
   gives a completely different life and a completely different corpse.
3. **The interesting parts are vanishingly short.** Ten billion years of nothing,
   then a day of silicon burning.

This is explicitly a *toy*. Real stellar evolution is a two-point boundary value
problem solved by codes like MESA over decades of development. What follows is
the largest amount of honest physics that fits in ~400 lines and runs in a
browser tab — enough that the headline results are *derived* rather than
scripted, and no more.

## Not a cellular automaton

Worth stating plainly because every other simulation in this repo is one. There
is no grid, no torus, no B/S rule, no neighbour count, no `resolveCell`, and no
`mulberry32`. The `life-gpu.js` harness is not used. `life-star` shares the
suite's visual skin and its core/shell/test split, and nothing else.

**The simulation is fully deterministic.** Its only inputs are initial mass and
metallicity; there is no RNG anywhere. The suite's "seeded runs must stay
reproducible" invariant is satisfied trivially rather than carefully.

## What 2D means here, and what it costs

The request was for a 2D simulation. Two things have to be said about that.

**True 2D gravity is not gravity.** The 2D Green's function of the Poisson
equation gives force ∝ 1/r and potential ∝ ln r — a confining potential in which
nothing ever escapes and no Keplerian orbit exists. Any 2D gravity toy that
behaves recognisably is quietly using a 1/r² law, which is a three-dimensional
force law in a flat world.

**The endpoints are 3D results.** The Chandrasekhar mass exists because
relativistic degeneracy pressure scales as ρ^(4/3), which in three dimensions
makes the n = 3 polytrope exactly marginal — its mass is independent of its
radius. That marginality is a statement about three-dimensional space. In two
dimensions the exponents shift and the sharp limit does not survive.

**Resolution:** the physics is one-dimensional and spherically symmetric — the
honest geometry for a star — and the *rendering* is 2D, as an equatorial
cross-section. The observer sees a 2D disc; the solver never pretends space is
flat. This gets the real physics and the requested picture at the same time, and
the only thing given up is a self-consistently two-dimensional universe, which
was never going to produce stars anyway.

## Architecture

Follows the mandatory core/shell split.

| File | Role |
|---|---|
| `life-star-core.js` | Pure physics. UMD-lite (script global + `require()`), no DOM. |
| `life-star.html` | 2D canvas shell: cross-section panel, HR panel, controls, Help. |
| `life-star-core.test.js` | `node --test`. |
| `index.html` | New card. Without it the page is unreachable from the deployed site. |

**No Three.js, no CDN.** A cross-section disc and a scatter plot are 2D canvas
work, as in `life-conveyor.html`. The page therefore works offline, unlike the
three torus simulations.

**No GPU.** Roughly 200 shells stepped ~10⁴ times is microseconds of CPU per
frame. `life-gpu.js` exists for grid CA with ping-pong textures and would be
pure overhead here.

### Float width — a deliberate departure

The suite's convention is `Float32Array` for continuous per-cell state. **That
convention must be broken here, and the reason belongs in a comment at the top
of the file** so the next reader does not "fix" it back for consistency.

Density spans roughly 10⁻⁶ g/cm³ in the envelope to 10¹⁰ g/cm³ in a degenerate
core — sixteen orders of magnitude. Pressure spans worse. `Float32` carries
about seven significant digits and tops out near 3.4 × 10³⁸, which silicon
burning densities and radiation pressures overrun outright. All continuous state
is `Float64Array`.

This is the mirror image of the trap already documented in `CLAUDE.md` for the
ecology port. There the danger was *computing* in f32 where the JS computed in
f64; here it is *storing* in f32 where the dynamic range forbids it. Same root
cause: float width is physics, not house style.

## The core model

Structure and composition are tracked separately. This is the load-bearing
approximation of the whole design.

**Global structure** comes from the Lane-Emden equation, solved fresh each step
for the star's current effective polytrope index `n`:

```
(1/ξ²) d/dξ (ξ² dθ/dξ) = −θⁿ ,    θ(0) = 1 ,  θ'(0) = 0
```

integrated by RK4 to the first zero ξ₁. Physical quantities follow from the
solution:

```
ρ(ξ) = ρ_c θⁿ
P     = K ρ^(1 + 1/n)
r     = a ξ ,      a² = (n+1) K ρ_c^(1/n − 1) / (4πG)
M     = 4π a³ ρ_c · (−ξ² dθ/dξ)|_{ξ₁}
```

Reference values, useful as unit-test anchors: for n = 1.5, ξ₁ = 3.65375 and
(−ξ²θ′)|_{ξ₁} = 2.71406; for n = 3, ξ₁ = 6.89685 and (−ξ²θ′)|_{ξ₁} = 2.01824.

**How `n` is chosen.** It is a continuous value, not a switch between two cases.
`eos(ρ_c, T_c, μ, μ_e)` evaluates the total central pressure as the sum of ideal
gas, radiation, and electron degeneracy (blended between the non-relativistic
ρ^(5/3) and relativistic ρ^(4/3) limits), then returns the *local logarithmic
slope* `Γ = d ln P / d ln ρ` at that point. The polytrope index is `n = 1/(Γ−1)`,
clamped to [1.0, 3.4]. The familiar values are the limiting cases this produces:
a fully convective or non-relativistic degenerate star lands near n = 1.5, a
radiation-dominated or relativistic one near n = 3.

`n` moves slowly, so `laneEmden(n)` is memoised on `n` rounded to a step of 0.05.

**Composition** lives on a fixed Lagrangian grid of ~200 mass shells, each
holding mass fractions of:

```
H1, He4, C12, O16, Ne20, Mg24, Si28, Fe56    (+ Li7, see below)
```

Lagrangian means the shells are labelled by enclosed mass and never exchange
material. There is no convective mixing in this model — a real simplification,
flagged below.

### The step

```
1. laneEmden(n)                    → θ(ξ) profile (memoised)
2. structure(M, n, K)              → ρ(m), T(m) sampled onto the mass grid
3. burn(ρ, T, X, dt) per shell     → dX, energy release
4. sum energy                      → luminosity L, effective temperature T_eff
5. eos + composition               → new mean molecular weight μ, new n and K
6. classifyEndpoint(state)         → terminal condition reached?
7. advance age by dt
```

Six functions: `laneEmden`, `structure`, `eos`, `burn`, `step`,
`classifyEndpoint`. Around 400 lines total.

## Why the onion emerges

The layered structure is not drawn and not scripted. It falls out of the
temperature sensitivity of the reaction rates meeting the star's temperature
gradient.

The *effective* temperature exponents near their respective ignition points are
roughly T⁴ for the pp chain, T¹⁷ for the CNO cycle, and **T⁴⁰** for the
triple-alpha reaction. Temperature falls with radius. A reaction therefore burns
in a shell whose thickness is set by how quickly T drops below its cliff —
triple-alpha's T⁴⁰ is precisely why helium burns in a razor-thin shell while
hydrogen burns in a fat one.

The visible onion is thus *evidence the physics is working*, not decoration. If
the layers come out the wrong thickness, the rates are wrong.

### Implement the Gamow forms, not the power laws

The exponents above are local logarithmic slopes that explain the behaviour.
They are **not** what the code should evaluate. Computing `Math.pow(T/1e8, 40)`
overflows to `Infinity` or flushes to zero on small drifts in T.

Use the standard Gamow-peak forms instead, which are both better conditioned and
valid over a far wider temperature range (ε in erg g⁻¹ s⁻¹, T₆ and T₈ in units
of 10⁶ K and 10⁸ K):

```
ε_pp   ≈ 2.4e4  · ρ X²        · T₆^(−2/3) · exp(−33.80 · T₆^(−1/3))
ε_CNO  ≈ 4.4e25 · ρ X X_CNO   · T₆^(−2/3) · exp(−152.28 · T₆^(−1/3))
ε_3α   ≈ 5.1e8  · ρ² Y³       · T₈^(−3)   · exp(−44.027 / T₈)
```

Accumulate in log space where products span many decades. A unit test asserts
`ε_3α` at T = 10⁹ K is finite and non-zero, which is the direct guard against
the overflow this note exists to prevent.

**Advanced burning (C, Ne, O, Si) is approximated.** These are not simple
chains — silicon burning in particular is a quasi-equilibrium network of
hundreds of reactions, not a reaction. The toy models each as threshold ignition
above a characteristic temperature, energy release from the binding-energy
difference, and a characteristic burning timescale. This is the largest
knowingly-crude piece of the model and should be labelled as such in the Help
dialog.

Ignition temperatures, approximately: H 15 MK, He 100 MK, C 600 MK, Ne 1.2 GK,
O 1.5 GK, Si 2.7 GK.

### Lithium

Li7 is carried for one reason: it burns at about 2.5 MK, *below* hydrogen
ignition, so it is destroyed while the star is still contracting toward the main
sequence. It gives the observer something visible to watch happen in the first
moments, before ignition, and it corrects the common intuition — reflected in the
original request — that lithium is something stars make. They only destroy it.

## Time

Physics timestep is set by the nuclear timescale, `dt ∝ E_available / L`, which
is automatically fine during fast burning and coarse on the main sequence. A
full lifetime takes roughly 10⁴ steps.

**`dt` must be capped by the fastest-burning shell, not a global average.**
Otherwise a 10⁶-year step steps straight over silicon burning, which lasts about
a day.

Presentation is decoupled from physics: a log-scale "years per second" control,
plus automatic slowdown when an event fires (ignition, helium flash, shell
burning onset, envelope loss, collapse). The physics stays honest and the pacing
stays watchable without the two fighting.

**The main sequence must look deliberately, legibly boring** — an explicit
elapsed-time readout and a visible "years per second" figure — rather than
accidentally frozen. Ten of a twelve-billion-year life is a circle that slowly
does not change, and an observer with no readout will assume the page has hung.

## Death

| Condition | Outcome |
|---|---|
| Core never reaches 0.08 M☉ ignition | Brown dwarf — never a star |
| Envelope shed, remaining core < 1.44 M☉ | White dwarf (He, CO or ONeMg by burning reached) |
| Iron core exceeds 1.44 M☉ | Core collapse → neutron star |
| Core above ~2.5 M☉ | Black hole |

### 1.44 is derived; 2.5 is asserted

This asymmetry is the most interesting honesty point in the design and belongs
in the Help dialog.

For n = 3 the polytrope mass is

```
M = 4π (K / πG)^(3/2) · 2.01824
```

with no ρ_c in it — **the mass does not depend on the central density**, which is
exactly why a limit exists at all. Substituting the relativistic degenerate
constant K₂ = (hc/8)(3/π)^(1/3) (μ_e m_H)^(−4/3) gives M_Ch ≈ 1.456 (2/μ_e)² M☉.
The solver already runs this machinery, so the Chandrasekhar mass costs nothing
extra and comes out as a *result*.

The TOV limit near 2.5 M☉ has no such derivation available. It depends on the
equation of state of matter above nuclear density, which is an open research
question, not a calculation. It is hardcoded, and the page should say so.

### Where the solver stops

Core collapse takes about one second, is hydrodynamic and neutrino-driven, and
occurs precisely when hydrostatic equilibrium fails — the assumption the entire
model rests on. The solver's job therefore ends at *detecting* the terminal
condition; the death itself is a scripted terminal animation.

This is the boundary of quasi-statics, not a shortcut. No toy simulation crosses
it, and the Help dialog should explain why rather than leaving the observer to
assume the simulation simply gave up.

## Presentation

Two panels.

**Left — cross-section.** Animated disc, concentric rings coloured by dominant
species, radius on a log axis because the star spans a factor of ~100 in radius
between the main sequence and the giant branch and ~10⁴ more down to a white
dwarf. Readouts: mass, radius, central temperature, central density, age, phase.

**Right — Hertzsprung-Russell diagram.** The star's track drawn live in
luminosity versus effective temperature. Both quantities already fall out of the
solver every step, so the panel is nearly free — and it is the view that makes
the physics legible, because the main sequence, the giant branch and the white
dwarf cooling track all appear as shapes an observer can recognise from any
astronomy book.

## Help dialog — "looks broken, isn't"

Per the suite's convention, findings a reader would otherwise mistake for bugs
belong in the page, not only in the commit message.

- The star gets **bigger and cooler** as it runs out of fuel. Looks like a sign
  error; it is the red giant branch.
- Luminosity jumps by orders of magnitude at the **helium flash**. Looks like
  numerical blow-up; it is real, and it is over in minutes.
- **Low-mass stars stop at helium and never reach iron.** Looks stuck; they
  simply never get hot enough. Iron requires roughly 8 M☉ or more.
- **Lithium vanishes almost immediately**, before the star ignites.
- **The main sequence looks frozen** for most of the run, because it is. About
  80% of all the nuclear energy a star will ever release comes from hydrogen
  burning alone.
- **Why 1.44 M☉ is derived and 2.5 M☉ is hardcoded.**
- **Why the supernova is an animation** rather than a simulation.
- **Advanced burning stages are approximated**, unlike H and He burning.

## Testing

`node --test`, as usual, and manual — CI runs Rust only.

Unlike every other simulation in this repo, **this one can be tested against
external ground truth.** `life-torus`, `life-ecology` and `life-conveyor` have no
authority to check against, which is why `tests/ecology_parity.rs` takes the form
it does — the JS *is* the reference because nothing more authoritative exists.
`life-star` has a century of astrophysics to check against, and the assertions
have known right answers:

| Test | Expected |
|---|---|
| **Chandrasekhar limit** (load-bearing) | 1.44 ± 0.05 M☉ |
| Lane-Emden anchors | n=1.5: ξ₁ = 3.65375; n=3: ξ₁ = 6.89685 |
| 1 M☉ main-sequence lifetime | ~10 Gyr, within a factor of 2 |
| Mass–luminosity on the main sequence | L ∝ M^3.5, within a factor of 2 |
| Endpoint map | 1 M☉ → white dwarf; 10 M☉ → neutron star; 30 M☉ → black hole |
| Triple-alpha at T = 10⁹ K | finite and non-zero (overflow guard) |
| Determinism | identical inputs give bit-identical output |

The Chandrasekhar test is the one that matters most: it fails if either the
Lane-Emden integration or the degenerate equation of state is wrong, and those
are the two pieces everything else depends on.

## Explicitly out of scope

- Convective mixing and dredge-up. Shells are Lagrangian and never exchange
  material. This is the largest omission and it is why the onion layers will be
  cleaner than a real star's.
- Opacity tables. No radiative transfer; the effective temperature comes from a
  simple surface condition.
- Mass loss by stellar wind, except as a scripted envelope-loss event.
- Rotation, magnetic fields, binary interaction.
- Nucleosynthesis beyond iron (the r- and s-processes).
- The supernova itself, as described above.
- A Rust port. The suite's Rust workspace covers the CA family and ecology;
  extending it to `life-star` is a separate decision, not implied by this spec.

## Open question for slicing

Whether the mass-family comparison — several masses racing across one HR diagram
to different endpoints — lands as a later slice. It is the best demonstration of
the mass-endpoint relationship and turns the page into something closer to the
stability search in `life-stochastic`, but it is meaningfully more UI and state
to manage. Deferred, not rejected.
