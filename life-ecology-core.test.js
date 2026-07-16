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
    const next = stepEcology(grid, new Float32Array(grid.length), new Uint8Array(grid.length), COLS, ROWS, 'none', 'none', params, mulberry32(t + 1)).grid;
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
    const next = stepEcology(grid, new Float32Array(grid.length), new Uint8Array(grid.length), COLS, ROWS, 'none', 'none', params, mulberry32(t + 1)).grid;
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
    const next = stepEcology(grid, new Float32Array(grid.length), new Uint8Array(grid.length), COLS, ROWS, 'none', 'none', params, mulberry32(t + 1)).grid;
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
  for (let t = 0; t < 400; t++) { const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', params, rng); grid = out.grid; energy = out.energy; }
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
  const next = stepEcology(grid, new Float32Array(grid.length), new Uint8Array(grid.length), COLS, ROWS, 'none', 'none', params, mulberry32(1)).grid;
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
  const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', PRED, () => 0.5);
  let preyGone = 0;
  for (let i = 0; i < N; i++) if ((before[i]===STATES.RED||before[i]===STATES.GREY) && out.grid[i]!==before[i]) preyGone++;
  let fed = 0;
  for (let i = 0; i < N; i++) if (before[i]===STATES.MARTEN && out.energy[i] > energy[i] - PRED.delta + 1e-9) fed++;
  assert.strictEqual(preyGone, fed, `prey removed (${preyGone}) must equal martens fed (${fed})`);
  // Counting fed martens can't see a double meal (one meal -> 7, two -> 10; both "fed").
  // Summing the energy does: eBreed/eCap are 100 here so no clamp or breedCost perturbs it.
  let gained = 0;
  for (let i = 0; i < N; i++) if (before[i]===STATES.MARTEN) gained += out.energy[i] - (energy[i] - PRED.delta);
  assert.strictEqual(gained, preyGone * PRED.g, `energy gained (${gained}) must be exactly ${PRED.g} per prey eaten (${preyGone})`);
});

test('missed hunt: two martens, one prey -> prey removed once, one fed one starves', () => {
  const COLS = 5, ROWS = 5, N = COLS * ROWS;
  const grid = new Uint8Array(N), energy = new Float32Array(N);
  grid[2*COLS+1] = STATES.MARTEN; energy[2*COLS+1] = 5;
  grid[2*COLS+3] = STATES.MARTEN; energy[2*COLS+3] = 5;
  grid[2*COLS+2] = STATES.RED;   // the single shared prey, between them
  const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', PRED, () => 0.5);
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
    const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', PRED, () => 0.5);
    grid = out.grid; energy = out.energy;
  }
  assert.strictEqual(grid[2*COLS+2], STATES.EMPTY, 'marten starved to death');
});

test('breeding: eligible marten beside empty spawns a marten and pays breedCost', () => {
  const COLS = 5, ROWS = 5, N = COLS * ROWS;
  const grid = new Uint8Array(N), energy = new Float32Array(N);
  grid[2*COLS+2] = STATES.MARTEN; energy[2*COLS+2] = 12;   // >= eBreed
  const P = { ...PRED, eBreed: 10, breedCost: 5, mu: 1, e0: 5 };  // mu=1 -> spill certain
  const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', P, () => 0.0);
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
    const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', params, rng);
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

// --- slice 3: asymmetric evasion (the cascade) -----------------------------

test('evade: a prey that evades is not eaten and its hunter goes hungry', () => {
  const COLS = 5, ROWS = 5, N = COLS * ROWS;
  const at = (c, r) => r * COLS + c;
  // one marten, one prey, no competition: the evade roll is the only thing in play.
  const build = () => {
    const grid = new Uint8Array(N), energy = new Float32Array(N);
    grid[at(2,2)] = STATES.MARTEN; energy[at(2,2)] = 5;
    grid[at(2,3)] = STATES.RED;
    return { grid, energy };
  };
  // evadeRed=1 -> always escapes. rng()=0.5 < 1, so the roll fires.
  let b = build();
  let out = stepEcology(b.grid, b.energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight',
                        { ...PRED, evadeRed: 1, evadeGrey: 0 }, () => 0.5);
  assert.strictEqual(out.grid[at(2,3)], STATES.RED, 'evading prey survives');
  assert.strictEqual(out.energy[at(2,2)], 5 - PRED.delta, 'hunter gained nothing and still paid delta');

  // evadeRed=0 -> same setup, prey is eaten. Confirms the difference is the evade roll.
  b = build();
  out = stepEcology(b.grid, b.energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight',
                    { ...PRED, evadeRed: 0, evadeGrey: 0 }, () => 0.5);
  assert.strictEqual(out.grid[at(2,3)], STATES.EMPTY, 'non-evading prey is eaten');
  assert.strictEqual(out.energy[at(2,2)], 5 - PRED.delta + PRED.g, 'hunter fed');
});

test('evade is per-species: the same rule eats the naive grey but not the evading red', () => {
  const COLS = 5, ROWS = 5, N = COLS * ROWS;
  const at = (c, r) => r * COLS + c;
  const grid = new Uint8Array(N), energy = new Float32Array(N);
  grid[at(1,1)] = STATES.MARTEN; energy[at(1,1)] = 5;
  grid[at(1,2)] = STATES.RED;                  // evades
  grid[at(3,3)] = STATES.MARTEN; energy[at(3,3)] = 5;
  grid[at(3,4)] = STATES.GREY;                 // naive
  const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight',
                          { ...PRED, evadeRed: 1, evadeGrey: 0 }, () => 0.5);
  assert.strictEqual(out.grid[at(1,2)], STATES.RED, 'red evades');
  assert.strictEqual(out.grid[at(3,4)], STATES.EMPTY, 'grey does not');
});

// The payoff assertion. Reds are near-extinct under grey invasion; martens arrive.
// Asymmetric evasion must reverse it; symmetric evasion -- same predator, same
// pressure, only the asymmetry removed -- must NOT. That is the ecological claim.
function cascadeTrial({ evadeRed, evadeGrey }) {
  const COLS = 60, ROWS = 30, N = COLS * ROWS;
  const INTRO = 300, END = 800;
  const params = { betaRed: 0.10, betaGrey: 0.14, sigma: 0.92, delta: 1.0, g: 3.0,
                   eBreed: 10, e0: 5, breedCost: 5, eCap: 15, mu: 0.5,
                   evadeRed, evadeGrey, gen: 0 };
  const rng = mulberry32(12345), seed = mulberry32(999);
  let grid = new Uint8Array(N), energy = new Float32Array(N);
  for (let i = 0; i < N; i++) {              // identical starting field for both arms
    const r = seed();
    if (r < 0.30) grid[i] = STATES.RED; else if (r < 0.60) grid[i] = STATES.GREY;
  }
  const tally = g => {
    let red = 0, grey = 0;
    for (const v of g) { if (v === STATES.RED) red++; else if (v === STATES.GREY) grey++; }
    return { red, grey };
  };
  let atIntro = null;
  for (let gen = 0; gen < END; gen++) {
    if (gen === INTRO) {
      atIntro = tally(grid);
      for (let i = 0; i < N; i++) if (grid[i] !== STATES.MARTEN && rng() < 0.02) { grid[i] = STATES.MARTEN; energy[i] = 8; }
    }
    params.gen = gen;
    ({ grid, energy } = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', params, rng));
  }
  return { atIntro, final: tally(grid) };
}

test('CASCADE: asymmetric evasion lets martens reverse the grey invasion', () => {
  const { atIntro, final } = cascadeTrial({ evadeRed: 0.70, evadeGrey: 0.05 });
  assert.ok(atIntro.red < atIntro.grey * 0.1,
    `setup: reds must be near-extinct when martens arrive (red ${atIntro.red}, grey ${atIntro.grey})`);
  assert.ok(final.red > atIntro.red * 5,
    `reds must recover strongly (${atIntro.red} -> ${final.red})`);
  assert.ok(final.grey < atIntro.grey * 0.5,
    `greys must be knocked back (${atIntro.grey} -> ${final.grey})`);
  assert.ok(final.red > final.grey,
    `reds must end up dominant (red ${final.red} vs grey ${final.grey})`);
});

// --- slice 4: alternative marten food (voles/birds/berries) ----------------

test('forage=0 changes nothing: the alternative-food term is off by default', () => {
  const COLS = 20, ROWS = 20, N = COLS * ROWS;
  const build = () => {
    const rng = mulberry32(3);
    const grid = new Uint8Array(N), energy = new Float32Array(N);
    for (let i = 0; i < N; i++) {
      const r = rng();
      if (r < 0.25) grid[i] = STATES.RED;
      else if (r < 0.45) grid[i] = STATES.GREY;
      else if (r < 0.55) { grid[i] = STATES.MARTEN; energy[i] = 6; }
    }
    return { grid, energy };
  };
  const P = { betaRed: 0.1, betaGrey: 0.14, sigma: 0.92, delta: 1, g: 3, eBreed: 10, e0: 5,
             breedCost: 5, eCap: 15, mu: 0.5, evadeRed: 0.7, evadeGrey: 0.05, gen: 5 };
  const a = build(), b = build();
  const outA = stepEcology(a.grid, a.energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', { ...P }, mulberry32(9));
  const outB = stepEcology(b.grid, b.energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', { ...P, forage: 0 }, mulberry32(9));
  assert.deepStrictEqual([...outB.grid], [...outA.grid], 'forage=0 must be identical to omitting forage');
  assert.deepStrictEqual([...outB.energy], [...outA.energy], 'energy must match too');
});

test('forage is a SHARED resource: a crowded marten gains less than an isolated one', () => {
  // The load-bearing property. A flat food term would be algebraically the same as
  // lowering delta and would add no dynamics; only saturation with crowding gives
  // martens a density cap. So the crowded marten MUST do worse.
  const COLS = 9, ROWS = 9, N = COLS * ROWS;
  const P = { betaRed: 0, betaGrey: 0, sigma: 1, delta: 1, g: 3, eBreed: 100, e0: 5,
              breedCost: 5, eCap: 100, mu: 0, evadeRed: 0, evadeGrey: 0, forage: 4, gen: 1 };
  const at = (c, r) => r * COLS + c;
  const grid = new Uint8Array(N), energy = new Float32Array(N);
  grid[at(1,1)] = STATES.MARTEN; energy[at(1,1)] = 5;          // isolated: 0 marten neighbours
  grid[at(5,5)] = STATES.MARTEN; energy[at(5,5)] = 5;          // crowded: 1 marten neighbour
  grid[at(6,5)] = STATES.MARTEN; energy[at(6,5)] = 5;
  const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', P, () => 0.5);
  const lone = out.energy[at(1,1)], crowded = out.energy[at(5,5)];
  assert.strictEqual(lone, 5 - 1 + 4 / 1, 'isolated marten gets the whole forage');
  assert.strictEqual(crowded, 5 - 1 + 4 / 2, 'a marten sharing with one neighbour gets half');
  assert.ok(crowded < lone, 'crowding must reduce the per-marten share');
});

test('forage lets martens outlive their prey, and the density self-caps', () => {
  const COLS = 40, ROWS = 20, N = COLS * ROWS;
  const run = forage => {
    const rng = mulberry32(11);
    const params = { betaRed: 0, betaGrey: 0, sigma: 1, delta: 1, g: 3, eBreed: 10, e0: 5,
                     breedCost: 5, eCap: 15, mu: 0.5, evadeRed: 0, evadeGrey: 0, forage, gen: 0 };
    let grid = new Uint8Array(N), energy = new Float32Array(N);   // martens only, zero prey
    for (let i = 0; i < N; i++) if (rng() < 0.10) { grid[i] = STATES.MARTEN; energy[i] = 8; }
    for (let gen = 0; gen < 400; gen++) {
      params.gen = gen;
      ({ grid, energy } = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', params, rng));
    }
    let n = 0; for (const v of grid) if (v === STATES.MARTEN) n++;
    return n;
  };
  assert.strictEqual(run(0), 0, 'without alternative food, martens starve out when prey is gone');
  const fed = run(2.0);
  assert.ok(fed > 0, `with alternative food martens persist on it (got ${fed})`);
  // Moore-8 crowding caps a no-neighbour packing at ~25% of the grid; the point is
  // that the shared resource bounds them well short of filling it.
  assert.ok(fed < N * 0.5, `density must self-cap, not fill the grid (got ${fed} of ${N})`);
});

test('HYPERPREDATION: subsidised martens extirpate the greys instead of cycling with them', () => {
  const COLS = 60, ROWS = 30, N = COLS * ROWS;
  const greysAfter = forage => {
    const params = { betaRed: 0.10, betaGrey: 0.14, sigma: 0.92, delta: 1.0, g: 3.0, eBreed: 10,
                     e0: 5, breedCost: 5, eCap: 15, mu: 0.5, evadeRed: 0.70, evadeGrey: 0.05, forage, gen: 0 };
    const rng = mulberry32(12345), seed = mulberry32(999);
    let grid = new Uint8Array(N), energy = new Float32Array(N);
    for (let i = 0; i < N; i++) { const r = seed(); if (r < 0.30) grid[i] = STATES.RED; else if (r < 0.60) grid[i] = STATES.GREY; }
    for (let gen = 0; gen < 1000; gen++) {
      if (gen === 300) for (let i = 0; i < N; i++) if (grid[i] !== STATES.MARTEN && rng() < 0.02) { grid[i] = STATES.MARTEN; energy[i] = 8; }
      params.gen = gen;
      ({ grid, energy } = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', params, rng));
    }
    let grey = 0; for (const v of grid) if (v === STATES.GREY) grey++;
    return grey;
  };
  // Unsubsidised martens starve as the greys thin out, so the greys always rebound.
  assert.ok(greysAfter(0) > 0, 'without alternative food the greys survive by outlasting the predator');
  // Subsidised martens do not decline with their prey, so the greys get no refuge.
  assert.strictEqual(greysAfter(1.0), 0, 'with alternative food the greys are extirpated');
});

// --- slice 5: founder viability (mate requirement + background mortality) ---

test('mate=false / mortality=0 change nothing: slice 5 is off by default', () => {
  const COLS = 20, ROWS = 20, N = COLS * ROWS;
  const build = () => {
    const rng = mulberry32(3);
    const grid = new Uint8Array(N), energy = new Float32Array(N);
    for (let i = 0; i < N; i++) {
      const r = rng();
      if (r < 0.25) grid[i] = STATES.RED;
      else if (r < 0.45) grid[i] = STATES.GREY;
      else if (r < 0.60) { grid[i] = STATES.MARTEN; energy[i] = 12; }   // above eBreed
    }
    return { grid, energy };
  };
  const P = { betaRed: 0.1, betaGrey: 0.14, sigma: 0.92, delta: 1, g: 3, eBreed: 10, e0: 5,
              breedCost: 5, eCap: 15, mu: 0.5, evadeRed: 0.9, evadeGrey: 0.05, forage: 1, gen: 5 };
  const a = build(), b = build();
  const outA = stepEcology(a.grid, a.energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', { ...P }, mulberry32(9));
  const outB = stepEcology(b.grid, b.energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight',
                           { ...P, mate: false, mortality: 0 }, mulberry32(9));
  assert.deepStrictEqual([...outB.grid], [...outA.grid], 'defaults must be identical to omitting them');
  assert.deepStrictEqual([...outB.energy], [...outA.energy], 'energy must match too');
});

test('mate: a lone marten cannot breed however well fed; a paired one can', () => {
  const COLS = 9, ROWS = 9, N = COLS * ROWS;
  const at = (c, r) => r * COLS + c;
  const P = { betaRed: 0, betaGrey: 0, sigma: 1, delta: 1, g: 3, eBreed: 10, e0: 5,
              breedCost: 5, eCap: 100, mu: 1, forage: 0, evadeRed: 0, evadeGrey: 0, gen: 1 };
  const martensAfter = (mate, paired) => {
    const grid = new Uint8Array(N), energy = new Float32Array(N);
    grid[at(4,4)] = STATES.MARTEN; energy[at(4,4)] = 50;      // far above eBreed
    if (paired) { grid[at(5,4)] = STATES.MARTEN; energy[at(5,4)] = 50; }
    // mu=1 -> any eligible neighbour breeds into an empty cell with certainty
    const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', { ...P, mate }, () => 0.5);
    let n = 0; for (const v of out.grid) if (v === STATES.MARTEN) n++;
    return n;
  };
  assert.ok(martensAfter(false, false) > 1, 'without the mate rule, one marten breeds alone (slice 4 behaviour)');
  assert.strictEqual(martensAfter(true, false), 1, 'with the mate rule, a lone marten founds nothing');
  assert.ok(martensAfter(true, true) > 2, 'with the mate rule, a pair does breed');
});

test('mate: the parent is not charged breedCost when it has no mate to breed with', () => {
  // Parent and offspring must agree on eligibility, or a lone marten pays for a
  // birth that never happens and slowly starves for nothing.
  const COLS = 9, ROWS = 9, N = COLS * ROWS;
  const at = (c, r) => r * COLS + c;
  const grid = new Uint8Array(N), energy = new Float32Array(N);
  grid[at(4,4)] = STATES.MARTEN; energy[at(4,4)] = 12;    // eligible on energy, but alone
  const P = { betaRed: 0, betaGrey: 0, sigma: 1, delta: 1, g: 3, eBreed: 10, e0: 5,
              breedCost: 5, eCap: 100, mu: 0.5, forage: 0, evadeRed: 0, evadeGrey: 0,
              mate: true, gen: 1 };
  const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', P, () => 0.9);
  assert.strictEqual(out.energy[at(4,4)], 12 - 1, 'lone marten pays delta only, not breedCost');
});

test('mortality: martens die at the background rate regardless of energy', () => {
  const COLS = 40, ROWS = 40, N = COLS * ROWS;
  const P = { betaRed: 0, betaGrey: 0, sigma: 1, delta: 0, g: 3, eBreed: 1000, e0: 5,
              breedCost: 5, eCap: 100, mu: 0, forage: 0, evadeRed: 0, evadeGrey: 0,
              mortality: 0.10, gen: 1 };
  const grid = new Uint8Array(N), energy = new Float32Array(N);
  for (let i = 0; i < N; i++) { grid[i] = STATES.MARTEN; energy[i] = 99; }  // delta=0, eBreed high: only mortality acts
  const out = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', P, mulberry32(5));
  let alive = 0; for (const v of out.grid) if (v === STATES.MARTEN) alive++;
  const died = (N - alive) / N;
  assert.ok(Math.abs(died - 0.10) < 0.02, `~10% should die of background causes, got ${(died*100).toFixed(1)}%`);
});

test('mortality: without it a break-even marten is immortal, with it it is not', () => {
  const COLS = 9, ROWS = 9, N = COLS * ROWS;
  const at = (c, r) => r * COLS + c;
  // forage exactly cancels delta -> the energy ledger can never kill this marten.
  const P = { betaRed: 0, betaGrey: 0, sigma: 1, delta: 1, g: 3, eBreed: 100, e0: 5,
              breedCost: 5, eCap: 15, mu: 0, forage: 1.0, evadeRed: 0, evadeGrey: 0, gen: 0 };
  const survive = mortality => {
    let grid = new Uint8Array(N), energy = new Float32Array(N);
    grid[at(4,4)] = STATES.MARTEN; energy[at(4,4)] = 5;
    const rng = mulberry32(2), params = { ...P, mortality };
    for (let gen = 0; gen < 2000; gen++) {
      params.gen = gen;
      ({ grid, energy } = stepEcology(grid, energy, new Uint8Array(COLS*ROWS), COLS, ROWS, 'straight', 'straight', params, rng));
    }
    return grid[at(4,4)] === STATES.MARTEN;
  };
  assert.strictEqual(survive(0), true, 'break-even marten never dies without background mortality');
  assert.strictEqual(survive(0.005), false, 'with mortality it eventually does');
});

// --- slice 6: squirrelpox ---------------------------------------------------
const POX = { betaRed: 0, betaGrey: 0, sigma: 1, delta: 1, g: 3, eBreed: 100, e0: 5,
              breedCost: 5, eCap: 100, mu: 0, forage: 0, evadeRed: 0, evadeGrey: 0, gen: 1 };

test('poxBeta=0 changes nothing: squirrelpox is off by default', () => {
  const COLS = 20, ROWS = 20, N = COLS * ROWS;
  const build = () => {
    const rng = mulberry32(3);
    const grid = new Uint8Array(N), energy = new Float32Array(N), inf = new Uint8Array(N);
    for (let i = 0; i < N; i++) {
      const r = rng();
      if (r < 0.25) grid[i] = STATES.RED;
      else if (r < 0.45) grid[i] = STATES.GREY;
      else if (r < 0.55) { grid[i] = STATES.MARTEN; energy[i] = 6; }
      if (grid[i] === STATES.GREY) inf[i] = 1;      // carriers present but pox disabled
    }
    return { grid, energy, inf };
  };
  const P = { betaRed: 0.1, betaGrey: 0.14, sigma: 0.92, delta: 1, g: 3, eBreed: 10, e0: 5,
              breedCost: 5, eCap: 15, mu: 0.5, evadeRed: 0.9, evadeGrey: 0.05, forage: 2, gen: 5 };
  const a = build(), b = build();
  const outA = stepEcology(a.grid, a.energy, a.inf, COLS, ROWS, 'straight', 'straight', { ...P }, mulberry32(9));
  const outB = stepEcology(b.grid, b.energy, b.inf, COLS, ROWS, 'straight', 'straight',
                           { ...P, poxBeta: 0, poxLethal: 0.5 }, mulberry32(9));
  assert.deepStrictEqual([...outB.grid], [...outA.grid], 'poxBeta=0 must be identical to omitting it');
  assert.deepStrictEqual([...outB.energy], [...outA.energy], 'energy must match too');
});

test('pox: greys are an asymptomatic reservoir, reds are killed by it', () => {
  const COLS = 5, ROWS = 5, N = COLS * ROWS;
  const at = (c, r) => r * COLS + c;
  const P = { ...POX, poxBeta: 0, poxLethal: 1 };   // no transmission; only the lethality acts
  const grid = new Uint8Array(N), energy = new Float32Array(N), inf = new Uint8Array(N);
  grid[at(1,1)] = STATES.GREY; inf[at(1,1)] = 1;
  grid[at(3,3)] = STATES.RED;  inf[at(3,3)] = 1;
  // poxBeta must be > 0 for the branch to run at all; use a tiny value with no
  // infected neighbours in range so nothing new is infected.
  const out = stepEcology(grid, energy, inf, COLS, ROWS, 'straight', 'straight',
                          { ...P, poxBeta: 1e-9 }, () => 0.5);
  assert.strictEqual(out.grid[at(1,1)], STATES.GREY, 'infected grey survives — it is the reservoir');
  assert.strictEqual(out.infected[at(1,1)], 1, 'and stays a carrier for life');
  assert.strictEqual(out.grid[at(3,3)], STATES.EMPTY, 'infected red is killed');
});

test('pox: an uninfected prey catches it at 1-(1-poxBeta)^k from k infected neighbours', () => {
  const COLS = 3, ROWS = 3;
  const k = 2, poxBeta = 0.3, expected = 1 - Math.pow(1 - poxBeta, k);
  let caught = 0, trials = 20000;
  for (let t = 0; t < trials; t++) {
    const grid = new Uint8Array(COLS * ROWS), inf = new Uint8Array(COLS * ROWS);
    const at = (c, r) => r * COLS + c;
    grid[at(1,1)] = STATES.GREY;                       // grey so it cannot die of pox
    grid[at(0,1)] = STATES.GREY; inf[at(0,1)] = 1;
    grid[at(2,1)] = STATES.GREY; inf[at(2,1)] = 1;
    const out = stepEcology(grid, new Float32Array(9), inf, COLS, ROWS, 'none', 'none',
                            { ...POX, poxBeta, poxLethal: 0 }, mulberry32(t + 1));
    if (out.infected[at(1,1)]) caught++;
  }
  assert.ok(Math.abs(caught / trials - expected) < 0.02, `infection rate ${caught/trials} vs ${expected}`);
});

test('pox: newborn prey are not born infected', () => {
  const COLS = 3, ROWS = 3;
  const at = (c, r) => r * COLS + c;
  const grid = new Uint8Array(9), inf = new Uint8Array(9);
  grid[at(0,1)] = STATES.GREY; inf[at(0,1)] = 1;      // infected parent beside an empty centre
  grid[at(2,1)] = STATES.GREY; inf[at(2,1)] = 1;
  const out = stepEcology(grid, new Float32Array(9), inf, COLS, ROWS, 'none', 'none',
                          { ...POX, betaGrey: 1, poxBeta: 0.5, poxLethal: 0 }, () => 0.01);
  assert.strictEqual(out.grid[at(1,1)], STATES.GREY, 'centre was colonised');
  assert.strictEqual(out.infected[at(1,1)], 0, 'the newborn is clean — no vertical transmission');
});

test('pox: it cannot persist without the grey reservoir', () => {
  // Reds die too fast to keep it alive on their own. Kill the greys and the
  // epidemic burns out — which is why crashing the greys helps reds twice over.
  const COLS = 30, ROWS = 30, N = COLS * ROWS;
  const rng = mulberry32(4);
  let grid = new Uint8Array(N), energy = new Float32Array(N), infected = new Uint8Array(N);
  for (let i = 0; i < N; i++) if (rng() < 0.5) { grid[i] = STATES.RED; infected[i] = rng() < 0.3 ? 1 : 0; }
  const params = { betaRed: 0.10, betaGrey: 0, sigma: 0.92, delta: 1, g: 3, eBreed: 10, e0: 5,
                   breedCost: 5, eCap: 15, mu: 0.5, forage: 0, evadeRed: 0, evadeGrey: 0,
                   poxBeta: 0.3, poxLethal: 0.5, gen: 0 };
  for (let gen = 0; gen < 300; gen++) {
    params.gen = gen;
    ({ grid, energy, infected } = stepEcology(grid, energy, infected, COLS, ROWS, 'straight', 'straight', params, rng));
  }
  let stillInfected = 0, reds = 0;
  for (let i = 0; i < N; i++) { if (infected[i]) stillInfected++; if (grid[i] === STATES.RED) reds++; }
  assert.strictEqual(stillInfected, 0, `pox must burn out in a red-only population (got ${stillInfected})`);
  assert.ok(reds > 0, 'and the surviving reds recover the ground');
});

test('POX ACCELERATION: the pathogen replaces reds far faster than competition alone', () => {
  const COLS = 60, ROWS = 30, N = COLS * ROWS;
  const redsLeft = poxBeta => {
    const params = { betaRed: 0.10, betaGrey: 0.14, sigma: 0.92, delta: 1, g: 3, eBreed: 10,
                     e0: 5, breedCost: 5, eCap: 15, mu: 0.5, forage: 0,
                     evadeRed: 0.90, evadeGrey: 0.05, poxBeta, poxLethal: 0.5, gen: 0 };
    const rng = mulberry32(12345), seed = mulberry32(999);
    let grid = new Uint8Array(N), energy = new Float32Array(N), infected = new Uint8Array(N);
    for (let i = 0; i < N; i++) { const r = seed(); if (r < 0.30) grid[i] = STATES.RED; else if (r < 0.60) grid[i] = STATES.GREY; }
    // one infected grey: the index case
    for (let i = 0; i < N; i++) if (grid[i] === STATES.GREY) { infected[i] = 1; break; }
    const trace = [];
    for (let gen = 0; gen < 200; gen++) {
      params.gen = gen;
      ({ grid, energy, infected } = stepEcology(grid, energy, infected, COLS, ROWS, 'straight', 'straight', params, rng));
      let reds = 0; for (const v of grid) if (v === STATES.RED) reds++;
      trace.push(reds);
    }
    return trace;
  };
  const clean = redsLeft(0), poxed = redsLeft(0.25);
  assert.ok(poxed[199] < clean[199] * 0.5,
    `pox must crush the reds far below competition alone (pox ${poxed[199]} vs clean ${clean[199]})`);
  // and it must do so much sooner
  const halfOf = t => { const start = t[0]; return t.findIndex(v => v < start * 0.5); };
  assert.ok(halfOf(poxed) < halfOf(clean), `pox must halve the reds sooner (pox gen ${halfOf(poxed)} vs clean gen ${halfOf(clean)})`);
});

// --- slice 7: terrain refuges ----------------------------------------------

test('terrain=0 changes nothing: the field is inert without an amplitude', () => {
  const COLS = 20, ROWS = 20, N = COLS * ROWS;
  const env = new Float32Array(N);
  for (let i = 0; i < N; i++) env[i] = Math.sin(i) ;       // a real, non-flat field
  const build = () => {
    const rng = mulberry32(3);
    const grid = new Uint8Array(N), energy = new Float32Array(N), inf = new Uint8Array(N);
    for (let i = 0; i < N; i++) {
      const r = rng();
      if (r < 0.25) grid[i] = STATES.RED;
      else if (r < 0.45) grid[i] = STATES.GREY;
      else if (r < 0.55) { grid[i] = STATES.MARTEN; energy[i] = 6; }
    }
    return { grid, energy, inf };
  };
  const P = { betaRed: 0.1, betaGrey: 0.14, sigma: 0.92, delta: 1, g: 3, eBreed: 10, e0: 5,
              breedCost: 5, eCap: 15, mu: 0.5, evadeRed: 0.9, evadeGrey: 0.05, forage: 2, gen: 5 };
  const a = build(), b = build();
  const outA = stepEcology(a.grid, a.energy, a.inf, COLS, ROWS, 'straight', 'straight', { ...P }, mulberry32(9));
  const outB = stepEcology(b.grid, b.energy, b.inf, COLS, ROWS, 'straight', 'straight',
                           { ...P, terrain: 0, env }, mulberry32(9));
  assert.deepStrictEqual([...outB.grid], [...outA.grid], 'terrain=0 must be identical even with a field present');
});

test('terrain: conifer favours red, broadleaf favours grey', () => {
  const COLS = 3, ROWS = 3, N = 9;
  const at = (c, r) => r * COLS + c;
  // one red and one grey flanking an empty centre: who takes it?
  const contest = envValue => {
    const env = new Float32Array(N).fill(envValue);
    let reds = 0, greys = 0;
    for (let t = 0; t < 20000; t++) {
      const grid = new Uint8Array(N);
      grid[at(0,1)] = STATES.RED;
      grid[at(2,1)] = STATES.GREY;
      const out = stepEcology(grid, new Float32Array(N), new Uint8Array(N), COLS, ROWS, 'none', 'none',
        { betaRed: 0.10, betaGrey: 0.14, sigma: 1, terrain: 0.8, env }, mulberry32(t + 1));
      const c = out.grid[at(1,1)];
      if (c === STATES.RED) reds++; else if (c === STATES.GREY) greys++;
    }
    return { reds, greys };
  };
  const conifer = contest(-1), broadleaf = contest(+1);
  assert.ok(conifer.reds > conifer.greys,
    `conifer must favour red (red ${conifer.reds} vs grey ${conifer.greys})`);
  assert.ok(broadleaf.greys > broadleaf.reds * 2,
    `broadleaf must favour grey (grey ${broadleaf.greys} vs red ${broadleaf.reds})`);
});

test('terrain: red is never excluded from broadleaf, only outcompeted there', () => {
  // Reds held the whole country before the greys arrived. If terrain scaled red down
  // in broadleaf, reds could never retake ground the marten cleared — the refuge
  // would become a prison. Red's beta must be terrain-independent.
  const COLS = 3, ROWS = 3, N = 9;
  const at = (c, r) => r * COLS + c;
  const redAlone = envValue => {
    const env = new Float32Array(N).fill(envValue);
    let reds = 0, trials = 20000;
    for (let t = 0; t < trials; t++) {
      const grid = new Uint8Array(N);
      grid[at(0,1)] = STATES.RED; grid[at(2,1)] = STATES.RED;   // no greys anywhere
      const out = stepEcology(grid, new Float32Array(N), new Uint8Array(N), COLS, ROWS, 'none', 'none',
        { betaRed: 0.30, betaGrey: 0.14, sigma: 1, terrain: 1.0, env }, mulberry32(t + 1));
      if (out.grid[at(1,1)] === STATES.RED) reds++;
    }
    return reds / trials;
  };
  const inConifer = redAlone(-1), inBroadleaf = redAlone(+1);
  const expected = 1 - Math.pow(1 - 0.30, 2);
  assert.ok(Math.abs(inBroadleaf - expected) < 0.02,
    `red must colonise broadleaf at its full rate (${inBroadleaf} vs ${expected})`);
  assert.ok(Math.abs(inConifer - inBroadleaf) < 0.02,
    `red must be terrain-blind (conifer ${inConifer} vs broadleaf ${inBroadleaf})`);
});

test('terrain: the marten is indifferent to it', () => {
  // The predator works conifer and broadleaf equally well. Same seed, same rng,
  // opposite terrain -> the marten's hunting and breeding must not budge.
  const COLS = 12, ROWS = 12, N = COLS * ROWS;
  const run = envValue => {
    const env = new Float32Array(N).fill(envValue);
    const rng = mulberry32(7);
    const grid = new Uint8Array(N), energy = new Float32Array(N);
    for (let i = 0; i < N; i++) if (i % 3 === 0) { grid[i] = STATES.MARTEN; energy[i] = 12; }
    // no prey at all, so only marten behaviour can differ
    const out = stepEcology(grid, energy, new Uint8Array(N), COLS, ROWS, 'straight', 'straight',
      { betaRed: 0.1, betaGrey: 0.14, sigma: 0.92, delta: 1, g: 3, eBreed: 10, e0: 5, breedCost: 5,
        eCap: 15, mu: 0.5, forage: 2, terrain: 1.0, env, gen: 3 }, rng);
    let n = 0; for (const v of out.grid) if (v === STATES.MARTEN) n++;
    return { n, energy: [...out.energy] };
  };
  const conifer = run(-1), broadleaf = run(+1);
  assert.strictEqual(conifer.n, broadleaf.n, 'marten count must not depend on terrain');
  assert.deepStrictEqual(conifer.energy, broadleaf.energy, 'marten energy must not depend on terrain');
});

test('REFUGE: a conifer patch keeps reds alive through a squirrelpox epidemic', () => {
  // The point of the whole slice. Greys colonise the conifer poorly, so the
  // reservoir never establishes there, and red-to-red transmission burns out.
  const COLS = 80, ROWS = 40, N = COLS * ROWS;
  const survivors = terrain => {
    const env = new Float32Array(N);
    for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) {
      // one conifer basin on the left third, broadleaf elsewhere
      const inRefuge = c < COLS / 3;
      env[r * COLS + c] = inRefuge ? -1 : +1;
    }
    const params = { betaRed: 0.10, betaGrey: 0.14, sigma: 0.92, delta: 1, g: 3, eBreed: 10,
                     e0: 5, breedCost: 5, eCap: 15, mu: 0.5, forage: 2, evadeRed: 0.90,
                     evadeGrey: 0.05, poxBeta: 0.25, poxLethal: 0.5, terrain, env, gen: 0 };
    const rng = mulberry32(12345), seed = mulberry32(999);
    let grid = new Uint8Array(N), energy = new Float32Array(N), infected = new Uint8Array(N);
    for (let i = 0; i < N; i++) { const r = seed(); if (r < 0.30) grid[i] = STATES.RED; else if (r < 0.60) grid[i] = STATES.GREY; }
    for (let gen = 0; gen < 600; gen++) {
      if (gen === 200) {   // introduce the pox into the broadleaf greys
        let n = 0;
        for (let i = 0; i < N && n < 20; i++) if (grid[i] === STATES.GREY && rng() < 0.02) { infected[i] = 1; n++; }
      }
      params.gen = gen;
      ({ grid, energy, infected } = stepEcology(grid, energy, infected, COLS, ROWS, 'straight', 'straight', params, rng));
    }
    let reds = 0; for (const v of grid) if (v === STATES.RED) reds++;
    return reds;
  };
  assert.strictEqual(survivors(0), 0, 'with no terrain the pox takes every red (slice 6 result)');
  assert.ok(survivors(0.9) > 0, 'a conifer refuge must leave reds alive');
});

test('BREAKOUT: reds are locked in the stronghold until the martens arrive', () => {
  // The payoff of the whole model. Pox present, reds confined to a conifer refuge —
  // Britain today. Release martens into the broadleaf and the reds should come out.
  const COLS = 120, ROWS = 60, N = COLS * ROWS, FRAC = 1 / 3;
  const env = new Float32Array(N);
  for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) env[r * COLS + c] = c < COLS * FRAC ? -1 : +1;
  const outsideReds = martens => {
    const params = { betaRed: 0.10, betaGrey: 0.14, sigma: 0.92, delta: 1, g: 3, eBreed: 10, e0: 5,
                     breedCost: 5, eCap: 15, mu: 0.5, forage: 2, mate: true, mortality: 0.005,
                     evadeRed: 0.90, evadeGrey: 0.05, poxBeta: 0.25, poxLethal: 0.5,
                     terrain: 0.9, env, gen: 0 };
    const rng = mulberry32(31), seed = mulberry32(901);
    let grid = new Uint8Array(N), energy = new Float32Array(N), infected = new Uint8Array(N);
    for (let i = 0; i < N; i++) { const r = seed(); if (r < 0.30) grid[i] = STATES.RED; else if (r < 0.60) grid[i] = STATES.GREY; }
    for (let gen = 0; gen < 2200; gen++) {
      if (gen === 200) { let n = 0; for (let i = 0; i < N && n < 20; i++) if (grid[i] === STATES.GREY && rng() < 0.02) { infected[i] = 1; n++; } }
      if (gen === 800 && martens) {
        let placed = 0; const c0 = (COLS * 0.75) | 0, r0 = ROWS >> 1;
        for (let rad = 0; placed < 40 && rad < 40; rad++)
          for (let dr = -rad; dr <= rad && placed < 40; dr++)
            for (let dc = -rad; dc <= rad && placed < 40; dc++) {
              if (Math.max(Math.abs(dr), Math.abs(dc)) !== rad) continue;
              const i = ((r0 + dr + ROWS) % ROWS) * COLS + ((c0 + dc + COLS) % COLS);
              if (grid[i] !== STATES.MARTEN) { grid[i] = STATES.MARTEN; energy[i] = 8; placed++; }
            }
      }
      params.gen = gen;
      ({ grid, energy, infected } = stepEcology(grid, energy, infected, COLS, ROWS, 'straight', 'straight', params, rng));
    }
    let inR = 0, outR = 0;
    for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++)
      if (grid[r * COLS + c] === STATES.RED) (c < COLS * FRAC ? inR++ : outR++);
    return { inR, outR };
  };
  const stuck = outsideReds(false);
  assert.ok(stuck.inR > 500, `reds must survive in the refuge (got ${stuck.inR})`);
  assert.ok(stuck.outR < stuck.inR * 0.05, `but stay locked inside it without martens (${stuck.outR} escaped)`);
  const freed = outsideReds(true);
  assert.ok(freed.outR > stuck.inR, `martens must let the reds break out (${freed.outR} outside vs ${stuck.inR} penned)`);
});

test('CASCADE control: symmetric evasion does NOT save the reds', () => {
  const { atIntro, final } = cascadeTrial({ evadeRed: 0.05, evadeGrey: 0.05 });
  assert.ok(final.grey < atIntro.grey * 0.9,
    `the predator still suppresses greys (${atIntro.grey} -> ${final.grey})`);
  assert.ok(final.red <= atIntro.red,
    `but reds must NOT recover without the asymmetry (${atIntro.red} -> ${final.red})`);
  assert.ok(final.red < final.grey,
    `reds must not become dominant (red ${final.red} vs grey ${final.grey})`);
});
