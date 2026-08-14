# Life-Star Slice 1 (Main Sequence + Mass Family) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A watchable browser page where four stars of different mass (0.5, 1, 8, 25 M☉) sit on the main sequence burning hydrogen, each drawn as a 2D cross-section and each tracing its own live track across a shared Hertzsprung-Russell diagram on one logarithmic clock.

**Architecture:** A new `life-star-core.js` (UMD-lite, like `life-stats.js`) holding pure physics with no DOM: a Lane-Emden RK4 integrator, an equation of state returning the adiabatic index, an equilibrium structure solver, a Gamow-form burning network, and a single-star `step()`. A new `life-star.html` 2D-canvas shell holds an array of independent star states and steps each one. The core never knows more than one star exists.

**Tech Stack:** Vanilla JS, 2D canvas (no Three.js, no CDN, works offline), Node built-in test runner (`node --test`). No build step, no new dependencies.

## Global Constraints

- Single-file HTML app opened via `file://` — no server, no bundler (suite rule).
- Core is UMD-lite: usable as `<script src>` global `LifeStar` and as Node `require()` (mirror `life-stats.js`).
- **All continuous state is `Float64Array`, never `Float32Array`.** Density spans ~10⁻⁶ to 10¹⁰ g/cm³ and pressure spans more; f32 has ~7 significant digits and overflows near 3.4e38. This deliberately breaks the suite convention — a comment at the top of the file must say why, or a later reader will "fix" it back.
- **No RNG anywhere.** The simulation is deterministic; inputs are initial mass and metallicity only. No `mulberry32`, no injected `rng` parameter.
- No grid, no torus, no `resolveCell`, no B/S rules, no `life-gpu.js`.
- All units **cgs**. Every function's units are stated in its doc comment.
- Never evaluate rate power laws as `Math.pow(T/T0, 40)`. Use the Gamow-peak exponential forms given in Task 4.
- `:root { color-scheme: dark }` must be present in the HTML (suite invariant — without it Chrome's auto-dark heuristic rewrites the palette).
- Commits: Conventional Commits scoped to the sim, e.g. `feat(life-star): ...`.
- CI runs Rust only. **Run `node --test` yourself before every commit** — nothing else will.

---

## File Structure

- **Create** `life-star-core.js` — pure physics. Five units with clear boundaries: `laneEmden` (dimensionless structure), `eos` (thermodynamics), `structure` (equilibrium solve), `burn` (nuclear rates), `step` (time advance). No DOM, no canvas, no knowledge of multiple stars.
- **Create** `life-star-core.test.js` — Node `node:test` asserts. Unusually for this repo, most assertions check against *external* ground truth (analytic Lane-Emden solutions, tabulated polytrope constants, textbook solar values) rather than regression baselines.
- **Create** `life-star.html` — browser shell: cross-section canvas, HR canvas, star-family state array, controls, phase readouts.
- **Modify** `index.html` — add the card. Without it the page is unreachable from the deployed site.

---

## Physical constants (cgs) — define once at the top of `life-star-core.js`

```js
const G      = 6.67430e-8;      // cm^3 g^-1 s^-2
const K_B    = 1.380649e-16;    // erg K^-1
const M_U    = 1.66053907e-24;  // g (atomic mass unit)
const A_RAD  = 7.565733e-15;    // erg cm^-3 K^-4
const SIGMA  = 5.670374e-5;     // erg cm^-2 s^-1 K^-4
const M_SUN  = 1.98892e33;      // g
const R_SUN  = 6.957e10;        // cm
const L_SUN  = 3.828e33;        // erg s^-1
const YEAR   = 3.1557e7;        // s

// Degenerate electron pressure coefficients, standard cgs form
// P_nr  = K1 * (rho/mu_e)^(5/3)      P_rel = K2 * (rho/mu_e)^(4/3)
const K1 = 1.0036e13;
const K2 = 1.2435e15;

// Nuclear energy release per gram of fuel consumed
const Q_H  = 6.3e18;   // erg/g of hydrogen burned to helium (0.007 c^2)
const Q_HE = 5.8e17;   // erg/g of helium burned to carbon
```

---

### Task 1: Lane-Emden integrator

**Files:**
- Create: `life-star-core.js`
- Test: `life-star-core.test.js`

**Interfaces:**
- Consumes: nothing (leaf module).
- Produces:
  - `laneEmden(n) -> { n, xi1, massFactor, xi: Float64Array, theta: Float64Array }`
    where `xi1` is the first zero of θ, `massFactor` is `(−ξ²·dθ/dξ)` evaluated at `ξ1`, and the two arrays are the sampled profile. Memoised on `n` rounded to 0.05.

**Background for the implementer.** The Lane-Emden equation describes the structure of a self-gravitating sphere whose pressure and density obey `P = K·ρ^(1+1/n)`:

```
(1/ξ²) d/dξ (ξ² dθ/dξ) = −θⁿ ,    θ(0) = 1 ,  θ'(0) = 0
```

`ξ` is dimensionless radius and `ρ = ρ_c·θⁿ`. Integrate outward until θ crosses zero; that is the stellar surface. Rewrite as two first-order equations for RK4: `dθ/dξ = φ`, `dφ/dξ = −θⁿ − 2φ/ξ`.

**The singularity at ξ = 0 must be handled.** The `2φ/ξ` term is `0/0` there. Start the integration at `ξ = 1e-4` using the series expansion `θ ≈ 1 − ξ²/6 + n·ξ⁴/120` and `φ ≈ −ξ/3 + n·ξ³/30`. Starting at exactly zero produces `NaN` immediately.

**θ can go slightly negative before you notice.** `Math.pow(negative, 1.5)` is `NaN`. Clamp `θⁿ` to zero when θ ≤ 0, and stop the integration on the first non-positive θ, interpolating linearly for `xi1`.

- [ ] **Step 1: Write the failing test**

Create `life-star-core.test.js`:

```js
const test = require('node:test');
const assert = require('node:assert');
const LS = require('./life-star-core.js');

// n = 0 and n = 1 have exact closed-form solutions. They are the only
// unambiguous checks available, so they come first.
test('laneEmden n=0 matches the analytic solution xi1 = sqrt(6)', () => {
  const r = LS.laneEmden(0);
  assert.ok(Math.abs(r.xi1 - Math.sqrt(6)) < 1e-3, `xi1 was ${r.xi1}`);
  // theta = 1 - xi^2/6, so -xi^2 theta' = xi^3/3
  const expected = Math.pow(Math.sqrt(6), 3) / 3;
  assert.ok(Math.abs(r.massFactor - expected) / expected < 1e-3,
    `massFactor was ${r.massFactor}, expected ${expected}`);
});

test('laneEmden n=1 matches the analytic solution xi1 = pi', () => {
  const r = LS.laneEmden(1);
  assert.ok(Math.abs(r.xi1 - Math.PI) < 1e-3, `xi1 was ${r.xi1}`);
  // theta = sin(xi)/xi, so -xi^2 theta' at xi1 = pi
  assert.ok(Math.abs(r.massFactor - Math.PI) / Math.PI < 1e-3,
    `massFactor was ${r.massFactor}`);
});

// The two indices the simulation actually uses, against tabulated values.
test('laneEmden n=1.5 matches tabulated values', () => {
  const r = LS.laneEmden(1.5);
  assert.ok(Math.abs(r.xi1 - 3.65375) / 3.65375 < 1e-3, `xi1 was ${r.xi1}`);
  assert.ok(Math.abs(r.massFactor - 2.71406) / 2.71406 < 1e-3,
    `massFactor was ${r.massFactor}`);
});

test('laneEmden n=3 matches tabulated values', () => {
  const r = LS.laneEmden(3);
  assert.ok(Math.abs(r.xi1 - 6.89685) / 6.89685 < 1e-3, `xi1 was ${r.xi1}`);
  assert.ok(Math.abs(r.massFactor - 2.01824) / 2.01824 < 1e-3,
    `massFactor was ${r.massFactor}`);
});

test('laneEmden profile is finite everywhere and monotonically decreasing', () => {
  const r = LS.laneEmden(3);
  for (let i = 0; i < r.theta.length; i++) {
    assert.ok(Number.isFinite(r.theta[i]), `theta[${i}] not finite`);
  }
  for (let i = 1; i < r.theta.length; i++) {
    assert.ok(r.theta[i] <= r.theta[i - 1] + 1e-12, `theta rose at index ${i}`);
  }
});

test('laneEmden memoises: same n returns the identical object', () => {
  assert.strictEqual(LS.laneEmden(3), LS.laneEmden(3));
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test life-star-core.test.js`
Expected: FAIL — `Cannot find module './life-star-core.js'`

- [ ] **Step 3: Write minimal implementation**

Create `life-star-core.js`:

```js
// life-star-core.js — pure stellar-structure physics, no DOM.
//
// FLOAT WIDTH: all continuous state here is Float64Array, NOT the Float32Array
// the rest of this suite uses. Density spans ~1e-6 to 1e10 g/cm^3 and pressure
// spans more; f32 carries ~7 significant digits and overflows near 3.4e38.
// Storing this state in f32 is a bug, not a style choice. Do not "fix" it.
//
// Units are cgs throughout. Every exported function states its units.

(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.LifeStar = api;
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  const laneEmdenCache = new Map();

  /**
   * Solve the Lane-Emden equation for polytrope index n.
   * Returns the dimensionless profile plus the two numbers everything
   * downstream needs: xi1 (surface) and massFactor = (-xi^2 dtheta/dxi)|xi1.
   * Memoised on n rounded to 0.05 because n moves slowly during evolution.
   */
  function laneEmden(n) {
    const key = Math.round(n / 0.05) * 0.05;
    const hit = laneEmdenCache.get(key);
    if (hit) return hit;

    const h = 1e-3;
    const xiStart = 1e-4;

    // Series start: the 2*phi/xi term is 0/0 at the origin.
    let xi = xiStart;
    let theta = 1 - xi * xi / 6 + key * Math.pow(xi, 4) / 120;
    let phi = -xi / 3 + key * Math.pow(xi, 3) / 30;

    const xis = [xi];
    const thetas = [theta];

    // theta^n, guarding against the negative base that appears one step
    // past the surface: Math.pow(negative, 1.5) is NaN.
    const thetaPow = (t) => (t > 0 ? Math.pow(t, key) : 0);

    const deriv = (x, t, p) => [p, -thetaPow(t) - 2 * p / x];

    let xi1 = null;
    let massFactor = null;

    for (let i = 0; i < 200000; i++) {
      const [k1t, k1p] = deriv(xi, theta, phi);
      const [k2t, k2p] = deriv(xi + h / 2, theta + h / 2 * k1t, phi + h / 2 * k1p);
      const [k3t, k3p] = deriv(xi + h / 2, theta + h / 2 * k2t, phi + h / 2 * k2p);
      const [k4t, k4p] = deriv(xi + h, theta + h * k3t, phi + h * k3p);

      const thetaNext = theta + h / 6 * (k1t + 2 * k2t + 2 * k3t + k4t);
      const phiNext = phi + h / 6 * (k1p + 2 * k2p + 2 * k3p + k4p);
      const xiNext = xi + h;

      if (thetaNext <= 0) {
        // Linear interpolation to the zero crossing.
        const frac = theta / (theta - thetaNext);
        xi1 = xi + frac * h;
        const phiSurf = phi + frac * (phiNext - phi);
        massFactor = -xi1 * xi1 * phiSurf;
        break;
      }

      xi = xiNext; theta = thetaNext; phi = phiNext;
      xis.push(xi); thetas.push(theta);
    }

    // n >= 5 has infinite radius; the caller clamps n well below that, but
    // fail loudly rather than returning a half-integrated profile.
    if (xi1 === null) throw new Error(`laneEmden(${n}) did not reach a surface`);

    const result = {
      n: key,
      xi1,
      massFactor,
      xi: Float64Array.from(xis),
      theta: Float64Array.from(thetas),
    };
    laneEmdenCache.set(key, result);
    return result;
  }

  return { laneEmden, G, K_B, M_U, A_RAD, SIGMA, M_SUN, R_SUN, L_SUN, YEAR };
});
```

Place the constants block from above immediately inside the factory, before `laneEmdenCache`.

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test life-star-core.test.js`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add life-star-core.js life-star-core.test.js
git commit -m "feat(life-star): Lane-Emden RK4 integrator with analytic-solution tests"
```

---

### Task 2: Equation of state and the adiabatic index

**Files:**
- Modify: `life-star-core.js`
- Test: `life-star-core.test.js`

**Interfaces:**
- Consumes: constants from Task 1.
- Produces:
  - `eos(rho, T, mu, muE) -> { P, pGas, pRad, pDeg, gamma1, n }` — `rho` in g/cm³, `T` in K, `mu` mean molecular weight, `muE` mean molecular weight per electron. `P` in dyn/cm². `gamma1` is the adiabatic index; `n = 1/(gamma1−1)` clamped to [1.0, 3.4].
  - `meanMolecularWeight(X) -> { mu, muE }` where `X` is a composition object `{ H1, He4, C12, O16, Ne20, Mg24, Si28, Fe56 }` of mass fractions.

**Background for the implementer.** Three pressure sources add:

```
P_gas = rho * K_B * T / (mu * M_U)          ideal gas
P_rad = A_RAD * T^4 / 3                     radiation
P_deg                                        degenerate electrons
```

Degenerate electron pressure has two limits — non-relativistic `P_nr = K1·(ρ/μ_e)^(5/3)` and relativistic `P_rel = K2·(ρ/μ_e)^(4/3)`. Blend them harmonically:

```
P_deg = P_nr * P_rel / (P_nr + P_rel)
```

This has the correct behaviour in both limits and crosses over near 2×10⁶ g/cm³, which is the right transition density for a white dwarf. It is a simplification of the exact parametric Fermi integral — mark it with a `ponytail:` comment naming the upgrade path.

**The polytrope index must come from the *adiabatic* exponent, not the isothermal one.** This is the single most likely thing to get wrong. `d ln P / d ln ρ` at fixed `T` is 1 for an ideal gas, which gives `n = 1/(1−1) = Infinity`. The physically meaningful quantity is `Γ₁ = (∂ ln P / ∂ ln ρ)_S`, which is 5/3 for a monatomic ideal gas and 4/3 for radiation. Compute it as a pressure-weighted mean of each component's own exponent:

```
gamma_deg = (5/3)*(1-f) + (4/3)*f       where f = P_nr / (P_nr + P_rel)
gamma1 = ( P_gas*(5/3) + P_rad*(4/3) + P_deg*gamma_deg ) / P_total
```

All four physical limits then come out exactly right: ideal gas → 5/3 → n = 1.5; radiation → 4/3 → n = 3; non-relativistic degenerate → 5/3 → n = 1.5; relativistic degenerate → 4/3 → n = 3.

Mean molecular weights for fully ionised matter:

```
1/mu  = sum over species of (mass fraction) * (1 + Z_i) / A_i
1/muE = sum over species of (mass fraction) * Z_i / A_i
```

with `(A, Z)` of `H1 (1,1)`, `He4 (4,2)`, `C12 (12,6)`, `O16 (16,8)`, `Ne20 (20,10)`, `Mg24 (24,12)`, `Si28 (28,14)`, `Fe56 (56,26)`.

- [ ] **Step 1: Write the failing test**

Append to `life-star-core.test.js`:

```js
test('eos: ideal-gas-dominated matter gives gamma1 = 5/3 and n = 1.5', () => {
  // Low density, moderate temperature: gas dominates over radiation and
  // degeneracy by many orders of magnitude.
  const r = LS.eos(1.0, 1e7, 0.6, 2.0);
  assert.ok(Math.abs(r.gamma1 - 5 / 3) < 0.01, `gamma1 was ${r.gamma1}`);
  assert.ok(Math.abs(r.n - 1.5) < 0.02, `n was ${r.n}`);
});

test('eos: radiation-dominated matter gives gamma1 = 4/3 and n = 3', () => {
  // Very low density, very high temperature.
  const r = LS.eos(1e-4, 1e9, 0.6, 2.0);
  assert.ok(r.pRad > 100 * r.pGas, 'test setup wrong: radiation should dominate');
  assert.ok(Math.abs(r.gamma1 - 4 / 3) < 0.02, `gamma1 was ${r.gamma1}`);
  assert.ok(Math.abs(r.n - 3) < 0.1, `n was ${r.n}`);
});

test('eos: cold moderately dense matter is non-relativistically degenerate', () => {
  const r = LS.eos(1e5, 1e5, 1.4, 2.0);
  assert.ok(r.pDeg > 100 * r.pGas, 'test setup wrong: degeneracy should dominate');
  assert.ok(Math.abs(r.gamma1 - 5 / 3) < 0.05, `gamma1 was ${r.gamma1}`);
});

test('eos: cold very dense matter is relativistically degenerate', () => {
  const r = LS.eos(1e9, 1e5, 1.4, 2.0);
  assert.ok(r.pDeg > 100 * r.pGas, 'test setup wrong: degeneracy should dominate');
  assert.ok(Math.abs(r.gamma1 - 4 / 3) < 0.05, `gamma1 was ${r.gamma1}`);
});

test('eos: n is clamped into [1.0, 3.4]', () => {
  for (const [rho, T] of [[1e-8, 1e9], [1e10, 1e4], [1, 1e7], [1e6, 1e8]]) {
    const r = LS.eos(rho, T, 0.6, 2.0);
    assert.ok(r.n >= 1.0 && r.n <= 3.4, `n was ${r.n} at rho=${rho} T=${T}`);
  }
});

test('meanMolecularWeight: pure ionised hydrogen gives mu = 0.5, muE = 1', () => {
  const { mu, muE } = LS.meanMolecularWeight({ H1: 1 });
  assert.ok(Math.abs(mu - 0.5) < 1e-6, `mu was ${mu}`);
  assert.ok(Math.abs(muE - 1.0) < 1e-6, `muE was ${muE}`);
});

test('meanMolecularWeight: pure ionised helium gives mu = 4/3, muE = 2', () => {
  const { mu, muE } = LS.meanMolecularWeight({ He4: 1 });
  assert.ok(Math.abs(mu - 4 / 3) < 1e-6, `mu was ${mu}`);
  assert.ok(Math.abs(muE - 2.0) < 1e-6, `muE was ${muE}`);
});

test('meanMolecularWeight: solar mix is close to 0.6', () => {
  const { mu } = LS.meanMolecularWeight({ H1: 0.70, He4: 0.28, C12: 0.02 });
  assert.ok(mu > 0.58 && mu < 0.64, `mu was ${mu}`);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test life-star-core.test.js`
Expected: FAIL — `LS.eos is not a function`

- [ ] **Step 3: Write minimal implementation**

Add inside the factory in `life-star-core.js`, before the `return`:

```js
  // (A, Z) per species. Fully ionised throughout — these stars have no
  // neutral zone worth modelling.
  const SPECIES = {
    H1:   [1, 1],
    He4:  [4, 2],
    C12:  [12, 6],
    O16:  [16, 8],
    Ne20: [20, 10],
    Mg24: [24, 12],
    Si28: [28, 14],
    Fe56: [56, 26],
  };

  /** Mean molecular weight and mean molecular weight per electron. */
  function meanMolecularWeight(X) {
    let invMu = 0, invMuE = 0;
    for (const key in SPECIES) {
      const frac = X[key] || 0;
      if (frac <= 0) continue;
      const [A, Z] = SPECIES[key];
      invMu += frac * (1 + Z) / A;
      invMuE += frac * Z / A;
    }
    return {
      mu: invMu > 0 ? 1 / invMu : 1e9,
      muE: invMuE > 0 ? 1 / invMuE : 1e9,
    };
  }

  /**
   * Total pressure and the adiabatic index at (rho, T).
   * rho g/cm^3, T K. Returns P in dyn/cm^2.
   *
   * gamma1 is the ADIABATIC exponent, not the isothermal one. The isothermal
   * d lnP / d ln rho for an ideal gas is 1, which would give n = infinity.
   */
  function eos(rho, T, mu, muE) {
    const pGas = rho * K_B * T / (mu * M_U);
    const pRad = A_RAD * T * T * T * T / 3;

    const pNr = K1 * Math.pow(rho / muE, 5 / 3);
    const pRel = K2 * Math.pow(rho / muE, 4 / 3);
    // ponytail: harmonic blend of the two degeneracy limits. Correct in both
    // limits and crosses over at the right density (~2e6 g/cm^3). Upgrade to
    // the exact parametric Fermi integral only if the white-dwarf mass-radius
    // relation is ever measured against rather than eyeballed.
    const pDeg = (pNr * pRel) / (pNr + pRel);

    const P = pGas + pRad + pDeg;

    const f = pNr / (pNr + pRel);
    const gammaDeg = (5 / 3) * (1 - f) + (4 / 3) * f;
    const gamma1 = (pGas * (5 / 3) + pRad * (4 / 3) + pDeg * gammaDeg) / P;

    let n = 1 / (gamma1 - 1);
    if (!Number.isFinite(n) || n > 3.4) n = 3.4;
    if (n < 1.0) n = 1.0;

    return { P, pGas, pRad, pDeg, gamma1, n };
  }
```

Add `eos` and `meanMolecularWeight` to the returned object.

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test life-star-core.test.js`
Expected: PASS, 13 tests.

- [ ] **Step 5: Commit**

```bash
git add life-star-core.js life-star-core.test.js
git commit -m "feat(life-star): equation of state with pressure-weighted adiabatic index"
```

---

### Task 3: Nuclear burning network (Gamow forms)

**Files:**
- Modify: `life-star-core.js`
- Test: `life-star-core.test.js`

**Interfaces:**
- Consumes: constants from Task 1.
- Produces:
  - `epsPP(rho, T, X) -> erg/g/s` — proton-proton chain. `X` is the hydrogen mass fraction (a number, not the composition object).
  - `epsCNO(rho, T, X, Z) -> erg/g/s` — CNO cycle. `Z` is metallicity.
  - `eps3a(rho, T, Y) -> erg/g/s` — triple-alpha. `Y` is the helium mass fraction.
  - `burnShell(rho, T, comp, dt) -> { energy, dComp }` — `energy` in erg/g, `dComp` the change in each mass fraction over `dt` seconds.
  - `CAL = { pp: 1.0, cno: 1.0, he: 1.0 }` — exported and mutable calibration multipliers.

**Background for the implementer.** Do **not** implement these as temperature power laws. The T⁴, T¹⁷ and T⁴⁰ figures quoted in the design spec are *local logarithmic slopes* that explain why the burning shells come out the thickness they do; they are not the formulae. `Math.pow(T/1e8, 40)` overflows to `Infinity` or flushes to zero on tiny drifts in T. Use the Gamow-peak forms, which are better conditioned and valid over a far wider range (T6 = T/10⁶, T8 = T/10⁸):

```
eps_pp  = 2.4e4  * rho     * X*X     * T6^(-2/3) * exp(-33.80  * T6^(-1/3))
eps_CNO = 4.4e25 * rho     * X * Z   * T6^(-2/3) * exp(-152.28 * T6^(-1/3))
eps_3a  = 5.1e8  * rho*rho * Y*Y*Y   * T8^(-3)   * exp(-44.027 / T8)
```

**Leave the calibration multipliers in place.** Published rate coefficients omit electron screening and are fitted over limited temperature ranges; a toy that composes them with an approximate structure model will not land on the solar luminosity by itself. `CAL.pp`, `CAL.cno` and `CAL.he` default to 1.0 and are tuned once in Task 5's integration test so that a 1 M☉ star gives roughly the solar luminosity and a roughly 10 Gyr lifetime. Recording the tuned values in a comment is part of that task.

- [ ] **Step 1: Write the failing test**

Append to `life-star-core.test.js`:

```js
// The whole "onion emerges from the physics" argument rests on the claim that
// these rates have effective temperature exponents of about 4, 17 and 40.
// Measure the exponent numerically rather than trusting the formula.
function logSlope(fn, T) {
  const d = 1e-4;
  return (Math.log(fn(T * (1 + d))) - Math.log(fn(T * (1 - d)))) / (2 * d);
}

test('pp chain has an effective temperature exponent near 4 at 15 MK', () => {
  const slope = logSlope((T) => LS.epsPP(100, T, 0.7), 1.5e7);
  assert.ok(slope > 3.4 && slope < 4.6, `slope was ${slope}`);
});

test('CNO cycle has an effective temperature exponent near 17 at 25 MK', () => {
  const slope = logSlope((T) => LS.epsCNO(100, T, 0.7, 0.02), 2.5e7);
  assert.ok(slope > 15 && slope < 19, `slope was ${slope}`);
});

test('triple-alpha has an effective temperature exponent near 40 at 100 MK', () => {
  const slope = logSlope((T) => LS.eps3a(1e4, T, 1.0), 1e8);
  assert.ok(slope > 37 && slope < 44, `slope was ${slope}`);
});

// The overflow guard. This is the assertion that fails if someone rewrites
// the rates as power laws.
test('rates stay finite and non-negative across the full temperature range', () => {
  for (const T of [1e6, 1e7, 1e8, 5e8, 1e9, 5e9]) {
    const pp = LS.epsPP(1e3, T, 0.7);
    const cno = LS.epsCNO(1e3, T, 0.7, 0.02);
    const he = LS.eps3a(1e5, T, 0.5);
    for (const [name, v] of [['pp', pp], ['cno', cno], ['3a', he]]) {
      assert.ok(Number.isFinite(v), `${name} not finite at T=${T}: ${v}`);
      assert.ok(v >= 0, `${name} negative at T=${T}: ${v}`);
    }
  }
});

test('CNO overtakes pp at high temperature, pp dominates at low', () => {
  const cool = 1.0e7, hot = 3.0e7;
  assert.ok(LS.epsPP(100, cool, 0.7) > LS.epsCNO(100, cool, 0.7, 0.02),
    'pp should dominate at 10 MK');
  assert.ok(LS.epsCNO(100, hot, 0.7, 0.02) > LS.epsPP(100, hot, 0.7),
    'CNO should dominate at 30 MK');
});

test('triple-alpha is negligible below its ignition temperature', () => {
  assert.ok(LS.eps3a(1e4, 2e7, 1.0) < 1e-20, 'helium should not burn at 20 MK');
  assert.ok(LS.eps3a(1e4, 1e8, 1.0) > 1e-3, 'helium should burn at 100 MK');
});

test('burnShell conserves total mass fraction', () => {
  const comp = { H1: 0.7, He4: 0.28, C12: 0.02 };
  const { dComp } = LS.burnShell(100, 1.5e7, comp, 1e10);
  let sum = 0;
  for (const k in dComp) sum += dComp[k];
  assert.ok(Math.abs(sum) < 1e-12, `mass fractions changed by ${sum}`);
});

test('burnShell converts hydrogen to helium and releases energy', () => {
  const comp = { H1: 0.7, He4: 0.28, C12: 0.02 };
  const { energy, dComp } = LS.burnShell(100, 1.5e7, comp, 1e12);
  assert.ok(dComp.H1 < 0, 'hydrogen should decrease');
  assert.ok(dComp.He4 > 0, 'helium should increase');
  assert.ok(energy > 0, 'energy should be released');
});

test('burnShell never burns more fuel than is present', () => {
  const comp = { H1: 1e-6, He4: 0.999999 };
  // A wildly long timestep at high temperature must not drive H1 negative.
  const { dComp } = LS.burnShell(1e3, 5e7, comp, 1e20);
  assert.ok(comp.H1 + dComp.H1 >= 0, `H1 went to ${comp.H1 + dComp.H1}`);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test life-star-core.test.js`
Expected: FAIL — `LS.epsPP is not a function`

- [ ] **Step 3: Write minimal implementation**

Add inside the factory, before the `return`:

```js
  // Calibration multipliers. Published rate coefficients omit electron
  // screening and are fitted over limited ranges; composing them with an
  // approximate structure model does not land on the solar luminosity by
  // itself. Tuned once in the Task 5 integration test.
  const CAL = { pp: 1.0, cno: 1.0, he: 1.0 };

  /**
   * Proton-proton chain, erg/g/s. Gamow-peak form, NOT a power law:
   * evaluating T^4 style expressions overflows. The effective exponent
   * near 15 MK is about 4, which is a property of this formula, not an
   * instruction to implement it as such.
   */
  function epsPP(rho, T, X) {
    if (T <= 0 || X <= 0) return 0;
    const T6 = T / 1e6;
    return CAL.pp * 2.4e4 * rho * X * X *
      Math.pow(T6, -2 / 3) * Math.exp(-33.80 * Math.pow(T6, -1 / 3));
  }

  /** CNO cycle, erg/g/s. Effective exponent near 25 MK is about 17. */
  function epsCNO(rho, T, X, Z) {
    if (T <= 0 || X <= 0 || Z <= 0) return 0;
    const T6 = T / 1e6;
    return CAL.cno * 4.4e25 * rho * X * Z *
      Math.pow(T6, -2 / 3) * Math.exp(-152.28 * Math.pow(T6, -1 / 3));
  }

  /** Triple-alpha, erg/g/s. Effective exponent near 100 MK is about 40. */
  function eps3a(rho, T, Y) {
    if (T <= 0 || Y <= 0) return 0;
    const T8 = T / 1e8;
    return CAL.he * 5.1e8 * rho * rho * Y * Y * Y *
      Math.pow(T8, -3) * Math.exp(-44.027 / T8);
  }

  /**
   * Burn one shell for dt seconds.
   * Returns energy in erg/g and the change in each mass fraction.
   * Fuel consumption is capped at what is actually present, so an
   * over-long timestep degrades to "burned everything" rather than
   * driving a mass fraction negative.
   */
  function burnShell(rho, T, comp, dt) {
    const X = comp.H1 || 0;
    const Y = comp.He4 || 0;
    // Metallicity: everything that is not hydrogen or helium.
    let Z = 0;
    for (const k in SPECIES) {
      if (k !== 'H1' && k !== 'He4') Z += comp[k] || 0;
    }

    const eH = epsPP(rho, T, X) + epsCNO(rho, T, X, Z);
    const eHe = eps3a(rho, T, Y);

    let dH = Math.min(eH * dt / Q_H, X);       // grams of H per gram, capped
    let dHe = Math.min(eHe * dt / Q_HE, Y);    // grams of He per gram, capped

    const energy = dH * Q_H + dHe * Q_HE;

    const dComp = {
      H1: -dH,
      He4: dH - dHe,
      C12: dHe,
    };
    return { energy, dComp };
  }
```

Add `epsPP`, `epsCNO`, `eps3a`, `burnShell` and `CAL` to the returned object.

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test life-star-core.test.js`
Expected: PASS, 22 tests.

- [ ] **Step 5: Commit**

```bash
git add life-star-core.js life-star-core.test.js
git commit -m "feat(life-star): Gamow-form burning network with measured-exponent tests"
```

---

### Task 4: Equilibrium structure solver

**Files:**
- Modify: `life-star-core.js`
- Test: `life-star-core.test.js`

**Interfaces:**
- Consumes: `laneEmden`, `eos`, `meanMolecularWeight`, `epsPP`, `epsCNO`, `eps3a` from Tasks 1–3.
- Produces:
  - `structure(M, comp, shells) -> { R, rhoC, tC, n, L, tEff, rho: Float64Array, T: Float64Array, m: Float64Array }` — `M` in grams, `comp` a composition object, `shells` the shell count (default 200). `R` cm, `L` erg/s, `tEff` K. The three arrays are the radial profile sampled on the Lagrangian mass grid.

**Background for the implementer.** A polytrope does not by itself fix a star's radius — you need a thermal condition. This model uses two standard results, and neither is scripting: they are the analytic consequences of radiative diffusion and the virial theorem.

**Central pressure from mass and radius.** For a polytrope of index n:

```
P_c = W_n * G * M^2 / R^4        where W_n = 1 / (4*pi*(n+1)*thetaPrime1^2)
rho_c = rhoMean * xi1^3 / (3 * massFactor)
```

with `thetaPrime1 = -massFactor / xi1^2` and `rhoMean = M / (4/3 pi R^3)`.

**The luminosity the star must radiate.** The homology solution of radiative diffusion with electron-scattering opacity gives `L ∝ μ⁴M³`. Anchored at the Sun:

```
L_target = L_SUN * (M/M_SUN)^3 * (mu/0.6)^4
```

This is a `ponytail:` simplification with a real ceiling — no opacity table, no radiative transfer — and it should be commented as such. It nonetheless reproduces the observed mass-luminosity relation, which is the point.

**Then R is whatever makes the star burn what it radiates.** Bisect on `log R` until the integrated nuclear luminosity equals `L_target`. Nuclear rates are so temperature-sensitive that this converges fast and is very stiff — which is exactly why real stars self-regulate on the main sequence.

Inner loop for a trial R: compute `rho_c` and `P_c` from the relations above, invert the EOS for `T_c` (bisect on `log T` since `eos` is monotonic in T), recompute `n` from `eos`, and repeat two or three times because `n` feeds back into `W_n`. Then sample the Lane-Emden profile onto the mass grid and sum `eps * dm`.

For temperature through the star, use the polytropic relation `T/T_c = θ` where the gas is ideal — correct for a polytrope with constant composition and good enough here.

- [ ] **Step 1: Write the failing test**

Append to `life-star-core.test.js`:

```js
const SOLAR = { H1: 0.70, He4: 0.28, C12: 0.02 };

test('structure: a 1 solar mass star has roughly solar radius', () => {
  const s = LS.structure(LS.M_SUN, SOLAR);
  const ratio = s.R / LS.R_SUN;
  assert.ok(ratio > 0.4 && ratio < 2.5, `R/R_sun was ${ratio}`);
});

test('structure: a 1 solar mass star has a central temperature near 15 MK', () => {
  const s = LS.structure(LS.M_SUN, SOLAR);
  assert.ok(s.tC > 8e6 && s.tC < 3e7, `T_c was ${s.tC}`);
});

test('structure: a 1 solar mass star has a central density near 150 g/cm3', () => {
  const s = LS.structure(LS.M_SUN, SOLAR);
  assert.ok(s.rhoC > 30 && s.rhoC < 600, `rho_c was ${s.rhoC}`);
});

test('structure: luminosity follows roughly L ~ M^3.5 on the main sequence', () => {
  const a = LS.structure(1 * LS.M_SUN, SOLAR);
  const b = LS.structure(4 * LS.M_SUN, SOLAR);
  const slope = Math.log(b.L / a.L) / Math.log(4);
  assert.ok(slope > 2.5 && slope < 4.5, `mass-luminosity slope was ${slope}`);
});

test('structure: more massive stars are bigger and hotter', () => {
  const a = LS.structure(1 * LS.M_SUN, SOLAR);
  const b = LS.structure(10 * LS.M_SUN, SOLAR);
  assert.ok(b.R > a.R, 'a 10 solar mass star should be larger');
  assert.ok(b.tC > a.tC, 'a 10 solar mass star should be hotter at the centre');
  assert.ok(b.tEff > a.tEff, 'a 10 solar mass star should have a hotter surface');
});

test('structure: profiles are finite, ordered and centrally condensed', () => {
  const s = LS.structure(LS.M_SUN, SOLAR, 200);
  assert.strictEqual(s.rho.length, 200);
  assert.ok(s.rho instanceof Float64Array, 'profiles must be Float64Array');
  for (let i = 0; i < s.rho.length; i++) {
    assert.ok(Number.isFinite(s.rho[i]) && s.rho[i] >= 0, `rho[${i}] = ${s.rho[i]}`);
    assert.ok(Number.isFinite(s.T[i]) && s.T[i] >= 0, `T[${i}] = ${s.T[i]}`);
  }
  assert.ok(s.rho[0] > s.rho[s.rho.length - 1], 'density should fall outward');
  assert.ok(s.T[0] > s.T[s.T.length - 1], 'temperature should fall outward');
});

test('structure: effective temperature satisfies the Stefan-Boltzmann relation', () => {
  const s = LS.structure(LS.M_SUN, SOLAR);
  const check = 4 * Math.PI * s.R * s.R * LS.SIGMA * Math.pow(s.tEff, 4);
  assert.ok(Math.abs(check - s.L) / s.L < 1e-6, 'L and tEff are inconsistent');
});

test('structure: raising mean molecular weight contracts and heats the star', () => {
  // Hydrogen partly burned to helium: mu rises, so the star must contract.
  const young = LS.structure(LS.M_SUN, { H1: 0.70, He4: 0.28, C12: 0.02 });
  const old = LS.structure(LS.M_SUN, { H1: 0.35, He4: 0.63, C12: 0.02 });
  assert.ok(old.tC > young.tC, 'central temperature should rise as mu rises');
});

test('structure: determinism — identical input gives identical output', () => {
  const a = LS.structure(2 * LS.M_SUN, SOLAR);
  const b = LS.structure(2 * LS.M_SUN, SOLAR);
  assert.strictEqual(a.R, b.R);
  assert.strictEqual(a.tC, b.tC);
  assert.strictEqual(a.L, b.L);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test life-star-core.test.js`
Expected: FAIL — `LS.structure is not a function`

- [ ] **Step 3: Write minimal implementation**

Add inside the factory, before the `return`:

```js
  /** Invert the EOS for temperature at a given density and pressure. */
  function temperatureFor(rho, P, mu, muE) {
    let lo = Math.log(1e3), hi = Math.log(1e11);
    // eos is monotonically increasing in T at fixed rho.
    for (let i = 0; i < 80; i++) {
      const mid = 0.5 * (lo + hi);
      if (eos(rho, Math.exp(mid), mu, muE).P < P) lo = mid; else hi = mid;
    }
    return Math.exp(0.5 * (lo + hi));
  }

  /** Central conditions and polytrope index for a trial radius. */
  function centralState(M, R, mu, muE) {
    let n = 3;
    let rhoC = 0, tC = 0, le = null;
    for (let iter = 0; iter < 4; iter++) {
      le = laneEmden(n);
      const thetaPrime1 = -le.massFactor / (le.xi1 * le.xi1);
      const Wn = 1 / (4 * Math.PI * (n + 1) * thetaPrime1 * thetaPrime1);
      const rhoMean = M / ((4 / 3) * Math.PI * R * R * R);
      rhoC = rhoMean * Math.pow(le.xi1, 3) / (3 * le.massFactor);
      const pC = Wn * G * M * M / Math.pow(R, 4);
      tC = temperatureFor(rhoC, pC, mu, muE);
      n = eos(rhoC, tC, mu, muE).n;
    }
    return { n, rhoC, tC, le };
  }

  /**
   * Solve for the equilibrium structure of a star of mass M and composition
   * comp. Radius is whatever makes nuclear output match the luminosity the
   * star must radiate.
   *
   * ponytail: L_target uses the homology result L ~ mu^4 M^3 from radiative
   * diffusion with electron-scattering opacity, anchored at the Sun. No
   * opacity table, no radiative transfer. Upgrade path is a real Kramers
   * opacity and a solved transport equation, which is a different project.
   */
  function structure(M, comp, shells) {
    shells = shells || 200;
    const { mu, muE } = meanMolecularWeight(comp);
    const X = comp.H1 || 0;
    const Y = comp.He4 || 0;
    let Z = 0;
    for (const k in SPECIES) if (k !== 'H1' && k !== 'He4') Z += comp[k] || 0;

    const lTarget = L_SUN * Math.pow(M / M_SUN, 3) * Math.pow(mu / 0.6, 4);

    // Nuclear luminosity for a trial radius.
    const nuclear = (R) => {
      const { n, rhoC, tC, le } = centralState(M, R, mu, muE);
      let total = 0;
      const dm = M / shells;
      for (let i = 0; i < shells; i++) {
        const mFrac = (i + 0.5) / shells;
        const theta = thetaAtMassFraction(le, mFrac);
        const rho = rhoC * Math.pow(Math.max(theta, 0), n);
        const T = tC * Math.max(theta, 0);
        total += (epsPP(rho, T, X) + epsCNO(rho, T, X, Z) + eps3a(rho, T, Y)) * dm;
      }
      return { total, n, rhoC, tC, le };
    };

    // Bisect on log R. Smaller R means hotter centre means far more burning,
    // so nuclear output is steeply decreasing in R.
    let lo = Math.log(1e8), hi = Math.log(1e14);
    for (let i = 0; i < 60; i++) {
      const mid = 0.5 * (lo + hi);
      if (nuclear(Math.exp(mid)).total > lTarget) lo = mid; else hi = mid;
    }
    const R = Math.exp(0.5 * (lo + hi));
    const { n, rhoC, tC, le } = nuclear(R);

    const rho = new Float64Array(shells);
    const T = new Float64Array(shells);
    const m = new Float64Array(shells);
    for (let i = 0; i < shells; i++) {
      const mFrac = (i + 0.5) / shells;
      const theta = Math.max(thetaAtMassFraction(le, mFrac), 0);
      rho[i] = rhoC * Math.pow(theta, n);
      T[i] = tC * theta;
      m[i] = mFrac * M;
    }

    const L = lTarget;
    const tEff = Math.pow(L / (4 * Math.PI * R * R * SIGMA), 0.25);
    return { R, rhoC, tC, n, L, tEff, rho, T, m };
  }
```

Add this helper, which maps enclosed-mass fraction to `θ` by walking the Lane-Emden profile and accumulating mass:

```js
  /**
   * theta at a given enclosed-mass fraction. The Lane-Emden solution is
   * sampled on xi, but the composition grid is Lagrangian, so this converts
   * between them. Cached per profile because the mapping never changes.
   */
  function thetaAtMassFraction(le, mFrac) {
    if (!le._massTable) {
      const N = le.xi.length;
      const cum = new Float64Array(N);
      let acc = 0;
      for (let i = 1; i < N; i++) {
        const xi = le.xi[i], dxi = le.xi[i] - le.xi[i - 1];
        const th = Math.max(le.theta[i], 0);
        acc += xi * xi * Math.pow(th, le.n) * dxi;
        cum[i] = acc;
      }
      for (let i = 0; i < N; i++) cum[i] /= acc || 1;
      le._massTable = cum;
    }
    const cum = le._massTable;
    // Binary search for the first index whose cumulative mass exceeds mFrac.
    let lo = 0, hi = cum.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (cum[mid] < mFrac) lo = mid + 1; else hi = mid;
    }
    return le.theta[lo];
  }
```

Add `structure` to the returned object.

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test life-star-core.test.js`
Expected: PASS, 31 tests.

If the solar radius or central temperature tests fail by more than the (deliberately generous) bracket, adjust `CAL.pp` — that is what it is for. Record the value you settled on in a comment next to `CAL`.

- [ ] **Step 5: Commit**

```bash
git add life-star-core.js life-star-core.test.js
git commit -m "feat(life-star): equilibrium structure solver anchored on solar values"
```

---

### Task 5: Time evolution and adaptive timestep

**Files:**
- Modify: `life-star-core.js`
- Test: `life-star-core.test.js`

**Interfaces:**
- Consumes: everything from Tasks 1–4.
- Produces:
  - `createStar(massSolar, opts) -> state` — `opts = { shells = 200, metallicity = 0.02 }`. `state` holds `{ M, age, comp: Array<compObject>, shells, struct, phase, alive }` where `comp[i]` is the composition of shell `i`.
  - `step(state) -> state` — advances one adaptive timestep in place and returns the same object. Sets `state.dt` to the seconds advanced.
  - `PHASES = { PRE: 'pre-main-sequence', MS: 'main sequence', POST_H: 'hydrogen exhausted' }`.

**Background for the implementer.** The timestep comes from the nuclear timescale — how long the available fuel would last at the current burn rate — scaled down by a safety factor:

```
dt = SAFETY * min over shells of (fuel energy remaining / burn rate)
```

**Cap `dt` by the fastest-burning shell, not by a global average.** A shell burning silicon lasts about a day while the star's average timescale is millions of years; averaging steps straight over it. This is the single most likely correctness bug in the task.

Also clamp `dt` to an absolute ceiling (say 10⁸ years) so a nearly-inert star cannot take a step longer than the age of the universe, and to a floor (1 second) so a numerical spike cannot stall the simulation completely.

Composition is per shell, so after burning, the shells no longer share a composition. `structure()` takes a single composition object; pass it the **mass-weighted mean** composition. That is the approximation that makes a single polytrope tractable, and it is precisely what Task 6 of slice 2 replaces with a two-zone model.

- [ ] **Step 1: Write the failing test**

Append to `life-star-core.test.js`:

```js
test('createStar builds a star with the requested mass and shell count', () => {
  const s = LS.createStar(1, { shells: 50 });
  assert.ok(Math.abs(s.M - LS.M_SUN) / LS.M_SUN < 1e-9);
  assert.strictEqual(s.comp.length, 50);
  assert.ok(Math.abs(s.comp[0].H1 - 0.70) < 0.05, 'should start hydrogen-rich');
  assert.strictEqual(s.age, 0);
});

test('step advances age by a positive, finite amount', () => {
  const s = LS.createStar(1, { shells: 50 });
  LS.step(s);
  assert.ok(Number.isFinite(s.dt) && s.dt > 0, `dt was ${s.dt}`);
  assert.ok(s.age > 0);
});

test('step burns hydrogen fastest in the centre, producing a gradient', () => {
  const s = LS.createStar(1, { shells: 50 });
  for (let i = 0; i < 200; i++) LS.step(s);
  assert.ok(s.comp[0].H1 < s.comp[49].H1,
    'the core should be more depleted than the surface');
});

test('step never drives a mass fraction negative or above one', () => {
  const s = LS.createStar(25, { shells: 50 });
  for (let i = 0; i < 500; i++) LS.step(s);
  for (const shell of s.comp) {
    for (const k in shell) {
      assert.ok(shell[k] >= -1e-12 && shell[k] <= 1 + 1e-12,
        `${k} out of range: ${shell[k]}`);
    }
  }
});

test('dt is capped by the fastest-burning shell, not a global average', () => {
  const s = LS.createStar(25, { shells: 50 });
  LS.step(s);
  // A 25 solar mass star lives ~7 Myr. No single step may exceed a
  // meaningful fraction of that.
  assert.ok(s.dt / LS.YEAR < 1e6, `dt was ${s.dt / LS.YEAR} yr, far too coarse`);
});

// The headline calibration checks. Both brackets are a factor of ~2, which
// is the honest accuracy of this model.
test('a 1 solar mass star lives roughly 10 Gyr on the main sequence', () => {
  const s = LS.createStar(1, { shells: 100 });
  let guard = 0;
  while (s.comp[0].H1 > 0.01 && guard++ < 200000) LS.step(s);
  const gyr = s.age / LS.YEAR / 1e9;
  assert.ok(gyr > 4 && gyr < 25, `main-sequence lifetime was ${gyr} Gyr`);
});

test('a 25 solar mass star lives far shorter than a 1 solar mass star', () => {
  const big = LS.createStar(25, { shells: 100 });
  const small = LS.createStar(1, { shells: 100 });
  let guard = 0;
  while (big.comp[0].H1 > 0.01 && guard++ < 200000) LS.step(big);
  guard = 0;
  while (small.comp[0].H1 > 0.01 && guard++ < 200000) LS.step(small);
  const ratio = small.age / big.age;
  assert.ok(ratio > 100, `lifetime ratio was only ${ratio}`);
});

test('evolution is deterministic', () => {
  const a = LS.createStar(2, { shells: 50 });
  const b = LS.createStar(2, { shells: 50 });
  for (let i = 0; i < 100; i++) { LS.step(a); LS.step(b); }
  assert.strictEqual(a.age, b.age);
  assert.strictEqual(a.comp[0].H1, b.comp[0].H1);
  assert.strictEqual(a.struct.L, b.struct.L);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test life-star-core.test.js`
Expected: FAIL — `LS.createStar is not a function`

- [ ] **Step 3: Write minimal implementation**

Add inside the factory, before the `return`:

```js
  const PHASES = {
    PRE: 'pre-main-sequence',
    MS: 'main sequence',
    POST_H: 'hydrogen exhausted',
  };

  const DT_SAFETY = 0.02;
  const DT_MAX = 1e8 * YEAR;
  const DT_MIN = 1.0;

  /** Mass-weighted mean composition across all shells. */
  function meanComposition(comp) {
    const out = {};
    for (const key in SPECIES) {
      let sum = 0;
      for (let i = 0; i < comp.length; i++) sum += comp[i][key] || 0;
      out[key] = sum / comp.length;
    }
    return out;
  }

  function createStar(massSolar, opts) {
    opts = opts || {};
    const shells = opts.shells || 200;
    const Z = opts.metallicity === undefined ? 0.02 : opts.metallicity;
    const comp = [];
    for (let i = 0; i < shells; i++) {
      comp.push({ H1: 0.70, He4: 0.30 - Z, C12: Z });
    }
    const state = {
      M: massSolar * M_SUN,
      massSolar,
      age: 0,
      dt: 0,
      shells,
      comp,
      phase: PHASES.MS,
      alive: true,
      struct: null,
    };
    state.struct = structure(state.M, meanComposition(comp), shells);
    return state;
  }

  function step(state) {
    const s = state.struct;
    const dm = state.M / state.shells;

    // Timescale of the FASTEST-burning shell, never a global average:
    // a shell burning on a timescale of days must not be stepped over by
    // a step sized for the star's million-year average.
    let dt = DT_MAX;
    for (let i = 0; i < state.shells; i++) {
      const c = state.comp[i];
      const X = c.H1 || 0, Y = c.He4 || 0;
      let Z = 0;
      for (const k in SPECIES) if (k !== 'H1' && k !== 'He4') Z += c[k] || 0;
      const rate = epsPP(s.rho[i], s.T[i], X) +
                   epsCNO(s.rho[i], s.T[i], X, Z) +
                   eps3a(s.rho[i], s.T[i], Y);
      if (rate <= 0) continue;
      const fuel = X * Q_H + Y * Q_HE;   // erg/g still available
      const tau = fuel / rate;
      if (tau < dt) dt = tau;
    }
    dt = Math.max(DT_MIN, Math.min(DT_MAX, dt * DT_SAFETY));

    for (let i = 0; i < state.shells; i++) {
      const { dComp } = burnShell(s.rho[i], s.T[i], state.comp[i], dt);
      for (const k in dComp) {
        state.comp[i][k] = (state.comp[i][k] || 0) + dComp[k];
        if (state.comp[i][k] < 0) state.comp[i][k] = 0;
      }
    }

    state.age += dt;
    state.dt = dt;
    state.struct = structure(state.M, meanComposition(state.comp), state.shells);
    state.phase = state.comp[0].H1 > 0.01 ? PHASES.MS : PHASES.POST_H;
    return state;
  }
```

Add `createStar`, `step`, `PHASES` and `meanComposition` to the returned object.

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test life-star-core.test.js`
Expected: PASS, 39 tests.

The lifetime test is the calibration gate. If a 1 M☉ star comes out at 40 Gyr, lower `CAL.pp`; if 1 Gyr, raise it. Record the final value in the comment beside `CAL` along with the lifetime it produces.

- [ ] **Step 5: Commit**

```bash
git add life-star-core.js life-star-core.test.js
git commit -m "feat(life-star): adaptive-timestep evolution capped by the fastest shell"
```

---

### Task 6: Browser shell — cross-section panel

**Files:**
- Create: `life-star.html`
- Modify: `index.html`

**Interfaces:**
- Consumes: `LifeStar.createStar`, `LifeStar.step`, `LifeStar.M_SUN`, `LifeStar.R_SUN`, `LifeStar.L_SUN`, `LifeStar.YEAR`.
- Produces: a working page with one star. The HR panel and the family arrive in Task 7.

**Background for the implementer.** 2D canvas only — no Three.js, no CDN, so the page works offline. Copy the dark visual skin from `life-conveyor.html`, which is the suite's other canvas-only page.

**`:root { color-scheme: dark }` is mandatory.** Without it, Chrome's auto-dark heuristic decides an already-dark page needs darkening and rewrites the whole palette. It is invisible locally with dark mode off and very visible to anyone you send the URL to.

**Radius must be drawn on a log scale.** A star spans a factor of ~100 in radius between the main sequence and the giant branch, and ~10⁴ more down to a white dwarf. Linear radius makes every interesting change invisible. Map `r_draw = maxPixels * log(1 + r/R_SUN) / log(1 + rMax/R_SUN)`.

Colour rings by the dominant species in each shell. Suggested palette, consistent with the suite's warm-dark convention:

```js
const SPECIES_COLOUR = {
  H1:   '#4a9ee0',   // blue
  He4:  '#e0c34a',   // yellow
  C12:  '#8a8a8a',   // grey
  O16:  '#5ad0a0',   // green
  Ne20: '#c07ae0',   // violet
  Mg24: '#e08a4a',   // orange
  Si28: '#e0e0d0',   // pale
  Fe56: '#c04a4a',   // red
};
```

- [ ] **Step 1: Create the page skeleton with the cross-section canvas**

Create `life-star.html`. Structure it as: a `<style>` block opening with `:root { color-scheme: dark }`, a header, a `<canvas id="crossSection" width="480" height="480">`, a controls row (mass select, play/pause, a log-scale "years per second" range input), a readouts panel, and a `<script>` block that inlines `life-star-core.js` via `<script src="life-star-core.js"></script>` followed by the page logic.

Readouts to show, all of which come straight off `state` and `state.struct`: mass in M☉, radius in R☉, central temperature in MK, central density in g/cm³, luminosity in L☉, effective temperature in K, age in years (formatted with unit suffixes), current phase, and the current `years per second` figure.

- [ ] **Step 2: Wire the render loop**

```js
let star = LifeStar.createStar(1, { shells: 200 });
let yearsPerSecond = 1e7;
let running = true;
let lastFrame = performance.now();

function drawCrossSection(ctx, state) {
  const W = ctx.canvas.width, H = ctx.canvas.height;
  ctx.fillStyle = '#0d0a06';
  ctx.fillRect(0, 0, W, H);

  const cx = W / 2, cy = H / 2, maxPx = Math.min(W, H) / 2 - 10;
  const s = state.struct;
  // Log radius: the star spans orders of magnitude across its life, and
  // a linear axis makes every interesting change invisible.
  const rMaxSolar = Math.max(s.R / LifeStar.R_SUN, 1) * 1.2;
  const scale = (rSolar) =>
    maxPx * Math.log1p(rSolar) / Math.log1p(rMaxSolar);

  // Draw outermost shell first so inner shells paint on top.
  for (let i = state.shells - 1; i >= 0; i--) {
    const rSolar = (s.R / LifeStar.R_SUN) * Math.cbrt((i + 1) / state.shells);
    ctx.beginPath();
    ctx.arc(cx, cy, scale(rSolar), 0, Math.PI * 2);
    ctx.fillStyle = dominantColour(state.comp[i]);
    ctx.fill();
  }
}

function dominantColour(comp) {
  let best = 'H1', bestVal = -1;
  for (const k in comp) {
    if (comp[k] > bestVal) { bestVal = comp[k]; best = k; }
  }
  return SPECIES_COLOUR[best] || '#888';
}

function frame(now) {
  const elapsedSeconds = (now - lastFrame) / 1000;
  lastFrame = now;
  if (running) {
    const targetYears = yearsPerSecond * elapsedSeconds;
    let advanced = 0;
    // Step until this frame's worth of simulated time is used up, with a
    // hard iteration cap so a tiny dt cannot freeze the browser.
    for (let guard = 0; advanced < targetYears * LifeStar.YEAR && guard < 500; guard++) {
      LifeStar.step(star);
      advanced += star.dt;
    }
  }
  drawCrossSection(ctx, star);
  updateReadouts(star);
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
```

- [ ] **Step 3: Verify in the browser**

Open `life-star.html` directly via `file://`. Expected: a blue disc that slowly develops a yellow (helium) core as hydrogen burns, with readouts updating. Central temperature should read roughly 15 MK for the 1 M☉ default.

Check the auto-dark guard: with Chrome's "Auto Dark Mode for Web Contents" flag enabled, the palette must be unchanged.

- [ ] **Step 4: Add the card to `index.html`**

Copy the structure of an existing card. Without this the page is unreachable from the deployed site — the same omission would make the whole slice invisible.

- [ ] **Step 5: Commit**

```bash
node --test
git add life-star.html index.html
git commit -m "feat(life-star): cross-section panel on a log radius axis"
```

---

### Task 7: HR diagram and the mass family

**Files:**
- Modify: `life-star.html`

**Interfaces:**
- Consumes: `LifeStar.createStar`, `LifeStar.step`; the render loop from Task 6.
- Produces: the finished slice-1 page.

**Background for the implementer.** The core stays strictly single-star. The shell holds an array of independent states and steps each one — no shared mutable state, no core API change. That separation is the entire reason this lands in slice 1 rather than as a retrofit.

**The stars share one real physical clock, not a normalised one.** A 25 M☉ star lives about 7 Myr and a 1 M☉ star about 10 Gyr — a factor of 1400. Normalising so all four finish together would look tidier and would delete the lesson, which is that massive stars live fast and die young. Massive stars therefore finish early and sit inert while the small ones are still burning hydrogen. That is what actually happens.

**Which is why the time axis is logarithmic.** On a linear axis, the 25 M☉ star's entire life is the first 0.07% of the run and effectively invisible. On a log axis, 7 Myr and 10 Gyr are three decades apart and both readable. The log axis is what lets the honest choice of clock survive contact with a screen.

**HR diagram conventions matter** — an observer recognises the shape only if it is drawn the usual way. Temperature increases to the **left** (reversed x axis), both axes are logarithmic, luminosity is in L☉ from about 10⁻⁴ to 10⁶, and effective temperature runs from about 40000 K down to 2000 K.

- [ ] **Step 1: Replace the single star with a family**

```js
const FAMILY = [0.5, 1, 8, 25];
const TRACK_COLOUR = ['#5ad0a0', '#4a9ee0', '#e0c34a', '#e0554a'];

let stars = FAMILY.map((m) => LifeStar.createStar(m, { shells: 200 }));
let tracks = FAMILY.map(() => []);      // [{ tEff, L }] history per star
let selected = 1;                        // index shown in the cross-section
let simYears = 0;                        // the shared physical clock
```

- [ ] **Step 2: Step every star on the shared clock**

Replace the stepping block in `frame()`:

```js
if (running) {
  const targetSeconds = yearsPerSecond * elapsedSeconds * LifeStar.YEAR;
  simYears += targetSeconds / LifeStar.YEAR;
  stars.forEach((star, i) => {
    if (!star.alive) return;
    let advanced = 0;
    for (let guard = 0; advanced < targetSeconds && guard < 500; guard++) {
      // Each star advances to the SAME physical time, not the same
      // fraction of its own life. Massive stars therefore finish first
      // and then sit inert, which is the point.
      if (star.age >= simYears * LifeStar.YEAR) break;
      LifeStar.step(star);
      advanced += star.dt;
    }
    const s = star.struct;
    const last = tracks[i][tracks[i].length - 1];
    // Only record a track point when it has moved perceptibly, or the
    // array grows without bound over 10^4 steps.
    if (!last || Math.abs(Math.log(s.L / last.L)) > 0.01 ||
                 Math.abs(Math.log(s.tEff / last.tEff)) > 0.005) {
      tracks[i].push({ tEff: s.tEff, L: s.L });
    }
  });
}
```

- [ ] **Step 3: Draw the HR diagram**

```js
function drawHR(ctx, stars, tracks) {
  const W = ctx.canvas.width, H = ctx.canvas.height;
  ctx.fillStyle = '#0d0a06';
  ctx.fillRect(0, 0, W, H);

  // Conventional HR axes: temperature increases LEFTWARD, both log.
  const T_HOT = 40000, T_COOL = 2000;
  const L_LO = 1e-4, L_HI = 1e6;
  const px = (tEff) => W * (Math.log10(T_HOT) - Math.log10(tEff)) /
                            (Math.log10(T_HOT) - Math.log10(T_COOL));
  const py = (L) => H * (Math.log10(L_HI) - Math.log10(L / LifeStar.L_SUN)) /
                        (Math.log10(L_HI) - Math.log10(L_LO));

  drawHRGrid(ctx, px, py, T_HOT, T_COOL, L_LO, L_HI);

  tracks.forEach((track, i) => {
    if (track.length < 2) return;
    ctx.strokeStyle = TRACK_COLOUR[i];
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(px(track[0].tEff), py(track[0].L));
    for (let k = 1; k < track.length; k++) {
      ctx.lineTo(px(track[k].tEff), py(track[k].L));
    }
    ctx.stroke();
  });

  // Current position as a filled dot, ringed if this star is selected.
  stars.forEach((star, i) => {
    const s = star.struct;
    const x = px(s.tEff), y = py(s.L);
    ctx.fillStyle = TRACK_COLOUR[i];
    ctx.beginPath();
    ctx.arc(x, y, i === selected ? 6 : 4, 0, Math.PI * 2);
    ctx.fill();
    if (i === selected) {
      ctx.strokeStyle = '#fff';
      ctx.lineWidth = 1.5;
      ctx.stroke();
    }
  });
}
```

Write `drawHRGrid` to draw decade gridlines and axis labels — temperature ticks at 40000, 20000, 10000, 5000, 3000 K and luminosity ticks at each decade from 10⁻⁴ to 10⁶ L☉.

- [ ] **Step 4: Add the family selector and the boring-main-sequence readouts**

A row of four clickable swatches, coloured by `TRACK_COLOUR` and labelled with the mass, sets `selected`. The cross-section then draws `stars[selected]`.

Add these three readouts, which exist specifically so a static main sequence reads as *deliberately* static rather than hung:

- **Elapsed:** `simYears` formatted with unit suffixes (kyr, Myr, Gyr).
- **Rate:** the current `years per second`.
- **Per star:** age and phase for each of the four, so the observer can see the 25 M☉ star finish while the 0.5 M☉ star has barely started.

- [ ] **Step 5: Verify in the browser**

Open via `file://`. Expected: four dots sitting on a diagonal band — that band *is* the main sequence, and seeing it appear unprompted is the moment the physics validates itself. As the clock runs, the 25 M☉ dot exhausts its hydrogen and stops while the 0.5 M☉ dot has visibly not moved.

Sanity check: the 1 M☉ star should sit near 5800 K and 1 L☉. If it does not, the calibration from Task 5 needs revisiting.

- [ ] **Step 6: Commit**

```bash
node --test
git add life-star.html
git commit -m "feat(life-star): HR diagram and the four-mass family on one log clock"
```

---

## Self-Review

**Spec coverage.** Lane-Emden (Task 1), EOS and adiabatic index with the explicit `n = 1/(Γ₁−1)` clamp to [1.0, 3.4] (Task 2), Gamow-form rates with measured exponents and the overflow guard (Task 3), the structure solve and mass-luminosity relation (Task 4), adaptive `dt` capped by the fastest shell (Task 5), log-radius cross-section (Task 6), HR panel and mass family on a shared real clock with a log axis (Task 7), `Float64Array` and determinism constraints (Global Constraints, asserted in Tasks 4–5), `index.html` card (Task 6), `color-scheme: dark` (Task 6).

Deliberately **not** in this slice, and tracked below: composite polytrope and the red giant branch, advanced burning beyond helium, endpoint classification, the Chandrasekhar test, lithium and the pre-main-sequence, and the Help dialog. Slice 1 delivers stars that ignite, burn hydrogen and stop.

**Placeholder scan.** No TBD or TODO. Every code step carries runnable code. `drawHRGrid` is specified by its tick values rather than written out — that is the one function left to the implementer, and it is pure axis decoration with the exact values given.

**Type consistency.** `comp` is an object of mass fractions everywhere; `state.comp` is an array of them, one per shell. `structure()` takes the singular form, which is why `meanComposition()` exists. `massFactor` means `(−ξ²θ′)|ξ1` in every task. `structure()` returns `tC`/`tEff` (capital T reserved for the profile array `T`), and Tasks 5–7 use those names unchanged.

---

## Notes for the executor

**Run `node --test` before every commit.** CI runs Rust only; nothing else will catch a broken core.

**The calibration constants are meant to be tuned.** `CAL.pp`, `CAL.cno` and `CAL.he` exist because published rate coefficients omit electron screening and are fitted over limited ranges. Tuning them so a 1 M☉ star lands near solar luminosity and a ~10 Gyr lifetime is doing the task correctly, not fudging it. Record the values you settle on.

**If the structure solver will not converge,** check the bisection bracket in `structure()` first — `1e8` to `1e14` cm covers white dwarfs through supergiants, but a star with almost no fuel left has no solution at all, and slice 1 has no handling for that. Stopping at hydrogen exhaustion is the intended slice boundary.

---

## Slices 2 and 3 (outline — separate plan files)

**Slice 2 — giants and the onion** (`2026-08-14-life-star-slice2.md`)

A single polytrope cannot become a red giant: as μ rises the star contracts and heats, and never swells. The fix is a **composite two-zone polytrope** — an inert helium core matched to a hydrogen-burning envelope at a common pressure and mass coordinate. The classic result this produces is the **Schönberg-Chandrasekhar limit**: an isothermal core can support the envelope only up to about 9% of the star's mass (`q_SC ≈ 0.37·(μ_env/μ_core)²`), past which it collapses and the envelope expands. That gives the giant branch as a *derived* result with a known number to test against, exactly as the Chandrasekhar mass is in slice 3.

Then advanced burning: carbon, neon, oxygen and silicon as threshold-ignited stages with binding-energy release. Ignition temperatures approximately 600 MK, 1.2 GK, 1.5 GK and 2.7 GK. The visible onion appears here, and the shell thicknesses are the check that the rates are right.

Also in this slice: lithium and the pre-main-sequence contraction, which is where Li7 visibly disappears before ignition.

**Slice 3 — death and the Help dialog** (`2026-08-14-life-star-slice3.md`)

`classifyEndpoint`, the terminal animations, and the **load-bearing Chandrasekhar test**: a cold relativistic degenerate configuration must give a limiting mass of 1.44 ± 0.05 M☉, which fails if either the Lane-Emden integration or the degenerate EOS is wrong. White dwarfs need the structure solver's alternate branch, since a star with no nuclear burning gets its radius from the degenerate mass-radius relation rather than from an energy balance.

Then the Help dialog, whose contents are already enumerated in the design spec — the red giant, the helium flash, low-mass stars never reaching iron, the vanishing lithium, the deliberately frozen main sequence, why 1.44 is derived while 2.5 is hardcoded, and why the supernova is an animation rather than a simulation.
