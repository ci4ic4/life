const test = require('node:test');
const assert = require('node:assert/strict');
const LifeStats = require('./life-stats.js');

test('bumpMax peaks at member of set, falls off with distance', () => {
  const p3 = LifeStats.bumpMax(new Set([3]), 3, 0.6);
  const p2 = LifeStats.bumpMax(new Set([3]), 2, 0.6);
  const p0 = LifeStats.bumpMax(new Set([3]), 0, 0.6);
  assert.equal(p3, 1);
  assert.ok(p2 < p3 && p0 < p2);
});

test('curve applies floor/ceiling as a clamp around the Gaussian bump', () => {
  const out = new Float64Array(9);
  LifeStats.curve(new Set([2, 3]), { sigma: 0.05, ceil: 0.7, floor: 0.2 }, out);
  assert.ok(Math.abs(out[2] - 0.7) < 1e-9, 'peak hits ceiling');
  assert.ok(Math.abs(out[0] - 0.2) < 1e-9, 'tail hits floor');
});

test('resolveCell wraps a torus on both axes', () => {
  assert.equal(LifeStats.resolveCell(-1, 0, 10, 5, 'straight', 'straight'), 0 * 10 + 9);
  assert.equal(LifeStats.resolveCell(10, 0, 10, 5, 'straight', 'straight'), 0 * 10 + 0);
});

test('resolveCell returns null crossing a "none" boundary', () => {
  assert.equal(LifeStats.resolveCell(-1, 0, 10, 5, 'none', 'straight'), null);
  assert.equal(LifeStats.resolveCell(0, -1, 10, 5, 'straight', 'none'), null);
});

test('resolveCell flips the other axis crossing a "flip" seam (Klein bottle)', () => {
  // crossing the C seam (cwrap='flip') mirrors r -> rows-1-r
  assert.equal(LifeStats.resolveCell(-1, 1, 10, 5, 'flip', 'straight'), (5 - 1 - 1) * 10 + 9);
});

test('countNeighbors counts live cells around a point on a small torus', () => {
  const cols = 3, rows = 3;
  const grid = new Uint8Array(cols * rows);
  grid[0] = 1; // (0,0)
  grid[1] = 1; // (1,0)
  const n = LifeStats.countNeighbors(0, 0, cols, rows, 'straight', 'straight', grid);
  // (1,0) is a direct neighbor; (0,0) itself doesn't count
  assert.equal(n, 1);
});

test('makeRingStats computes mean/stdDev over a fixed window, ring-overwrites old values', () => {
  const rs = LifeStats.makeRingStats(3);
  [1, 1, 1].forEach(v => rs.push(v));
  assert.ok(rs.full());
  assert.equal(rs.mean(), 1);
  assert.equal(rs.stdDev(), 0);
  rs.push(0); // overwrites the oldest 1 -> window is now [1,1,0]
  assert.ok(Math.abs(rs.mean() - (2 / 3)) < 1e-9);
  assert.ok(rs.stdDev() > 0);
});

test('classify labels dead/full/stable/chaotic by thresholds', () => {
  const t = LifeStats.THRESHOLDS;
  assert.equal(LifeStats.classify(0.01, 0, t), 'dead');
  assert.equal(LifeStats.classify(0.97, 0, t), 'full');
  assert.equal(LifeStats.classify(0.4, 0.01, t), 'stable');
  assert.equal(LifeStats.classify(0.4, 0.2, t), 'chaotic');
});

test('stepGrid: an isolated live cell with 0 neighbors dies under default B3/S23-shaped tables', () => {
  const cols = 5, rows = 5;
  const grid = new Uint8Array(cols * rows);
  grid[12] = 1; // center, no neighbors
  const pBirth = new Float64Array(9), pSurvive = new Float64Array(9);
  LifeStats.curve(new Set([3]), { sigma: 0.6, ceil: 1, floor: 0 }, pBirth);
  LifeStats.curve(new Set([2, 3]), { sigma: 0.6, ceil: 1, floor: 0 }, pSurvive);
  const next = LifeStats.stepGrid(grid, cols, rows, 'straight', 'straight', pBirth, pSurvive, () => 0.5);
  assert.equal(next[12], 0, 'survive prob at n=0 is ~0, rng()=0.5 should not pass');
});

test('runHeadlessTrial: rng that always fails every threshold keeps an empty grid dead', () => {
  const result = LifeStats.runHeadlessTrial({
    cols: 10, rows: 10, cwrap: 'straight', rwrap: 'straight',
    gens: 60, windowSize: LifeStats.THRESHOLDS.WINDOW, thresholds: LifeStats.THRESHOLDS,
    birth: new Set([3]), survive: new Set([2, 3]),
    bParm: { sigma: 0.6, ceil: 1, floor: 0 }, sParm: { sigma: 0.6, ceil: 1, floor: 0 },
    density: 0.3, rng: () => 0.99,
  });
  assert.equal(result.status, 'dead');
  assert.equal(result.mean, 0);
});

test('runHeadlessTrial: rng that always passes fills and stays full', () => {
  const result = LifeStats.runHeadlessTrial({
    cols: 10, rows: 10, cwrap: 'straight', rwrap: 'straight',
    gens: 60, windowSize: LifeStats.THRESHOLDS.WINDOW, thresholds: LifeStats.THRESHOLDS,
    birth: new Set([3]), survive: new Set([2, 3]),
    bParm: { sigma: 0.6, ceil: 1, floor: 0 }, sParm: { sigma: 0.6, ceil: 1, floor: 0 },
    density: 0.3, rng: () => 0,
  });
  assert.equal(result.status, 'full');
  assert.equal(result.mean, 1);
});
