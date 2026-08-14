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
