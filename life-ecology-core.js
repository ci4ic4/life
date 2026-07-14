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

  function stepEcology(grid, COLS, ROWS, Cwrap, Rwrap, params, rng) {
    const { betaRed, betaGrey, sigma } = params;
    const next = new Uint8Array(COLS * ROWS);
    for (let r = 0; r < ROWS; r++) {
      for (let c = 0; c < COLS; c++) {
        const here = r * COLS + c, v = grid[here];
        if (v === STATES.RED || v === STATES.GREY) {
          // natural survival (predation added in slice 2)
          next[here] = rng() < sigma ? v : STATES.EMPTY;
          continue;
        }
        // EMPTY: contact-process colonisation from prey neighbours
        let nRed = 0, nGrey = 0;
        for (const [dc, dr] of MOORE) {
          const i = resolveCell(c + dc, r + dr, COLS, ROWS, Cwrap, Rwrap);
          if (i === null) continue;
          if (grid[i] === STATES.RED) nRed++;
          else if (grid[i] === STATES.GREY) nGrey++;
        }
        const pRed  = nRed  ? 1 - Math.pow(1 - betaRed,  nRed)  : 0;
        const pGrey = nGrey ? 1 - Math.pow(1 - betaGrey, nGrey) : 0;
        // one draw partitions the empty cell: red slice first, then grey.
        // ponytail: red-first is a fixed convention; grey still wins via higher
        // beta when reds are scarce, which is the invasive-advantage story.
        const roll = rng();
        if (roll < pRed) next[here] = STATES.RED;
        else if (roll < pRed + pGrey) next[here] = STATES.GREY;
        else next[here] = STATES.EMPTY;
      }
    }
    return next;
  }

  return { STATES, resolveCell, stepEcology };
});
