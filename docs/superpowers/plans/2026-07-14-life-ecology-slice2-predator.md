# Life-Ecology Slice 2 (Conserved Predator) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the pine marten — a conserved-consumption predator with an energy/hunger counter — to the ecology sim, and prove headless that predator and prey oscillate on the torus without collapsing (the make-or-break feasibility gate).

**Architecture:** Extend the tested core `life-ecology-core.js` with a two-phase conserved-predation rule (deterministic hash arbitration: one prey/one death, one marten/one meal, missed hunts drain the loser) plus a per-cell `energy` array. Task 1 lands the rule + Node asserts + a headless persistence/oscillation check — all headless, the feasibility answer. Task 2 wires it into `life-ecology.html` so it can be watched. Still CPU-only; GPU deferred to slice 4.

**Tech Stack:** Vanilla JS, Three.js r0.128 (CDN), Node built-in test runner. No build step, no new dependencies.

## Global Constraints

- Single-file HTML app opened via `file://`; sim rule lives ONLY in `life-ecology-core.js` (no rule copy in the HTML).
- Cell states: `0=EMPTY, 1=RED, 2=GREY, 3=MARTEN`.
- **Conserved consumption:** each prey eaten by exactly one marten; each marten takes at most one meal per tick; a missed hunt still costs the marten its turn (gains nothing). Arbitration is a deterministic integer hash of `(martenIndex, preyIndex, gen)` so it is reproducible.
- Marten carries an **energy** counter (`Float32Array` parallel to `grid`): `−δ` each tick, `+g` on a meal, `≤0` → dies, clamped to `E_cap`, breeds when `≥E_breed`.
- Slice 2 keeps BOTH prey (red+grey competition still runs); the marten eats both **equally** — `evade = 0` for both (asymmetry is slice 3). `evadeRed`/`evadeGrey` params exist, default 0.
- Marten colour: brown `#7a4a20` → `[122,74,32]`, brightness ramped by energy. EMPTY `[26,18,8]`, RED `[224,83,58]`, GREY `[154,164,176]` unchanged.
- Starting params: `δ=1.0, g=3.0, E_breed=10, E0=5, breedCost=5, E_cap=15, μ=0.5`. Prey: `βred=0.10, βgrey=0.14` (narrowed gap per the slice-1 decision — reds must linger, not go extinct), `σ=0.92`.
- All randomness through an injected `rng`. Commits: Conventional Commits scoped `feat(life-ecology): ...`.

---

## File Structure

- **Modify** `life-ecology-core.js` — extend `stepEcology` to `(grid, energy, COLS, ROWS, Cwrap, Rwrap, params, rng)` returning `{ grid, energy }`; add the two-phase conserved predation + `hasEmptyNeighbour` helper. One responsibility unchanged: advance the grid one generation.
- **Modify** `life-ecology-core.test.js` — migrate the 7 existing calls to the new signature (mechanical), add predation asserts + the headless persistence/oscillation feasibility check.
- **Modify** `life-ecology.html` — thread the `energy` array; add marten to render (energy-ramped brown), a "Seed martens" button + a Marten pen species, the 7 marten sliders, a 3rd (marten) chart line, and marten count + mean-energy in the status.

---

### Task 1: Conserved-predation core + headless feasibility gate

**Files:**
- Modify: `life-ecology-core.js`
- Modify: `life-ecology-core.test.js`

**Interfaces:**
- Consumes: existing `STATES`, `resolveCell`, `MOORE` (already in the module).
- Produces:
  - `stepEcology(grid, energy, COLS, ROWS, Cwrap, Rwrap, params, rng) -> { grid: Uint8Array, energy: Float32Array }`. `params = { betaRed, betaGrey, sigma, delta?, g?, eBreed?, e0?, breedCost?, eCap?, mu?, evadeRed?, evadeGrey?, gen? }` (marten fields default so prey-only callers keep working).
  - `hasEmptyNeighbour(grid, c, r, COLS, ROWS, Cwrap, Rwrap) -> bool`.

- [ ] **Step 1: Migrate existing test calls to the new signature (write the change first, watch it fail)**

In `life-ecology-core.test.js`, every existing `stepEcology(grid, COLS, ROWS, Cwrap, Rwrap, params, rng)` becomes `stepEcology(grid, new Float32Array(grid.length), COLS, ROWS, Cwrap, Rwrap, params, rng).grid`. (Prey-only tests pass a zero energy array and no marten cells, so predation is a no-op — behaviour is identical.) Do this for all 7 calls (the 4 original + the 3 competition/flip/empty tests). Do not change their assertions.

- [ ] **Step 2: Run tests to verify they fail on the signature change**

Run: `node --test life-ecology-core.test.js`
Expected: FAIL — the current `stepEcology` returns a `Uint8Array`, so `.grid` is `undefined` and the migrated tests throw / assert wrong. (This confirms the tests now demand the new contract.)

- [ ] **Step 3: Write the failing predation tests**

Append to `life-ecology-core.test.js`:

```js
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
```

- [ ] **Step 4: Run to verify the predation tests fail (function not extended yet)**

Run: `node --test life-ecology-core.test.js`
Expected: FAIL on the new predation tests (and still the migrated ones).

- [ ] **Step 5: Extend the core**

In `life-ecology-core.js`, replace `stepEcology` with the version below and add `hasEmptyNeighbour`; keep `STATES`, `resolveCell`, `MOORE` and the export list (add `hasEmptyNeighbour`).

```js
  function hasEmptyNeighbour(grid, c, r, COLS, ROWS, Cwrap, Rwrap) {
    for (const [dc, dr] of MOORE) {
      const i = resolveCell(c + dc, r + dr, COLS, ROWS, Cwrap, Rwrap);
      if (i !== null && grid[i] === STATES.EMPTY) return true;
    }
    return false;
  }

  // One generation. Prey grow by contact process; the marten is a conserved
  // predator with an energy counter. Two phases with an intent buffer (claim[]):
  // (1) each marten claims its highest-priority prey neighbour; (2) each prey is
  // eaten by the highest-priority marten that claimed it (unless it evades), so
  // one prey dies at most once and one marten eats at most once. Missed hunts
  // (lost the arbitration, or prey evaded) feed nobody.
  function stepEcology(grid, energy, COLS, ROWS, Cwrap, Rwrap, params, rng) {
    const N = COLS * ROWS;
    const { betaRed, betaGrey, sigma,
            delta = 1, g = 3, eBreed = 10, e0 = 5, breedCost = 5, eCap = 15, mu = 0.5,
            evadeRed = 0, evadeGrey = 0, gen = 0 } = params;
    const { EMPTY, RED, GREY, MARTEN } = STATES;
    const nextGrid = new Uint8Array(N), nextEnergy = new Float32Array(N);
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
        const p = prio(m, i);
        if (best < 0 || p > bestP) { best = i; bestP = p; }
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

    // PHASE 2b: build the next grid + energy.
    for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) {
      const here = r * COLS + c, v = grid[here];
      if (v === MARTEN) {
        let e = energy[here] - delta + (ate[here] ? g : 0);
        // ponytail: flat breed cost when eligible AND could seed an empty neighbour;
        // exact per-offspring charge needs radius-2, not worth it for the dynamics.
        if (energy[here] >= eBreed && hasEmptyNeighbour(grid, c, r, COLS, ROWS, Cwrap, Rwrap)) e -= breedCost;
        if (e <= 0) { nextGrid[here] = EMPTY; nextEnergy[here] = 0; }
        else { nextGrid[here] = MARTEN; nextEnergy[here] = e < eCap ? e : eCap; }
        continue;
      }
      if (isPrey(v)) {
        nextGrid[here] = eaten[here] ? EMPTY : (rng() < sigma ? v : EMPTY);
        continue;
      }
      // EMPTY: contact-process prey colonisation vs marten breeding spill, one roll.
      let nRed = 0, nGrey = 0, nElig = 0;
      for (const [dc, dr] of MOORE) {
        const i = resolveCell(c + dc, r + dr, COLS, ROWS, Cwrap, Rwrap);
        if (i === null) continue;
        if (grid[i] === RED) nRed++;
        else if (grid[i] === GREY) nGrey++;
        else if (grid[i] === MARTEN && energy[i] >= eBreed) nElig++;
      }
      const pRed  = nRed  ? 1 - Math.pow(1 - betaRed,  nRed)  : 0;
      const pGrey = nGrey ? 1 - Math.pow(1 - betaGrey, nGrey) : 0;
      const pMart = nElig ? 1 - Math.pow(1 - mu,       nElig) : 0;
      const roll = rng();
      if (roll < pRed) nextGrid[here] = RED;
      else if (roll < pRed + pGrey) nextGrid[here] = GREY;
      else if (roll < pRed + pGrey + pMart) { nextGrid[here] = MARTEN; nextEnergy[here] = e0; }
      else nextGrid[here] = EMPTY;
    }
    return { grid: nextGrid, energy: nextEnergy };
  }
```

Update the module's `return { ... }` to include `hasEmptyNeighbour`.

- [ ] **Step 6: Run all tests**

Run: `node --test life-ecology-core.test.js`
Expected: PASS — all migrated + 4 predation tests green.

- [ ] **Step 7: Write the headless feasibility gate (the make-or-break check)**

Append to `life-ecology-core.test.js`:

```js
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
```

- [ ] **Step 8: Run the feasibility gate — treat the result as real data**

Run: `node --test life-ecology-core.test.js`
Expected: PASS if the conserved cell-based predator oscillates at these params. **If it FAILS, that is the feasibility finding, not a bug to hide.** Do NOT weaken the asserts. Sweep the params by hand to find a persisting/oscillating regime (try `g` 2.5–4, `delta` 0.5–1.5, initial marten density 0.01–0.05, `eBreed` 8–14) and record what you tried. Report to the controller: (a) which regime oscillates and its numbers, or (b) that no regime in that sweep sustains oscillation — in which case the cell-based conserved model may be infeasible and the fallback (WaTor movement, or the non-conserved field model) is the real next question. Commit with the working params baked into the test; if none works, commit the test as `.skip` with a comment recording the sweep, and flag BLOCKED.

- [ ] **Step 9: Commit**

```bash
git add life-ecology-core.js life-ecology-core.test.js
git commit -m "feat(life-ecology): conserved marten predation + energy + feasibility gate (slice 2)"
```

---

### Task 2: Wire the marten into `life-ecology.html`

**Files:**
- Modify: `life-ecology.html`

**Interfaces:**
- Consumes: the new `stepEcology(grid, energy, ...)` contract + `STATES.MARTEN` (Task 1).
- Produces: a watchable predator sim; no downstream code depends on it.

**Note:** Edit-and-verify, not unit TDD. Keep all rule logic in the core. Verify via the boot `selfTest()` + driving the page (headless counts + a screenshot when paused).

- [ ] **Step 1: Add the energy array + new signature**

In the sim globals, alongside `grid`, add `let energy = new Float32Array(COLS * ROWS);`. Change `step()` to:

```js
function step() {
  params.gen = generation;
  const out = stepEcology(grid, energy, COLS, ROWS, Cwrap, Rwrap, params, rng);
  grid = out.grid; energy = out.energy;
  generation++;
  paint(); updateStatus();
}
```

Extend `params` to include the marten fields: `let params = { betaRed: 0.10, betaGrey: 0.14, sigma: 0.92, delta: 1.0, g: 3.0, eBreed: 10, e0: 5, breedCost: 5, eCap: 15, mu: 0.5, evadeRed: 0, evadeGrey: 0, gen: 0 };`. In `randomize()` and `clearGrid()` also reset energy: `energy = new Float32Array(COLS * ROWS);` (randomize seeds prey only — martens come from the button). In `rebuild(c, r)` add `energy = new Float32Array(COLS * ROWS);` beside the `grid` reallocation.

- [ ] **Step 2: Marten colour (energy-ramped) in paint**

Replace `cellRGB`:

```js
const DEAD = [26, 18, 8], RED_RGB = [224, 83, 58], GREY_RGB = [154, 164, 176];
const MART_DIM = [60, 36, 16], MART_BRIGHT = [150, 96, 44];   // brown, dim (hungry) -> bright (fed)
function cellRGB(v, e) {
  if (v === STATES.RED)  return RED_RGB;
  if (v === STATES.GREY) return GREY_RGB;
  if (v === STATES.MARTEN) {
    const t = Math.max(0, Math.min(1, e / params.eCap));
    return [0,1,2].map(k => MART_DIM[k] + (MART_BRIGHT[k] - MART_DIM[k]) * t);
  }
  return DEAD;
}
```

And in `paint()` pass energy: `const rgb = cellRGB(grid[i], energy[i]);`.

- [ ] **Step 3: Seed-martens button + Marten pen species**

In the panel, add a Marten button to the pen-species row and a Seed-martens button near Random/Clear:

```html
  <div class="row" id="specRow">
    <button class="spec active" data-spec="1" style="color:#e0533a">■ Red</button>
    <button class="spec" data-spec="2" style="color:#9aa4b0">■ Grey</button>
    <button class="spec" data-spec="3" style="color:#b07a40">■ Marten</button>
  </div>
```
```html
  <button id="seedMart" style="margin-top:6px; width:100%">Seed a marten population</button>
```

Pen must seed energy when placing a marten; extend the pointer handlers so painting `STATES.MARTEN` sets `energy[i] = params.e0` (and clears it on erase / for prey). Add the seeding action:

```js
function seedMartens() {
  // drop a sparse scatter of martens across the grid (the small viable population)
  for (let i = 0; i < grid.length; i++)
    if (grid[i] !== STATES.MARTEN && Math.random() < 0.02) { grid[i] = STATES.MARTEN; energy[i] = params.e0 + 3; }
  paint(); updateStatus();
}
$('seedMart').onclick = seedMartens;
```

In the pen `pointerdown`/`pointermove`, when `paintVal === STATES.MARTEN` set `energy[idx] = params.e0`; when erasing or painting prey set `energy[idx] = 0`.

- [ ] **Step 4: Marten sliders**

Add a "Pine marten" control group after the Squirrels group:

```html
  <div class="grp">Pine marten</div>
  <label>Meal energy g (<span id="gv">3.0</span>)</label>
  <input type="range" id="mg" min="0" max="80" value="30">
  <label>Metabolism δ (<span id="dv">1.0</span>) / tick</label>
  <input type="range" id="md" min="1" max="40" value="10">
  <label>Breed threshold E_breed (<span id="ebv">10</span>)</label>
  <input type="range" id="meb" min="1" max="30" value="10">
  <label>Breed spill μ (<span id="muv">0.50</span>)</label>
  <input type="range" id="mmu" min="0" max="100" value="50">
```

Wire them (g/δ scale by /10, E_breed integer, μ by /100):

```js
$('mg').oninput  = e => { params.g     = +e.target.value/10;  $('gv').textContent  = params.g.toFixed(1); };
$('md').oninput  = e => { params.delta = +e.target.value/10;  $('dv').textContent  = params.delta.toFixed(1); };
$('meb').oninput = e => { params.eBreed = +e.target.value|0;  $('ebv').textContent = params.eBreed; };
$('mmu').oninput = e => { params.mu    = +e.target.value/100; $('muv').textContent = params.mu.toFixed(2); };
```

(E0, breedCost, E_cap keep their defaults — not every knob needs a slider for slice 2. `// ponytail: expose only the knobs that change the dynamics; add the rest if tuning needs them.`)

- [ ] **Step 5: 3rd chart line + status**

Extend `counts()` to also count martens + mean energy, add a marten history line, and show it:

```js
function counts() {
  let nr = 0, ng = 0, nm = 0, es = 0;
  for (let i = 0; i < grid.length; i++) {
    const v = grid[i];
    if (v === STATES.RED) nr++;
    else if (v === STATES.GREY) ng++;
    else if (v === STATES.MARTEN) { nm++; es += energy[i]; }
  }
  return { nr, ng, nm, me: nm ? es / nm : 0 };
}
```

In `updateStatus()` push `{ nr, ng, nm }` to `popHist`, set a marten count span, and a mean-energy span. In `drawChart()` add a third line: `line('nm', '#b07a40');`. Add the marten readout to the status HTML (`● marten <span id="nmart">—</span> · Ē <span id="emart">—</span>`), and set them in `updateStatus`.

- [ ] **Step 6: Update selfTest + Help**

Extend the boot `selfTest()` with a marten check (a marten beside a prey eats it and gains energy under a deterministic rng), e.g. build a 5×5, one marten (energy 5) next to one red, `stepEcology(..., () => 0.5)`, assert the prey cell became EMPTY and the marten's energy rose. Update the Help dialog copy to describe the marten: conserved predator, energy/hunger, breeds when fed, starves without prey, eats red and grey equally for now (asymmetry — the reason reds recover — comes next).

- [ ] **Step 7: Verify in the browser**

Serve and load `life-ecology.html`; `randomize()` then `seedMartens()`, run ~300 gens headless (drive `step()` in a loop via the page), read `counts()` — expect martens persist (nm in a band, not 0, not everything) and prey persist; then pause and screenshot the torus (martens as brown flecks among red/grey). Confirm the 3-line chart shows the marten line oscillating and the console is clean.

- [ ] **Step 8: Commit**

```bash
git add life-ecology.html
git commit -m "feat(life-ecology): wire marten predator into the browser shell (slice 2)"
```

---

## Self-Review

**Spec coverage (slice 2 scope):**
- Conserved two-phase predation (hash arbitration, one prey/one meal, missed-hunt drain) → Task 1 `stepEcology` + conservation/missed-hunt tests. ✓
- Marten energy counter (−δ, +g, ≤0 dies, E_cap clamp, breeds ≥E_breed) → Task 1 + starvation/breeding tests. ✓
- Both prey kept, marten eats both equally (evade 0) → Global Constraints + FEASIBILITY test uses one prey behaviour to isolate the predator. ✓
- **Persistence/oscillation feasibility gate** → Task 1 Step 7–8 headless check, explicitly allowed to fail as a finding. ✓
- Marten render (energy-ramped brown), seed button, pen species, sliders, 3-line chart, status → Task 2. ✓
- Rule stays in the core; HTML has no rule copy → Task 2 delegates via `stepEcology`. ✓
- Narrowed β gap (0.10/0.14) so reds linger → Global Constraints + params. ✓
- Deferred: asymmetric evasion / cascade (slice 3), GPU (slice 4) — `evade*` params reserved, defaulted 0. ✓ (noted, not built)

**Placeholder scan:** No TBD/TODO; Task 1 carries complete code; Task 2 gives concrete code for every non-trivial piece and names exact ids/handlers for the rest. ✓

**Type consistency:** `stepEcology(grid, energy, COLS, ROWS, Cwrap, Rwrap, params, rng) -> {grid, energy}` identical across core, tests, and HTML. `params` marten fields (`delta, g, eBreed, e0, breedCost, eCap, mu, evadeRed, evadeGrey, gen`) consistent between defaults, tests, sliders, and `cellRGB`'s `params.eCap`. Chart keys `nr`/`ng`/`nm` consistent between `counts()`, `updateStatus`, `drawChart`. `STATES.MARTEN = 3` throughout. ✓

---

## Notes for the executor

- Task 1 is the feasibility gate — its headless persistence/oscillation result is the deliverable the user is waiting on. Report the actual numbers (marten mean/stdDev, final prey count) whether it passes or fails. A failure that survives a param sweep is a legitimate STOP-and-report, not something to force green.
- Do NOT add asymmetric evasion (slice 3) or GPU (slice 4). `evadeRed`/`evadeGrey` stay 0.
- After Task 2, stop and hand back for slice-2 evaluation before planning slice 3 (the cascade).
