// Spatial-ecology CA core (shared by life-ecology.html and its Node tests).
// Contact-process prey: empty cells are colonised by adjacent prey at per-species
// rate beta; prey survive each tick with prob sigma. No B/S rule, no predation yet
// (MARTEN=3 reserved for slice 2). Pure: no DOM, no Three.js.
(function (root, factory) {
  const mod = factory();
  if (typeof module !== 'undefined' && module.exports) module.exports = mod;
  else root.LifeEco = mod;
})(typeof self !== 'undefined' ? self : this, function () {
  const STATES = { EMPTY: 0, RED: 1, GREY: 2, MARTEN: 3 };

  // Map a neighbour coord (possibly off-grid by the kernel radius) back onto the
  // grid per topology. 'straight' wraps, 'flip' wraps + mirrors the other axis
  // (Klein seam), 'none' is an open edge (off-grid -> null).
  function resolveCell(nc, nr, COLS, ROWS, Cwrap, Rwrap) {
    if (nc < 0 || nc >= COLS) {
      if (Cwrap === 'none') return null;
      if (Cwrap === 'flip') nr = ROWS - 1 - nr;
      nc = (nc + COLS) % COLS;
    }
    if (nr < 0 || nr >= ROWS) {
      if (Rwrap === 'none') return null;
      if (Rwrap === 'flip') nc = COLS - 1 - nc;
      nr = (nr + ROWS) % ROWS;
    }
    return nr * COLS + nc;
  }

  const MOORE = [[-1,-1],[0,-1],[1,-1],[-1,0],[1,0],[-1,1],[0,1],[1,1]];

  function hasEmptyNeighbour(grid, c, r, COLS, ROWS, Cwrap, Rwrap) {
    for (const [dc, dr] of MOORE) {
      const i = resolveCell(c + dc, r + dr, COLS, ROWS, Cwrap, Rwrap);
      if (i !== null && grid[i] === STATES.EMPTY) return true;
    }
    return false;
  }

  function countMartenNeighbours(grid, c, r, COLS, ROWS, Cwrap, Rwrap) {
    let n = 0;
    for (const [dc, dr] of MOORE) {
      const i = resolveCell(c + dc, r + dr, COLS, ROWS, Cwrap, Rwrap);
      if (i !== null && grid[i] === STATES.MARTEN) n++;
    }
    return n;
  }

  function countInfectedNeighbours(grid, infected, c, r, COLS, ROWS, Cwrap, Rwrap) {
    let n = 0;
    for (const [dc, dr] of MOORE) {
      const i = resolveCell(c + dc, r + dr, COLS, ROWS, Cwrap, Rwrap);
      // Both species transmit. Greys are the reservoir that keeps it alive, but reds
      // do pass it to each other during an outbreak — they just die too fast to
      // sustain it on their own.
      if (i !== null && infected[i] && (grid[i] === STATES.RED || grid[i] === STATES.GREY)) n++;
    }
    return n;
  }

  // One generation. Prey grow by contact process; the marten is a conserved
  // predator with an energy counter. Two phases with an intent buffer (claim[]):
  // (1) each marten claims its highest-priority prey neighbour; (2) each prey is
  // eaten by the highest-priority marten that claimed it (unless it evades), so
  // one prey dies at most once and one marten eats at most once. Missed hunts
  // (lost the arbitration, or prey evaded) feed nobody.
  function stepEcology(grid, energy, infected, COLS, ROWS, Cwrap, Rwrap, params, rng) {
    const N = COLS * ROWS;
    const { betaRed, betaGrey, sigma,
            delta = 1, g = 3, eBreed = 10, e0 = 5, breedCost = 5, eCap = 15, mu = 0.5,
            evadeRed = 0, evadeGrey = 0, forage = 0, mate = false, mortality = 0,
            poxBeta = 0, poxLethal = 0, gen = 0 } = params;
    const { EMPTY, RED, GREY, MARTEN } = STATES;
    const nextGrid = new Uint8Array(N), nextEnergy = new Float32Array(N);
    const nextInfected = new Uint8Array(N);
    const isPrey = v => v === RED || v === GREY;

    // deterministic arbitration priority for the ordered pair (marten, prey, gen)
    function prio(mIdx, pIdx) {
      let h = (Math.imul(mIdx + 1, 1973) ^ Math.imul(pIdx + 1, 9277) ^ Math.imul(gen + 1, 26699)) >>> 0;
      h ^= h >>> 16; h = Math.imul(h, 0x7feb352d) >>> 0; h ^= h >>> 15;
      h = Math.imul(h, 0x846ca68b) >>> 0; h ^= h >>> 16;
      return h >>> 0;
    }

    // PHASE 1: each hungry marten claims one prey neighbour (highest priority).
    const claim = new Int32Array(N).fill(-1);
    for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) {
      const m = r * COLS + c;
      if (grid[m] !== MARTEN || energy[m] >= eCap) continue;   // sated martens rest
      let best = -1, bestP = 0;
      for (const [dc, dr] of MOORE) {
        const i = resolveCell(c + dc, r + dr, COLS, ROWS, Cwrap, Rwrap);
        if (i === null || !isPrey(grid[i])) continue;
        // NB: prio() takes (marten, prey) here and in phase 2a. Both directions must
        // pass the same ordered pair or the two phases disagree on the winner.
        const pr = prio(m, i);
        if (best < 0 || pr > bestP) { best = i; bestP = pr; }
      }
      claim[m] = best;
    }

    // PHASE 2a: resolve who eats whom. A prey's winner is the highest-priority
    // marten that claimed it; the prey is eaten unless it evades.
    const eaten = new Uint8Array(N), ate = new Uint8Array(N);
    for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) {
      const p = r * COLS + c;
      if (!isPrey(grid[p])) continue;
      let winner = -1, bestP = 0;
      for (const [dc, dr] of MOORE) {
        const i = resolveCell(c + dc, r + dr, COLS, ROWS, Cwrap, Rwrap);
        if (i === null || grid[i] !== MARTEN || claim[i] !== p) continue;
        const pr = prio(i, p);
        if (winner < 0 || pr > bestP) { winner = i; bestP = pr; }
      }
      if (winner < 0) continue;
      const evade = grid[p] === RED ? evadeRed : evadeGrey;
      if (evade > 0 && rng() < evade) continue;   // escaped; winner still spent its hunt
      eaten[p] = 1; ate[winner] = 1;
    }

    // Breeding eligibility, computed once because both the parent (which pays the
    // breed cost) and the empty cell (which counts eligible neighbours) must agree.
    // With `mate`, a marten also needs an adjacent marten: one animal cannot found a
    // population, so a reintroduction needs a viable *group* rather than a lucky
    // individual. It pulls against forage, which is divided by that same crowding —
    // so martens can only breed where prey is rich enough to pay for the company.
    const canBreed = new Uint8Array(N);
    for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) {
      const i = r * COLS + c;
      if (grid[i] !== MARTEN || energy[i] < eBreed) continue;
      if (!mate || countMartenNeighbours(grid, c, r, COLS, ROWS, Cwrap, Rwrap) > 0) canBreed[i] = 1;
    }

    // PHASE 2b: build the next grid + energy.
    for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) {
      const here = r * COLS + c, v = grid[here];
      if (v === MARTEN) {
        // Background mortality: age, accident, disease. Death that the energy ledger
        // cannot express — without it a marten that breaks even never dies at all,
        // and small founder groups never fail the way real ones do.
        if (mortality > 0 && rng() < mortality) { nextGrid[here] = EMPTY; nextEnergy[here] = 0; continue; }
        let e = energy[here] - delta + (ate[here] ? g : 0);
        // Alternative prey (voles, birds, berries). It must be a SHARED resource:
        // a flat term would be algebraically identical to lowering delta, so it
        // would add no dynamics at all. Dividing by the local marten count makes it
        // saturate with crowding, which is what gives martens a baseline density
        // set by the background food rather than by squirrels. forage=0 disables.
        if (forage > 0) e += forage / (1 + countMartenNeighbours(grid, c, r, COLS, ROWS, Cwrap, Rwrap));
        // ponytail: flat breed cost when eligible AND could seed an empty neighbour;
        // exact per-offspring charge needs radius-2, not worth it for the dynamics.
        if (canBreed[here] && hasEmptyNeighbour(grid, c, r, COLS, ROWS, Cwrap, Rwrap)) e -= breedCost;
        if (e <= 0) { nextGrid[here] = EMPTY; nextEnergy[here] = 0; }
        else { nextGrid[here] = MARTEN; nextEnergy[here] = e < eCap ? e : eCap; }
        continue;
      }
      if (isPrey(v)) {
        // Same short-circuit as before: an eaten prey never rolls for survival, so
        // the rng stream is untouched when pox is off.
        if (!(!eaten[here] && rng() < sigma)) { nextGrid[here] = EMPTY; continue; }
        nextGrid[here] = v;
        if (poxBeta > 0) {
          // Squirrelpox. Greys are an asymptomatic reservoir: they carry it for life
          // and pay nothing. Reds are killed by it. That asymmetry runs opposite to
          // the evasion asymmetry, which is the whole point of the mechanic.
          let inf = infected[here];
          if (!inf) {
            const nInf = countInfectedNeighbours(grid, infected, c, r, COLS, ROWS, Cwrap, Rwrap);
            if (nInf && rng() < 1 - Math.pow(1 - poxBeta, nInf)) inf = 1;
          }
          if (inf && v === RED && rng() < poxLethal) { nextGrid[here] = EMPTY; continue; }
          nextInfected[here] = inf;
        }
        continue;
      }
      // EMPTY: contact-process prey colonisation vs marten breeding spill, one roll.
      let nRed = 0, nGrey = 0, nElig = 0;
      for (const [dc, dr] of MOORE) {
        const i = resolveCell(c + dc, r + dr, COLS, ROWS, Cwrap, Rwrap);
        if (i === null) continue;
        if (grid[i] === RED) nRed++;
        else if (grid[i] === GREY) nGrey++;
        else if (grid[i] === MARTEN && canBreed[i]) nElig++;
      }
      const pRed  = nRed  ? 1 - Math.pow(1 - betaRed,  nRed)  : 0;
      const pGrey = nGrey ? 1 - Math.pow(1 - betaGrey, nGrey) : 0;
      const pMart = nElig ? 1 - Math.pow(1 - mu,       nElig) : 0;
      // one draw partitions the empty cell: red slice first, then grey, then marten.
      // ponytail: red-first is a fixed convention; grey still wins via higher
      // beta when reds are scarce, which is the invasive-advantage story.
      const roll = rng();
      if (roll < pRed) nextGrid[here] = RED;
      else if (roll < pRed + pGrey) nextGrid[here] = GREY;
      else if (roll < pRed + pGrey + pMart) { nextGrid[here] = MARTEN; nextEnergy[here] = e0; }
      else nextGrid[here] = EMPTY;
    }
    // Newborn prey are never born infected: nextInfected starts zeroed and only
    // surviving carriers write to it. No vertical transmission — a grey born beside
    // infected greys simply catches it next tick.
    return { grid: nextGrid, energy: nextEnergy, infected: nextInfected };
  }

  return { STATES, resolveCell, stepEcology, hasEmptyNeighbour };
});
