// Conveyor-assembly simulation core (shared by life-conveyor.html and its Node tests).
// Port of cl01/simulate_conveyor.py, generalised from the fixed A+B->Product assignment
// to an arbitrary part/recipe space so that more subparts, more products, multi-stage
// assembly and alternative supply policies are configuration rather than new code.
// Pure: no DOM, no timers, no I/O.
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

  // ---- configuration ---------------------------------------------------------
  // A part is an integer id; 0 is EMPTY and is never a part. A part is *raw* if no
  // recipe produces it — raw parts are what the supply puts on the belt. A recipe's
  // output is itself a part, so a product can be another recipe's input and
  // multi-stage assembly needs no new concept.
  //
  // `time` is the Python `assembly_time` and carries its exact semantics: assembly is
  // stepped once on the tick the last input is picked up, so a product appears
  // `time + 1` ticks after that pickup. Preserved deliberately — cl01's
  // test_assembly_time_respected pins it.
  function defaultConfig() {
    return {
      numCells: 3,
      sides: 2,
      parts: [
        { id: 1, name: 'Part A',  char: 'A', color: '#e0533a' },
        { id: 2, name: 'Part B',  char: 'B', color: '#4a90d0' },
        { id: 3, name: 'Product', char: 'P', color: '#7fd18c' },
      ],
      recipes: [{ out: 3, inputs: [1, 2], time: 3 }],
      // 'north' reproduces cl01 exactly: the north worker always gets first refusal on
      // a cell. 'alternate' flips the winner by tick parity — there to make the
      // fairness question measurable, since nothing in cl01's suite tests it.
      priority: 'north',
      // null = uniform over EMPTY plus every raw part, which is cl01's
      // `Content(random.randrange(0, 3))`. A storehouse or a just-in-time delivery
      // schedule is a different function with the same signature: (rng, tick, cfg) -> partId.
      supply: null,
    };
  }

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
  function makeWorker(cfg, side, cell) {
    return {
      side, cell,
      held: new Int32Array(maxPartId(cfg) + 1), // held[partId] = count
      product: EMPTY,                           // finished item awaiting a free cell
      building: null,                           // { ri, remaining }
      made: 0,
      idle: 0,                                  // ticks spent doing nothing at all
    };
  }

  function createState(cfg, seed) {
    const n = maxPartId(cfg) + 1;
    const st = {
      cells: new Int32Array(cfg.numCells),      // all EMPTY
      workers: [],
      tick: 0,
      rng: mulberry32(seed >>> 0),
      seed: seed >>> 0,
      stats: {
        placed: new Int32Array(n),    // put on the belt by the supply
        removed: new Int32Array(n),   // fell off the far end
        consumed: new Int32Array(n),  // absorbed into a finished product
        produced: new Int32Array(n),  // completed by a worker
        madeBySide: new Int32Array(cfg.sides),
      },
    };
    for (let side = 0; side < cfg.sides; side++)
      for (let c = 0; c < cfg.numCells; c++) st.workers.push(makeWorker(cfg, side, c));
    return st;
  }

  const workerAt = (st, cfg, side, cell) => st.workers[side * cfg.numCells + cell];

  // ---- worker logic ----------------------------------------------------------
  /** Recipes this worker may build. Absent `w.recipes`, all of them (no specialists yet). */
  function recipesOf(cfg, w) {
    return w.recipes ? w.recipes.map(i => cfg.recipes[i]) : cfg.recipes;
  }

  function needed(recipe, part) {
    let n = 0;
    for (const p of recipe.inputs) if (p === part) n++;
    return n;
  }

  /** True if some buildable recipe still wants another `part`. Generalises cl01's
   *  `not worker.has_a` — with a multiset of inputs, "another one" is a count test. */
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
  function act(st, cfg, w, mayUseCell) {
    const i = w.cell;
    if (mayUseCell) {
      if (st.cells[i] === EMPTY && w.product !== EMPTY) {
        st.cells[i] = w.product;
        w.product = EMPTY;
        return true;
      }
      if (!w.building && w.product === EMPTY && wants(cfg, w, st.cells[i])) {
        const part = st.cells[i];
        st.cells[i] = EMPTY;
        w.held[part]++;
        const ri = startable(cfg, w);
        if (ri >= 0) beginAssembly(st, cfg, w, ri);
        return true;
      }
    }
    if (w.building) stepAssembly(st, cfg, w);
    else if (w.product === EMPTY) w.idle++;
    return false;
  }

  /** Side order for this cell this tick. 'north' = cl01. */
  function sideOrder(cfg, tick, cellIdx) {
    if (cfg.sides < 2) return [NORTH];
    if (cfg.priority === 'alternate') return (tick + cellIdx) % 2 ? [SOUTH, NORTH] : [NORTH, SOUTH];
    return [NORTH, SOUTH];
  }

  // ---- tick ------------------------------------------------------------------
  /** Shift the belt one cell and feed a new one at the head. */
  function advance(st, cfg) {
    const off = st.cells[cfg.numCells - 1];
    st.stats.removed[off]++;
    for (let i = cfg.numCells - 1; i > 0; i--) st.cells[i] = st.cells[i - 1];
    const supply = cfg.supply || defaultSupply;
    const incoming = supply(st.rng, st.tick, cfg);
    st.cells[0] = incoming;
    st.stats.placed[incoming]++;
  }

  function processCells(st, cfg) {
    for (let i = 0; i < cfg.numCells; i++) {
      let free = true;
      for (const side of sideOrder(cfg, st.tick, i)) {
        const used = act(st, cfg, workerAt(st, cfg, side, i), free);
        if (used) free = false;
      }
    }
  }

  /** One conveyor movement plus the worker round it triggers. */
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

  function onBelt(st, cfg) {
    const belt = new Int32Array(maxPartId(cfg) + 1);
    for (let i = 0; i < cfg.numCells; i++) if (st.cells[i] !== EMPTY) belt[st.cells[i]]++;
    return belt;
  }

  /**
   * Conservation ledger, one row per part. Everything the supply put on the belt must
   * still be somewhere: fallen off the end, on the belt, in a hand, or absorbed into a
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

  /** Products completed per movement — cl01's efficiency ratio. */
  function efficiency(st, cfg) {
    const total = cfg.recipes.reduce((s, r) => s + st.stats.produced[r.out], 0);
    return total / (st.tick + cfg.numCells);
  }

  /** Share of output made by each side. Flat 0.5/0.5 means the priority rule is fair. */
  function sideShare(st, cfg) {
    const total = Array.from(st.stats.madeBySide).reduce((a, b) => a + b, 0);
    if (!total) return new Array(cfg.sides).fill(0);
    return Array.from(st.stats.madeBySide, n => n / total);
  }

  /** Headless run. Returns the scalars a parameter sweep wants, nothing else. */
  function runTrial(cfg, seed, iterations) {
    const st = createState(cfg, seed);
    for (let i = 0; i < iterations; i++) step(st, cfg);
    return {
      efficiency: efficiency(st, cfg),
      produced: Array.from(st.stats.produced),
      sideShare: sideShare(st, cfg),
      conserved: conservation(st, cfg).every(r => r.delta === 0),
      state: st,
    };
  }

  return {
    EMPTY, NORTH, SOUTH,
    mulberry32, defaultConfig, defaultSupply,
    rawParts, partById, partIds, maxPartId,
    createState, workerAt, makeWorker,
    wants, startable, act, sideOrder,
    advance, processCells, step,
    onBelt, heldByWorkers, conservation, workerStatesValid,
    efficiency, sideShare, runTrial,
  };
});
