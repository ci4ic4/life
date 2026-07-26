// Conveyor-assembly simulation core (shared by life-conveyor.html and its Node tests).
// Port of cl01/simulate_conveyor.py, generalised from the fixed A+B->Product assignment
// to an arbitrary part/recipe space, several parallel belts, and workers specialised to
// one product. Pure: no DOM, no timers, no I/O.
//
// The shape is deliberately the same as life-ecology-core.js: a lattice of cells, agents
// bound to cells, a contested resource resolved by a priority rule, a conserved quantity,
// and a seeded stream so a run replays exactly. Only the lattice is 1-D and it translates.
(function (root, factory) {
  const mod = factory();
  if (typeof module !== 'undefined' && module.exports) module.exports = mod;
  else root.LifeConveyor = mod;
})(typeof self !== 'undefined' ? self : this, function () {

  const EMPTY = 0;
  const NORTH = 0, SOUTH = 1;
  const MAX_BELTS = 10;

  // Same generator as life-ecology, so a saved scenario replays identically in the
  // browser and under `node --test`.
  function mulberry32(a) {
    return function () {
      a |= 0; a = (a + 0x6D2B79F5) | 0;
      let t = Math.imul(a ^ (a >>> 15), 1 | a);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  // ---- parts and products ----------------------------------------------------
  // A part is an integer id; 0 is EMPTY and is never a part. A part is *raw* if no
  // recipe produces it — raw parts are what the supply puts on the belts. A recipe's
  // output is itself a part, so a product can be another recipe's input and
  // multi-stage assembly needs no new concept.
  //
  // `time` carries the Python `assembly_time` semantics exactly: assembly is stepped
  // once on the tick the last input is picked up, so a product appears `time` ticks
  // after that pickup. cl01's test_assembly_time_respected pins it.
  const P = (id, name, char, color) => ({ id, name, char, color });

  /** Named product mixes. Each is a complete parts+recipes pair; `time` is set by the UI. */
  const PRESETS = {
    simple: {
      label: 'One product — A + B',
      parts: [P(1, 'Part A', 'A', '#e0533a'), P(2, 'Part B', 'B', '#4a90d0'),
              P(3, 'Product', 'P', '#7fd18c')],
      recipes: [{ out: 3, inputs: [1, 2], time: 3 }],
    },
    twoLines: {
      label: 'Two products, separate parts',
      parts: [P(1, 'Part A', 'A', '#e0533a'), P(2, 'Part B', 'B', '#4a90d0'),
              P(3, 'Part C', 'C', '#d9a441'), P(4, 'Part D', 'D', '#a06cd0'),
              P(5, 'Product X', 'X', '#7fd18c'), P(6, 'Product Y', 'Y', '#56b6c2')],
      recipes: [{ out: 5, inputs: [1, 2], time: 3 }, { out: 6, inputs: [3, 4], time: 3 }],
    },
    shared: {
      label: 'Two products sharing part A',
      parts: [P(1, 'Part A', 'A', '#e0533a'), P(2, 'Part B', 'B', '#4a90d0'),
              P(3, 'Part C', 'C', '#d9a441'),
              P(4, 'Product X', 'X', '#7fd18c'), P(5, 'Product Y', 'Y', '#56b6c2')],
      recipes: [{ out: 4, inputs: [1, 2], time: 3 }, { out: 5, inputs: [1, 3], time: 3 }],
    },
    lopsided: {
      label: 'A three-part product and an easy one',
      parts: [P(1, 'Part A', 'A', '#e0533a'), P(2, 'Part B', 'B', '#4a90d0'),
              P(3, 'Part C', 'C', '#d9a441'), P(4, 'Part D', 'D', '#a06cd0'),
              P(5, 'Product X', 'X', '#7fd18c'), P(6, 'Product Y', 'Y', '#56b6c2')],
      recipes: [{ out: 5, inputs: [1, 2, 3], time: 3 }, { out: 6, inputs: [3, 4], time: 3 }],
    },
  };

  /** A config from a preset name. Everything else is a plain field the caller may edit. */
  function configFrom(preset, over) {
    const p = PRESETS[preset] || PRESETS.simple;
    return Object.assign({
      preset,
      numCells: 3,
      numBelts: 1,
      sides: 2,
      parts: p.parts.map(x => Object.assign({}, x)),
      recipes: p.recipes.map(r => Object.assign({}, r, { inputs: r.inputs.slice() })),
      // 'north' reproduces cl01 exactly: the north worker always gets first refusal on
      // a cell. 'alternate' flips the winner by tick parity — there to make the
      // fairness question measurable, since nothing in cl01's suite tests it.
      priority: 'north',
      // Which product each worker is allowed to build. 'none' = every worker builds
      // anything, which is cl01. See assignSpecialisation.
      specialisation: 'none',
      // null = uniform over EMPTY plus every raw part, which is cl01's
      // `Content(random.randrange(0, 3))`. A storehouse or a just-in-time delivery
      // schedule is a different function with the same signature:
      // (rng, tick, cfg, beltIdx) -> partId.
      supply: null,
    }, over || {});
  }

  const defaultConfig = () => configFrom('simple');

  /** Belt count, tolerating hand-built configs that omit it. */
  const nBelts = cfg => cfg.numBelts || 1;

  const partIds = cfg => cfg.parts.map(p => p.id);
  const maxPartId = cfg => partIds(cfg).reduce((m, id) => Math.max(m, id), 0);

  /** Parts no recipe produces — the ones that must arrive from outside. */
  function rawParts(cfg) {
    const made = new Set(cfg.recipes.map(r => r.out));
    return partIds(cfg).filter(id => !made.has(id));
  }

  function partById(cfg, id) {
    return cfg.parts.find(p => p.id === id) || null;
  }

  /** Uniform over EMPTY + raw parts. Matches cl01 when there are exactly two raws. */
  function defaultSupply(rng, _tick, cfg) {
    const raw = rawParts(cfg);
    const k = Math.floor(rng() * (raw.length + 1));
    return k === 0 ? EMPTY : raw[k - 1];
  }

  // ---- state -----------------------------------------------------------------
  function makeWorker(cfg, belt, side, cell) {
    return {
      belt, side, cell,
      held: new Int32Array(maxPartId(cfg) + 1), // held[partId] = count
      product: EMPTY,                           // finished item awaiting a free cell
      building: null,                           // { ri, remaining }
      recipes: null,                            // null = may build anything
      made: 0,
    };
  }

  /**
   * Give each worker its product. A specialist picks up only the parts its own recipe
   * needs — that falls out of `wants()` consulting `recipesOf`, so no extra rule.
   *   byBelt     — one belt per product: dedicated lines.
   *   bySide     — north builds one product, south the other: shared line, split staff.
   *   roundRobin — alternating along and across each belt: fully mixed line.
   * With one recipe there is nothing to specialise in and every mode is a no-op.
   */
  function assignSpecialisation(cfg, st) {
    const n = cfg.recipes.length;
    const mode = cfg.specialisation;
    if (n < 2 || !mode || mode === 'none') {
      for (const w of st.workers) w.recipes = null;
      return;
    }
    for (const w of st.workers) {
      let r;
      if (mode === 'byBelt') r = w.belt % n;
      else if (mode === 'bySide') r = w.side % n;
      else r = (w.cell + w.side) % n;
      w.recipes = [r];
    }
  }

  function createState(cfg, seed) {
    const n = maxPartId(cfg) + 1;
    const belts = [];
    for (let b = 0; b < nBelts(cfg); b++) belts.push(new Int32Array(cfg.numCells));
    const st = {
      belts,
      workers: [],
      tick: 0,
      rng: mulberry32(seed >>> 0),
      seed: seed >>> 0,
      stats: {
        placed: new Int32Array(n),    // put on a belt by the supply
        removed: new Int32Array(n),   // fell off a far end
        consumed: new Int32Array(n),  // absorbed into a finished product
        produced: new Int32Array(n),  // completed by a worker
        madeBySide: new Int32Array(cfg.sides),
        madeByBelt: new Int32Array(nBelts(cfg)),
      },
    };
    for (let b = 0; b < nBelts(cfg); b++)
      for (let side = 0; side < cfg.sides; side++)
        for (let c = 0; c < cfg.numCells; c++) st.workers.push(makeWorker(cfg, b, side, c));
    assignSpecialisation(cfg, st);
    return st;
  }

  /** Workers are laid out belt-major, then side, then cell. */
  const workerAt = (st, cfg, side, cell, belt) =>
    st.workers[((belt || 0) * cfg.sides + side) * cfg.numCells + cell];

  // ---- worker logic ----------------------------------------------------------
  /** Recipes this worker may build — all of them unless it has been specialised. */
  function recipesOf(cfg, w) {
    return w.recipes ? w.recipes.map(i => cfg.recipes[i]) : cfg.recipes;
  }

  function needed(recipe, part) {
    let n = 0;
    for (const p of recipe.inputs) if (p === part) n++;
    return n;
  }

  /** True if some buildable recipe still wants another `part`. Generalises cl01's
   *  `not worker.has_a` — with a multiset of inputs, "another one" is a count test.
   *  A specialist consults only its own recipe, so it ignores other products' parts. */
  function wants(cfg, w, part) {
    if (part === EMPTY) return false;
    for (const r of recipesOf(cfg, w)) if (w.held[part] < needed(r, part)) return true;
    return false;
  }

  /** Index into cfg.recipes of a recipe whose inputs are fully held, else -1. */
  function startable(cfg, w) {
    for (let i = 0; i < cfg.recipes.length; i++) {
      if (w.recipes && !w.recipes.includes(i)) continue;
      const r = cfg.recipes[i];
      let ok = true;
      for (const p of new Set(r.inputs)) if (w.held[p] < needed(r, p)) { ok = false; break; }
      if (ok) return i;
    }
    return -1;
  }

  // Inputs stay in `held` for the whole assembly and are consumed at completion — that
  // is cl01's behaviour (has_a/has_b cleared in the done branch) and it keeps the
  // "still held by workers" census honest while a build is in flight.
  function stepAssembly(st, cfg, w) {
    const r = cfg.recipes[w.building.ri];
    if (w.building.remaining === 0) {
      for (const p of r.inputs) { w.held[p]--; st.stats.consumed[p]++; }
      w.product = r.out;
      w.building = null;
      w.made++;
      st.stats.produced[r.out]++;
      st.stats.madeBySide[w.side]++;
      st.stats.madeByBelt[w.belt]++;
    } else {
      w.building.remaining--;
    }
  }

  function beginAssembly(st, cfg, w, ri) {
    w.building = { ri, remaining: cfg.recipes[ri].time };
    stepAssembly(st, cfg, w); // cl01 steps on the pickup tick
  }

  /**
   * One worker's turn at its cell. `mayUseCell` is the mutual-exclusion grant.
   * Returns true if it consumed the cell (took a part or placed a product), which is
   * what blocks the other side. Continuing an assembly never blocks — and a blocked
   * worker may still continue one, exactly as in cl01.
   */
  function act(st, cfg, belt, w, mayUseCell) {
    const i = w.cell;
    if (mayUseCell) {
      if (belt[i] === EMPTY && w.product !== EMPTY) {
        belt[i] = w.product;
        w.product = EMPTY;
        return true;
      }
      if (!w.building && w.product === EMPTY && wants(cfg, w, belt[i])) {
        const part = belt[i];
        belt[i] = EMPTY;
        w.held[part]++;
        const ri = startable(cfg, w);
        if (ri >= 0) beginAssembly(st, cfg, w, ri);
        return true;
      }
    }
    if (w.building) stepAssembly(st, cfg, w);
    return false;
  }

  /** Side order for this cell this tick. 'north' = cl01. */
  function sideOrder(cfg, tick, cellIdx) {
    if (cfg.sides < 2) return [NORTH];
    if (cfg.priority === 'alternate') return (tick + cellIdx) % 2 ? [SOUTH, NORTH] : [NORTH, SOUTH];
    return [NORTH, SOUTH];
  }

  // ---- tick ------------------------------------------------------------------
  /** Shift every belt one cell and feed a new one at each head. */
  function advance(st, cfg) {
    const supply = cfg.supply || defaultSupply;
    for (let b = 0; b < nBelts(cfg); b++) {
      const belt = st.belts[b];
      st.stats.removed[belt[cfg.numCells - 1]]++;
      for (let i = cfg.numCells - 1; i > 0; i--) belt[i] = belt[i - 1];
      const incoming = supply(st.rng, st.tick, cfg, b);
      belt[0] = incoming;
      st.stats.placed[incoming]++;
    }
  }

  function processCells(st, cfg) {
    for (let b = 0; b < nBelts(cfg); b++) {
      const belt = st.belts[b];
      for (let i = 0; i < cfg.numCells; i++) {
        let free = true;
        for (const side of sideOrder(cfg, st.tick, i)) {
          if (act(st, cfg, belt, workerAt(st, cfg, side, i, b), free)) free = false;
        }
      }
    }
  }

  /** One conveyor movement plus the worker round it triggers, on every belt. */
  function step(st, cfg) {
    advance(st, cfg);
    processCells(st, cfg);
    st.tick++;
  }

  // ---- reporting -------------------------------------------------------------
  /** Parts sitting in workers' hands right now (inputs held + finished products). */
  function heldByWorkers(st, cfg) {
    const held = new Int32Array(maxPartId(cfg) + 1);
    for (const w of st.workers) {
      for (let p = 1; p < held.length; p++) held[p] += w.held[p];
      if (w.product !== EMPTY) held[w.product]++;
    }
    return held;
  }

  /** Parts on all belts. */
  function onBelt(st, cfg) {
    const belt = new Int32Array(maxPartId(cfg) + 1);
    for (const b of st.belts)
      for (let i = 0; i < cfg.numCells; i++) if (b[i] !== EMPTY) belt[b[i]]++;
    return belt;
  }

  /**
   * Conservation ledger, one row per part. Everything the supply put on a belt must
   * still be somewhere: fallen off an end, on a belt, in a hand, or absorbed into a
   * product. `delta` must be 0 for every part — this is cl01 invariant 1, generalised.
   */
  function conservation(st, cfg) {
    const belt = onBelt(st, cfg), held = heldByWorkers(st, cfg);
    const rows = [];
    for (const id of partIds(cfg)) {
      const inflow = st.stats.placed[id] + st.stats.produced[id];
      const outflow = st.stats.removed[id] + belt[id] + held[id] + st.stats.consumed[id];
      rows.push({ part: id, inflow, outflow, delta: inflow - outflow });
    }
    return rows;
  }

  /** cl01 invariant 2: no worker holds a finished product and raw inputs at once. */
  function workerStatesValid(st) {
    return st.workers.every(w => {
      if (w.product === EMPTY) return true;
      return w.held.every(c => c === 0);
    });
  }

  /** Products completed per belt-movement, so the number is comparable across belt counts. */
  function efficiency(st, cfg) {
    const total = cfg.recipes.reduce((s, r) => s + st.stats.produced[r.out], 0);
    return total / (nBelts(cfg) * (st.tick + cfg.numCells));
  }

  /** Share of output made by each side. Flat means the priority rule is fair. */
  function sideShare(st, cfg) {
    const total = Array.from(st.stats.madeBySide).reduce((a, b) => a + b, 0);
    if (!total) return new Array(cfg.sides).fill(0);
    return Array.from(st.stats.madeBySide, n => n / total);
  }

  /** How many of each product got built. Uneven output is the interesting failure. */
  function productMix(st, cfg) {
    return cfg.recipes.map(r => ({ product: r.out, built: st.stats.produced[r.out] }));
  }

  /** Headless run. Returns the scalars a parameter sweep wants, plus the final state. */
  function runTrial(cfg, seed, iterations) {
    const st = createState(cfg, seed);
    for (let i = 0; i < iterations; i++) step(st, cfg);
    return {
      efficiency: efficiency(st, cfg),
      produced: Array.from(st.stats.produced),
      productMix: productMix(st, cfg),
      sideShare: sideShare(st, cfg),
      conserved: conservation(st, cfg).every(r => r.delta === 0),
      state: st,
    };
  }

  return {
    EMPTY, NORTH, SOUTH, MAX_BELTS, PRESETS, nBelts,
    mulberry32, defaultConfig, configFrom, defaultSupply,
    rawParts, partById, partIds, maxPartId,
    createState, workerAt, makeWorker, assignSpecialisation, recipesOf,
    wants, startable, act, sideOrder,
    advance, processCells, step,
    onBelt, heldByWorkers, conservation, workerStatesValid,
    efficiency, sideShare, productMix, runTrial,
  };
});
