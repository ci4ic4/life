const test = require('node:test');
const assert = require('node:assert');
const { stepEcology, resolveCell, STATES } = require('./life-ecology-core.js');

// deterministic RNG for reproducible statistical tests
function mulberry32(a) {
  return function () {
    a |= 0; a = (a + 0x6D2B79F5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

test('empty cell with k red neighbours colonises red at prob 1-(1-beta)^k', () => {
  const COLS = 3, ROWS = 3;
  // centre empty, its 8 Moore neighbours: put 2 reds adjacent to centre
  const params = { betaRed: 0.3, betaGrey: 0.0, sigma: 1.0 };
  const k = 2, beta = 0.3, expected = 1 - Math.pow(1 - beta, k);
  let reds = 0, trials = 20000;
  for (let t = 0; t < trials; t++) {
    const grid = new Uint8Array(COLS * ROWS);
    grid[resolveCell(0, 1, COLS, ROWS, 'none', 'none')] = STATES.RED; // left of centre
    grid[resolveCell(2, 1, COLS, ROWS, 'none', 'none')] = STATES.RED; // right of centre
    const next = stepEcology(grid, COLS, ROWS, 'none', 'none', params, mulberry32(t + 1));
    if (next[resolveCell(1, 1, COLS, ROWS, 'none', 'none')] === STATES.RED) reds++;
  }
  assert.ok(Math.abs(reds / trials - expected) < 0.02, `red rate ${reds/trials} vs ${expected}`);
});

test('grey out-colonises red when betaGrey > betaRed (symmetric neighbours)', () => {
  const COLS = 3, ROWS = 3;
  const params = { betaRed: 0.10, betaGrey: 0.30, sigma: 1.0 };
  let reds = 0, greys = 0, trials = 20000;
  for (let t = 0; t < trials; t++) {
    const grid = new Uint8Array(COLS * ROWS);
    grid[resolveCell(0, 1, COLS, ROWS, 'none', 'none')] = STATES.RED;
    grid[resolveCell(2, 1, COLS, ROWS, 'none', 'none')] = STATES.GREY;
    const next = stepEcology(grid, COLS, ROWS, 'none', 'none', params, mulberry32(t + 1));
    const c = next[resolveCell(1, 1, COLS, ROWS, 'none', 'none')];
    if (c === STATES.RED) reds++; else if (c === STATES.GREY) greys++;
  }
  assert.ok(greys > reds * 1.5, `grey ${greys} should dominate red ${reds}`);
});

test('prey dies at rate 1-sigma with no neighbours', () => {
  const COLS = 3, ROWS = 3;
  const params = { betaRed: 0, betaGrey: 0, sigma: 0.8 };
  let survived = 0, trials = 20000;
  for (let t = 0; t < trials; t++) {
    const grid = new Uint8Array(COLS * ROWS);
    grid[resolveCell(1, 1, COLS, ROWS, 'none', 'none')] = STATES.RED;
    const next = stepEcology(grid, COLS, ROWS, 'none', 'none', params, mulberry32(t + 1));
    if (next[resolveCell(1, 1, COLS, ROWS, 'none', 'none')] === STATES.RED) survived++;
  }
  assert.ok(Math.abs(survived / trials - 0.8) < 0.02, `survival ${survived/trials}`);
});

test("resolveCell wraps straight and returns null on 'none' edge", () => {
  assert.strictEqual(resolveCell(-1, 0, 4, 4, 'straight', 'straight'), 0 * 4 + 3);
  assert.strictEqual(resolveCell(-1, 0, 4, 4, 'none', 'none'), null);
});
