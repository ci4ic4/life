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
  const r = LS.eos(1e3, 1e4, 1.4, 2.0);
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
