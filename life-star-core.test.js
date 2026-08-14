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

test('eos: ideal-gas-dominated matter gives gamma1 = 5/3 and n = 1.5', () => {
  // Low density, moderate temperature: gas dominates over radiation by ~55x
  // and over degeneracy by ~441x.
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
  const r = LS.eos(1e3, 1e4, 1.4, 2.0);
  assert.ok(r.pDeg > 100 * r.pGas, 'test setup wrong: degeneracy should dominate');
  assert.ok(Math.abs(r.gamma1 - 5 / 3) < 0.05, `gamma1 was ${r.gamma1}`);
});

test('eos: cold very dense matter is relativistically degenerate', () => {
  const r = LS.eos(1e9, 1e5, 1.4, 2.0);
  assert.ok(r.pDeg > 100 * r.pGas, 'test setup wrong: degeneracy should dominate');
  assert.ok(Math.abs(r.gamma1 - 4 / 3) < 0.05, `gamma1 was ${r.gamma1}`);
});

test('eos: n stays within [1.5, 3.0] across all physical regimes', () => {
  for (const [rho, T] of [[1e-8, 1e9], [1e10, 1e4], [1, 1e7], [1e6, 1e8]]) {
    const r = LS.eos(rho, T, 0.6, 2.0);
    assert.ok(r.n >= 1.5 && r.n <= 3.0, `n was ${r.n} at rho=${rho} T=${T}`);
  }
});

test('eos: NaN from zero pressure is clamped to n = 3.4', () => {
  // P = 0 gives gamma1 = 0/0 = NaN, which must not propagate to laneEmden.
  const r = LS.eos(0, 0, 0.6, 2.0);
  assert.ok(Number.isFinite(r.n), `n must be finite, got ${r.n}`);
  assert.strictEqual(r.n, 3.4, `n should be clamped to 3.4, got ${r.n}`);
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

test('structure: raising mean molecular weight heats the star', () => {
  // Hydrogen partly burned to helium: mu rises, so at fixed rho_c and P_c
  // the ideal-gas relation T = P*mu/(rho*k) makes the core hotter directly.
  //
  // NOT asserting R falls here: measured at LS.M_SUN with these two
  // compositions, R actually rises slightly (0.474 -> 0.497 R_sun) rather
  // than falling, because the mu-driven core heating alone (~1.37x, before
  // any radius change) already outruns the ~3.5x rise in L_target given how
  // steeply nuclear output responds to T at this T_c, while X^2 in the fuel
  // term drops 4x (0.35^2/0.7^2) — net a wash, resolved by a small radius
  // increase rather than the contraction naive homology intuition predicts.
  // Checked at 0.5 solar masses too: there R does fall (0.330 -> 0.291
  // R_sun), so the direction is mass-dependent in this toy model, not a bug.
  // See the fix-round-2 report for the numbers.
  const young = LS.structure(LS.M_SUN, { H1: 0.70, He4: 0.28, C12: 0.02 });
  const old = LS.structure(LS.M_SUN, { H1: 0.35, He4: 0.63, C12: 0.02 });
  assert.ok(old.tC > young.tC, 'central temperature should rise as mu rises');
});

// Nothing else checks that the bisection actually converged: the L ~ M^3
// slope test is arithmetic on a closed-form expression and would pass even
// if nuclear() and centralState() were broken, and the Stefan-Boltzmann
// test is tautological once L is derived from tEff. This is the test that
// would catch a star with no radius at which nuclear output can match the
// homology target — e.g. once its hydrogen is exhausted, the bisection
// silently runs to a bracket endpoint and returns a nonsense star with
// nothing thrown. Task 5 handles exhaustion explicitly; this guards the
// healthy main-sequence case.
test('structure: converged luminosity matches the homology target within 1%', () => {
  const s = LS.structure(LS.M_SUN, SOLAR);
  const { mu } = LS.meanMolecularWeight(SOLAR);
  const target = LS.L_SUN * Math.pow(LS.M_SUN / LS.M_SUN, 3) * Math.pow(mu / 0.6, 4);
  const rel = Math.abs(s.L - target) / target;
  assert.ok(rel < 0.01, `relative difference was ${rel}`);
});

test('structure: determinism — identical input gives identical output', () => {
  const a = LS.structure(2 * LS.M_SUN, SOLAR);
  const b = LS.structure(2 * LS.M_SUN, SOLAR);
  assert.strictEqual(a.R, b.R);
  assert.strictEqual(a.tC, b.tC);
  assert.strictEqual(a.L, b.L);
});
