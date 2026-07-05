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
