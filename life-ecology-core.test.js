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
    const next = stepEcology(grid, new Float32Array(grid.length), COLS, ROWS, 'none', 'none', params, mulberry32(t + 1)).grid;
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
    const next = stepEcology(grid, new Float32Array(grid.length), COLS, ROWS, 'none', 'none', params, mulberry32(t + 1)).grid;
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
    const next = stepEcology(grid, new Float32Array(grid.length), COLS, ROWS, 'none', 'none', params, mulberry32(t + 1)).grid;
    if (next[resolveCell(1, 1, COLS, ROWS, 'none', 'none')] === STATES.RED) survived++;
  }
  assert.ok(Math.abs(survived / trials - 0.8) < 0.02, `survival ${survived/trials}`);
});

test("resolveCell wraps straight and returns null on 'none' edge", () => {
  assert.strictEqual(resolveCell(-1, 0, 4, 4, 'straight', 'straight'), 0 * 4 + 3);
  assert.strictEqual(resolveCell(-1, 0, 4, 4, 'none', 'none'), null);
});

test('grey competitively excludes red on a torus over time', () => {
  const COLS = 60, ROWS = 30, N = COLS * ROWS;
  const params = { betaRed: 0.10, betaGrey: 0.18, sigma: 0.92 };
  const rng = mulberry32(12345);
  let grid = new Uint8Array(N);
  for (let i = 0; i < N; i++) grid[i] = rng() < 0.30 ? (rng() < 0.5 ? STATES.RED : STATES.GREY) : STATES.EMPTY;
  const count = g => { let nr = 0, ng = 0; for (const v of g) { if (v === STATES.RED) nr++; else if (v === STATES.GREY) ng++; } return { nr, ng }; };
  const start = count(grid);
  let energy = new Float32Array(N);
  for (let t = 0; t < 400; t++) { const out = stepEcology(grid, energy, COLS, ROWS, 'straight', 'straight', params, rng); grid = out.grid; energy = out.energy; }
  const end = count(grid);
  // grey share rises, red share falls — competitive exclusion of the native
  const startGreyShare = start.ng / (start.nr + start.ng);
  const endGreyShare   = end.ng / (end.nr + end.ng);
  assert.ok(endGreyShare > startGreyShare + 0.15, `grey share ${startGreyShare.toFixed(2)} -> ${endGreyShare.toFixed(2)}`);
  assert.ok(end.nr < start.nr, `red count should fall: ${start.nr} -> ${end.nr}`);
});

test('flip topology wraps column and mirrors row', () => {
  assert.strictEqual(resolveCell(-1, 0, 4, 4, 'flip', 'straight'), 15);
  assert.strictEqual(resolveCell(-1, 0, 4, 4, 'straight', 'straight'), 3);
});

test('empty cell with zero prey neighbours stays empty', () => {
  const COLS = 3, ROWS = 3;
  const params = { betaRed: 0.5, betaGrey: 0.5, sigma: 1.0 };
  const grid = new Uint8Array(COLS * ROWS);
  const next = stepEcology(grid, new Float32Array(grid.length), COLS, ROWS, 'none', 'none', params, mulberry32(1)).grid;
  const centreIndex = resolveCell(1, 1, COLS, ROWS, 'none', 'none');
  assert.strictEqual(next[centreIndex], STATES.EMPTY);
});

// --- slice 2: conserved predation -----------------------------------------
const PRED = { betaRed: 0, betaGrey: 0, sigma: 1, delta: 1, g: 3,
               eBreed: 100, e0: 5, breedCost: 5, eCap: 100, mu: 0, gen: 1 };

test('conservation: prey eaten equals martens that fed, no double meals', () => {
  const COLS = 8, ROWS = 8, N = COLS * ROWS;
  const grid = new Uint8Array(N), energy = new Float32Array(N);
  // scatter martens and prey so several martens border shared prey
  const put = (c, r, v, e) => { grid[r*COLS+c] = v; if (e != null) energy[r*COLS+c] = e; };
  put(2,2,STATES.MARTEN,5); put(4,2,STATES.MARTEN,5); put(3,2,STATES.RED);
  put(3,3,STATES.GREY); put(2,3,STATES.MARTEN,5); put(6,6,STATES.MARTEN,5); put(6,5,STATES.GREY);
  const before = grid.slice();
  const out = stepEcology(grid, energy, COLS, ROWS, 'straight', 'straight', PRED, () => 0.5);
  let preyGone = 0;
  for (let i = 0; i < N; i++) if ((before[i]===STATES.RED||before[i]===STATES.GREY) && out.grid[i]!==before[i]) preyGone++;
  let fed = 0;
  for (let i = 0; i < N; i++) if (before[i]===STATES.MARTEN && out.energy[i] > energy[i] - PRED.delta + 1e-9) fed++;
  assert.strictEqual(preyGone, fed, `prey removed (${preyGone}) must equal martens fed (${fed})`);
});

test('missed hunt: two martens, one prey -> prey removed once, one fed one starves', () => {
  const COLS = 5, ROWS = 5, N = COLS * ROWS;
  const grid = new Uint8Array(N), energy = new Float32Array(N);
  grid[2*COLS+1] = STATES.MARTEN; energy[2*COLS+1] = 5;
  grid[2*COLS+3] = STATES.MARTEN; energy[2*COLS+3] = 5;
  grid[2*COLS+2] = STATES.RED;   // the single shared prey, between them
  const out = stepEcology(grid, energy, COLS, ROWS, 'straight', 'straight', PRED, () => 0.5);
  assert.strictEqual(out.grid[2*COLS+2], STATES.EMPTY, 'prey eaten');
  const e1 = out.energy[2*COLS+1], e2 = out.energy[2*COLS+3];
  const gained = [e1, e2].filter(e => e > 5 - PRED.delta + 1e-9).length;
  const starved = [e1, e2].filter(e => Math.abs(e - (5 - PRED.delta)) < 1e-9).length;
  assert.strictEqual(gained, 1, 'exactly one marten fed');
  assert.strictEqual(starved, 1, 'exactly one marten missed and only drained delta');
});

test('starvation: a marten with no prey dies after ceil(E/delta) ticks', () => {
  const COLS = 5, ROWS = 5, N = COLS * ROWS;
  let grid = new Uint8Array(N), energy = new Float32Array(N);
  grid[2*COLS+2] = STATES.MARTEN; energy[2*COLS+2] = 3;   // 3 / delta(1) = 3 ticks
  for (let t = 0; t < 3; t++) {
    const out = stepEcology(grid, energy, COLS, ROWS, 'straight', 'straight', PRED, () => 0.5);
    grid = out.grid; energy = out.energy;
  }
  assert.strictEqual(grid[2*COLS+2], STATES.EMPTY, 'marten starved to death');
});

test('breeding: eligible marten beside empty spawns a marten and pays breedCost', () => {
  const COLS = 5, ROWS = 5, N = COLS * ROWS;
  const grid = new Uint8Array(N), energy = new Float32Array(N);
  grid[2*COLS+2] = STATES.MARTEN; energy[2*COLS+2] = 12;   // >= eBreed
  const P = { ...PRED, eBreed: 10, breedCost: 5, mu: 1, e0: 5 };  // mu=1 -> spill certain
  const out = stepEcology(grid, energy, COLS, ROWS, 'straight', 'straight', P, () => 0.0);
  let born = 0; for (let i = 0; i < N; i++) if (i !== 2*COLS+2 && out.grid[i] === STATES.MARTEN) born++;
  assert.ok(born >= 1, 'at least one offspring marten');
  // parent paid delta + breedCost: 12 - 1 - 5 = 6
  assert.ok(Math.abs(out.energy[2*COLS+2] - 6) < 1e-9, `parent energy ${out.energy[2*COLS+2]} should be 6`);
});

test('FEASIBILITY: predator and prey persist and oscillate on a torus', () => {
  const COLS = 60, ROWS = 30, N = COLS * ROWS;
  const params = { betaRed: 0.14, betaGrey: 0.14, sigma: 0.92,   // one prey behaviour: isolate predator
                   delta: 1.0, g: 3.0, eBreed: 10, e0: 5, breedCost: 5, eCap: 15, mu: 0.5, gen: 0 };
  const rng = mulberry32(7);
  let grid = new Uint8Array(N), energy = new Float32Array(N);
  for (let i = 0; i < N; i++) grid[i] = rng() < 0.30 ? STATES.RED : STATES.EMPTY;   // prey field
  for (let i = 0; i < N; i++) if (rng() < 0.02) { grid[i] = STATES.MARTEN; energy[i] = 8; }  // sparse martens
  const preyOf = g => { let n = 0; for (const v of g) if (v === STATES.RED || v === STATES.GREY) n++; return n; };
  const martOf = g => { let n = 0; for (const v of g) if (v === STATES.MARTEN) n++; return n; };
  const mart = [];
  for (let t = 0; t < 400; t++) {
    params.gen = t;
    const out = stepEcology(grid, energy, COLS, ROWS, 'straight', 'straight', params, rng);
    grid = out.grid; energy = out.energy;
    if (t >= 200) mart.push(martOf(grid));   // sample the settled second half
  }
  const mean = mart.reduce((a, b) => a + b, 0) / mart.length;
  const sd = Math.sqrt(mart.reduce((a, b) => a + (b - mean) ** 2, 0) / mart.length);
  assert.ok(preyOf(grid) > 0, `prey must survive (got ${preyOf(grid)})`);
  assert.ok(mean > 0, `martens must persist (mean ${mean.toFixed(1)})`);
  assert.ok(mean < N * 0.9, `martens must not fill the grid (mean ${mean.toFixed(1)} of ${N})`);
  assert.ok(sd > 1, `population should oscillate, not flatline (stdDev ${sd.toFixed(2)})`);
});
