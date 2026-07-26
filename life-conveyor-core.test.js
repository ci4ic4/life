// Node built-in test runner:  node --test
// Port of cl01/test_simulate_conveyor.py's four layers (unit / invariants / correctness /
// regression), plus a layer the Python suite has no equivalent for: the generalised
// part-and-recipe space.
const test = require('node:test');
const assert = require('node:assert/strict');
const C = require('./life-conveyor-core.js');

// --- helpers -----------------------------------------------------------------
const cfgAB = (over = {}) => Object.assign(C.defaultConfig(), over);

/** A config with three raw parts feeding one recipe. */
function cfg3() {
  return {
    numCells: 4, sides: 2,
    parts: [
      { id: 1, char: 'A' }, { id: 2, char: 'B' }, { id: 3, char: 'C' },
      { id: 4, char: 'P' },
    ],
    recipes: [{ out: 4, inputs: [1, 2, 3], time: 2 }],
    priority: 'north', supply: null,
  };
}

/** Drop `part` on cell 0 every tick; nothing else ever arrives. */
const fixedSupply = part => () => part;

// --- unit: parts and recipes --------------------------------------------------
test('rawParts excludes anything a recipe produces', () => {
  assert.deepEqual(C.rawParts(cfgAB()), [1, 2]);
  assert.deepEqual(C.rawParts(cfg3()), [1, 2, 3]);
});

test('a worker wants only inputs it is still short of', () => {
  const cfg = cfgAB();
  const st = C.createState(cfg, 1);
  const w = C.workerAt(st, cfg, C.NORTH, 0);
  assert.equal(C.wants(cfg, w, 1), true);
  assert.equal(C.wants(cfg, w, C.EMPTY), false, 'EMPTY is never wanted');
  w.held[1] = 1;
  assert.equal(C.wants(cfg, w, 1), false, 'one A is enough for B3/S23... er, for A+B');
  assert.equal(C.wants(cfg, w, 2), true);
});

test('startable fires only when every input is held', () => {
  const cfg = cfg3();
  const st = C.createState(cfg, 1);
  const w = C.workerAt(st, cfg, C.NORTH, 0);
  w.held[1] = 1; w.held[2] = 1;
  assert.equal(C.startable(cfg, w), -1);
  w.held[3] = 1;
  assert.equal(C.startable(cfg, w), 0);
});

// --- unit: assembly timing (cl01 test_assembly_time_respected) -----------------
test('a product appears exactly `time` ticks after the last pickup', () => {
  const cfg = cfgAB({ numCells: 1, sides: 1, supply: fixedSupply(1) });
  cfg.recipes = [{ out: 3, inputs: [1], time: 3 }];
  const st = C.createState(cfg, 1);

  C.step(st, cfg);                       // pickup tick: assembly starts and steps once
  const w = C.workerAt(st, cfg, C.NORTH, 0);
  assert.ok(w.building, 'assembling after pickup');
  assert.equal(w.building.remaining, 2);

  C.step(st, cfg); assert.equal(w.building.remaining, 1);
  C.step(st, cfg); assert.equal(w.building.remaining, 0);
  assert.equal(w.product, C.EMPTY, 'not finished yet');
  C.step(st, cfg);
  assert.equal(w.building, null);
  assert.equal(w.product, 3, 'product completed 3 ticks after pickup');
});

test('a worker holding a product cannot pick up', () => {
  const cfg = cfgAB({ numCells: 1, sides: 1, supply: fixedSupply(1) });
  const st = C.createState(cfg, 7);
  const w = C.workerAt(st, cfg, C.NORTH, 0);
  w.product = 3;
  C.step(st, cfg);
  assert.equal(w.held[1], 0, 'hands full, so the A goes by untouched');
  assert.equal(w.product, 3, 'still carrying it — the cell was occupied');
  assert.equal(st.belts[0][0], 1, 'the A stays on the belt');
});

test('a worker holding a product puts it on the first empty cell', () => {
  const cfg = cfgAB({ numCells: 1, sides: 1, supply: fixedSupply(C.EMPTY) });
  const st = C.createState(cfg, 7);
  const w = C.workerAt(st, cfg, C.NORTH, 0);
  w.product = 3;
  C.step(st, cfg);
  assert.equal(w.product, C.EMPTY, 'hands free again');
  assert.equal(st.belts[0][0], 3, 'product is on the belt');
});

// --- invariant 1: component conservation --------------------------------------
test('conservation holds across a long default run', () => {
  const cfg = cfgAB({ numCells: 5 });
  const r = C.runTrial(cfg, 42, 2000);
  for (const row of C.conservation(r.state, cfg))
    assert.equal(row.delta, 0, `part ${row.part} leaked: ${JSON.stringify(row)}`);
});

test('conservation holds with a slow assembly and a long belt', () => {
  const cfg = cfgAB({ numCells: 12 });
  cfg.recipes = [{ out: 3, inputs: [1, 2], time: 9 }];
  const r = C.runTrial(cfg, 7, 1500);
  assert.ok(r.conserved);
});

test('conservation holds for a three-input recipe', () => {
  const cfg = cfg3();
  const r = C.runTrial(cfg, 11, 1500);
  assert.ok(r.conserved);
  assert.ok(r.produced[4] > 0, 'something was actually built');
});

// --- invariant 2: worker state validity ---------------------------------------
test('no worker ever holds a product and raw inputs at once', () => {
  const cfg = cfgAB({ numCells: 6 });
  const st = C.createState(cfg, 3);
  for (let i = 0; i < 800; i++) {
    C.step(st, cfg);
    assert.ok(C.workerStatesValid(st), `violated at tick ${st.tick}`);
  }
});

// --- invariant 3: cell mutual exclusion ---------------------------------------
test('north takes the cell and south is blocked that tick', () => {
  const cfg = cfgAB({ numCells: 1, supply: fixedSupply(1) });
  const st = C.createState(cfg, 1);
  C.step(st, cfg);
  assert.equal(C.workerAt(st, cfg, C.NORTH, 0).held[1], 1);
  assert.equal(C.workerAt(st, cfg, C.SOUTH, 0).held[1], 0);
  assert.equal(st.belts[0][0], C.EMPTY, 'the part left the belt exactly once');
});

test('a blocked worker still advances an assembly in progress', () => {
  const cfg = cfgAB({ numCells: 1, supply: fixedSupply(1) });
  cfg.recipes = [{ out: 3, inputs: [1], time: 5 }];
  const st = C.createState(cfg, 1);
  const south = C.workerAt(st, cfg, C.SOUTH, 0);
  south.held[1] = 1;
  south.building = { ri: 0, remaining: 4 };   // mid-assembly when the tick starts
  C.step(st, cfg);                            // north wins the cell
  assert.equal(south.building.remaining, 3, 'south lost the cell but kept working');
});

// --- correctness: determinism -------------------------------------------------
test('same seed and config reproduce the run exactly', () => {
  const a = C.runTrial(cfgAB({ numCells: 5 }), 12345, 700);
  const b = C.runTrial(cfgAB({ numCells: 5 }), 12345, 700);
  assert.equal(a.efficiency, b.efficiency);
  assert.deepEqual(a.produced, b.produced);
  assert.deepEqual(Array.from(a.state.belts[0]), Array.from(b.state.belts[0]));
});

test('a different seed gives a different run', () => {
  const a = C.runTrial(cfgAB({ numCells: 5 }), 1, 700);
  const b = C.runTrial(cfgAB({ numCells: 5 }), 2, 700);
  assert.notDeepEqual(Array.from(a.state.belts[0]), Array.from(b.state.belts[0]));
});

// --- correctness: efficiency ---------------------------------------------------
test('faster assembly is at least as efficient', () => {
  const mk = time => {
    const cfg = cfgAB({ numCells: 6 });
    cfg.recipes = [{ out: 3, inputs: [1, 2], time }];
    return C.runTrial(cfg, 99, 3000).efficiency;
  };
  assert.ok(mk(1) > mk(8), `fast ${mk(1)} should beat slow ${mk(8)}`);
});

test('efficiency is a rate in [0,1]', () => {
  const r = C.runTrial(cfgAB({ numCells: 4 }), 5, 1000);
  assert.ok(r.efficiency >= 0 && r.efficiency <= 1, `got ${r.efficiency}`);
});

test.describe('simulation completes across the parameter grid', () => {
  for (const numCells of [1, 3, 8]) for (const time of [1, 3, 6]) {
    test(`cells=${numCells} time=${time}`, () => {
      const cfg = cfgAB({ numCells });
      cfg.recipes = [{ out: 3, inputs: [1, 2], time }];
      const r = C.runTrial(cfg, 4, 400);
      assert.ok(r.conserved);
      assert.equal(r.state.tick, 400);
    });
  }
});

// --- the question cl01's suite never asks: is north-first fair? ---------------
test('north-first priority skews output towards north', () => {
  const share = C.runTrial(cfgAB({ numCells: 6, priority: 'north' }), 21, 3000).sideShare;
  assert.ok(share[C.NORTH] > share[C.SOUTH],
    `expected a north bias, got ${JSON.stringify(share)}`);
});

test('alternating priority is measurably fairer than north-first', () => {
  const skew = p => {
    const s = C.runTrial(cfgAB({ numCells: 6, priority: p }), 21, 3000).sideShare;
    return Math.abs(s[C.NORTH] - s[C.SOUTH]);
  };
  assert.ok(skew('alternate') < skew('north'),
    `alternate ${skew('alternate')} should be flatter than north ${skew('north')}`);
});

test('alternating priority still conserves and still produces', () => {
  const r = C.runTrial(cfgAB({ numCells: 6, priority: 'alternate' }), 21, 1000);
  assert.ok(r.conserved);
  assert.ok(r.produced[3] > 0);
});

// --- generalisation: the hooks the expansions will use ------------------------
test('a custom supply policy replaces the random feed', () => {
  const cfg = cfgAB({ numCells: 2, sides: 1, supply: (_r, tick) => (tick % 2 ? 2 : 1) });
  const r = C.runTrial(cfg, 1, 600);
  assert.ok(r.conserved);
  assert.equal(r.state.stats.placed[C.EMPTY], 0, 'the hook is honoured: no gaps fed');
});

// A result worth locking down, because it looks like a bug and is not. A perfect
// gapless just-in-time feed DEADLOCKS the line: a finished product needs an empty
// cell to leave on, and a supply that never sends a gap never provides one. The idle
// cells in the random feed are not waste — they are the output channel. Any future
// storehouse or delivery-schedule policy has to reserve outbound capacity.
test('a gapless just-in-time feed deadlocks the line', () => {
  const cfg = cfgAB({ numCells: 6, sides: 2, supply: (_r, tick, c) => {
    const raw = C.rawParts(c);
    return raw[tick % raw.length];
  } });
  const r = C.runTrial(cfg, 1, 2000);
  assert.ok(r.conserved, 'a deadlocked line still conserves parts');
  assert.ok(r.state.belts.every(b => b.every(v => v !== C.EMPTY)), 'belts are saturated');
  assert.ok(r.state.workers.every(w => w.product !== C.EMPTY),
    'every worker is stuck holding a finished product');
  // only the startup transient, while the belt was still empty, ever shipped
  assert.ok(r.efficiency < 0.05, `throughput collapses, got ${r.efficiency}`);
});

test('two recipes can share a raw part', () => {
  const cfg = {
    numCells: 6, sides: 2, priority: 'north', supply: null,
    parts: [{ id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }, { id: 5 }],
    recipes: [
      { out: 4, inputs: [1, 2], time: 2 },
      { out: 5, inputs: [1, 3], time: 2 },
    ],
  };
  const r = C.runTrial(cfg, 8, 3000);
  assert.ok(r.conserved);
  assert.ok(r.produced[4] > 0 && r.produced[5] > 0, 'both products get built');
});

test('a recipe can consume two of the same part', () => {
  const cfg = {
    numCells: 3, sides: 1, priority: 'north', supply: fixedSupply(1),
    parts: [{ id: 1 }, { id: 2 }],
    recipes: [{ out: 2, inputs: [1, 1], time: 1 }],
  };
  const r = C.runTrial(cfg, 1, 400);
  assert.ok(r.conserved);
  assert.ok(r.produced[2] > 0, 'built from two of the same input');
  assert.equal(r.state.stats.consumed[1], r.produced[2] * 2);
});

test('a product can be another recipe input (multi-stage)', () => {
  // 1+2 -> 3 upstream; 3+1 -> 4 downstream, fed by whatever reaches the far cells.
  const cfg = {
    numCells: 10, sides: 2, priority: 'north', supply: null,
    parts: [{ id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
    recipes: [
      { out: 3, inputs: [1, 2], time: 1 },
      { out: 4, inputs: [3, 1], time: 1 },
    ],
  };
  const r = C.runTrial(cfg, 6, 4000);
  assert.ok(r.conserved, 'conservation survives a part being both output and input');
  assert.ok(r.produced[4] > 0, 'the second stage actually ran');
});

test('one-sided belts work (sides = 1)', () => {
  const r = C.runTrial(cfgAB({ numCells: 4, sides: 1 }), 2, 800);
  assert.ok(r.conserved);
  assert.equal(r.state.workers.length, 4);
  assert.equal(r.sideShare.length, 1);
});

// =============================================================================
// Parallel belts
// =============================================================================
test('a belt count builds the right number of workers and belts', () => {
  const cfg = cfgAB({ numCells: 4, numBelts: 3, sides: 2 });
  const st = C.createState(cfg, 1);
  assert.equal(st.belts.length, 3);
  assert.equal(st.workers.length, 3 * 2 * 4);
  // workerAt is belt-major: distinct belts must give distinct workers
  assert.notEqual(C.workerAt(st, cfg, C.NORTH, 0, 0), C.workerAt(st, cfg, C.NORTH, 0, 1));
  assert.equal(C.workerAt(st, cfg, C.NORTH, 0, 2).belt, 2);
});

test('belts default to one when a config omits the field', () => {
  const st = C.createState(cfg3(), 1);   // hand-built, no numBelts
  assert.equal(st.belts.length, 1);
});

test('output scales with belt count while efficiency stays flat', () => {
  const at = numBelts => {
    let eff = 0, built = 0;
    for (let s = 1; s <= 6; s++) {
      const r = C.runTrial(cfgAB({ numCells: 6, numBelts }), s, 2000);
      eff += r.efficiency / 6; built += r.produced[3] / 6;
    }
    return { eff, built };
  };
  const one = at(1), eight = at(8);
  assert.ok(Math.abs(one.eff - eight.eff) < 0.02,
    `efficiency is per belt-movement so it should not move: ${one.eff} vs ${eight.eff}`);
  assert.ok(eight.built > one.built * 7, 'eight belts build about eight times as much');
});

test('conservation holds across ten belts', () => {
  const r = C.runTrial(cfgAB({ numCells: 5, numBelts: C.MAX_BELTS }), 3, 1200);
  assert.ok(r.conserved);
  assert.equal(r.state.belts.length, 10);
});

// =============================================================================
// Several products, several part sets
// =============================================================================
test.describe('every preset runs, conserves and builds all its products', () => {
  for (const name of Object.keys(C.PRESETS)) {
    test(name, () => {
      const cfg = C.configFrom(name, { numCells: 8, numBelts: 2 });
      const r = C.runTrial(cfg, 5, 3000);
      assert.ok(r.conserved, 'parts are conserved');
      for (const m of r.productMix)
        assert.ok(m.built > 0, `product ${m.product} was never built`);
    });
  }
});

test('a preset with disjoint part sets keeps the two products independent', () => {
  const cfg = C.configFrom('twoLines', { numCells: 6 });
  assert.deepEqual(C.rawParts(cfg), [1, 2, 3, 4]);
  assert.deepEqual(cfg.recipes.map(r => r.out), [5, 6]);
});

// =============================================================================
// Worker specialisation
// =============================================================================
test('specialisation is a no-op when there is only one product', () => {
  const cfg = cfgAB({ numCells: 3, specialisation: 'byBelt' });
  const st = C.createState(cfg, 1);
  assert.ok(st.workers.every(w => w.recipes === null), 'nothing to specialise in');
});

test('a specialist ignores the other product parts entirely', () => {
  const cfg = C.configFrom('twoLines', { numCells: 3, numBelts: 2, specialisation: 'byBelt' });
  const st = C.createState(cfg, 1);
  const x = C.workerAt(st, cfg, C.NORTH, 0, 0);   // belt 0 -> recipe 0 -> parts A,B
  const y = C.workerAt(st, cfg, C.NORTH, 0, 1);   // belt 1 -> recipe 1 -> parts C,D
  assert.deepEqual(x.recipes, [0]);
  assert.deepEqual(y.recipes, [1]);
  assert.ok(C.wants(cfg, x, 1) && C.wants(cfg, x, 2), 'X-builder takes A and B');
  assert.ok(!C.wants(cfg, x, 3) && !C.wants(cfg, x, 4), 'and will not touch C or D');
  assert.ok(C.wants(cfg, y, 3) && C.wants(cfg, y, 4), 'Y-builder takes C and D');
  assert.ok(!C.wants(cfg, y, 1), 'and will not touch A');
});

test('each specialisation mode assigns along its own axis', () => {
  const mk = specialisation => C.createState(
    C.configFrom('twoLines', { numCells: 4, numBelts: 2, specialisation }), 1);
  const byBelt = mk('byBelt');
  assert.ok(byBelt.workers.every(w => w.recipes[0] === w.belt % 2));
  const bySide = mk('bySide');
  assert.ok(bySide.workers.every(w => w.recipes[0] === w.side % 2));
  const rr = mk('roundRobin');
  assert.ok(rr.workers.every(w => w.recipes[0] === (w.cell + w.side) % 2));
});

test('all specialisation modes still conserve', () => {
  for (const specialisation of ['none', 'byBelt', 'bySide', 'roundRobin']) {
    const cfg = C.configFrom('shared', { numCells: 6, numBelts: 2, specialisation });
    assert.ok(C.runTrial(cfg, 9, 1500).conserved, `${specialisation} leaked parts`);
  }
});

// The configuration trap: byBelt hands worker w the recipe (w.belt % nRecipes), so with
// fewer belts than products the later products get nobody and are never built at all.
// The UI warns about this; the test makes sure the warning stays true.
test('byBelt with fewer belts than products starves the extra products', () => {
  const cfg = C.configFrom('twoLines', { numCells: 6, numBelts: 1, specialisation: 'byBelt' });
  const r = C.runTrial(cfg, 5, 3000);
  assert.ok(r.conserved);
  assert.ok(r.productMix[0].built > 0, 'the first product is fine');
  assert.equal(r.productMix[1].built, 0, 'nobody on any belt is allowed to build it');
});

// Dedicating a line to a product does NOT pay unless the supply is dedicated too: the
// feed is undifferentiated, so half of what reaches a dedicated belt is parts its
// workers refuse to touch, and it rides straight off the end.
test('dedicated lines on a shared supply roughly halve throughput', () => {
  const eff = specialisation => {
    let e = 0;
    for (let s = 1; s <= 6; s++)
      e += C.runTrial(C.configFrom('twoLines',
        { numCells: 6, numBelts: 2, specialisation }), s, 2500).efficiency / 6;
    return e;
  };
  const generalist = eff('none'), dedicated = eff('byBelt');
  assert.ok(dedicated < generalist * 0.75,
    `dedicated ${dedicated.toFixed(3)} should fall well short of ${generalist.toFixed(3)}`);
});

// What specialisation buys, when it buys anything: control of the product mix.
// Generalists resolve a tie towards the first satisfiable recipe, so with two products
// competing for one shared part the first one crowds the other out.
test('specialisation balances the product mix that generalists skew', () => {
  const skew = specialisation => {
    let s = 0;
    for (let seed = 1; seed <= 6; seed++) {
      const m = C.runTrial(C.configFrom('shared',
        { numCells: 6, numBelts: 2, specialisation }), seed, 2500).productMix;
      const tot = m[0].built + m[1].built;
      s += tot ? Math.abs(m[0].built - m[1].built) / tot / 6 : 0;
    }
    return s;
  };
  const generalist = skew('none'), dedicated = skew('byBelt');
  assert.ok(generalist > 0.2, `generalists should skew, got ${generalist.toFixed(3)}`);
  assert.ok(dedicated < 0.05, `dedicated lines should even out, got ${dedicated.toFixed(3)}`);
});
