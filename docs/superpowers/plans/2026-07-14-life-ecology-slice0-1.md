# Life-Ecology Slices 0–1 (Substrate + Competition) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A watchable browser sim where invasive grey squirrels out-breed and displace native reds via a space-limited contact process — no predator yet.

**Architecture:** A new self-contained `life-ecology.html` forked from `life-evolve.html` (torus render, tools, chart, GPU-harness pattern kept; B/S + genes + terrain dropped). The subtle sim rule lives in ONE tested module `life-ecology-core.js` (UMD-lite, like `life-stats.js`) so the HTML and Node tests share a single reference — the same reasoning the repo applies to `life-gpu.js`. Slice 1 is CPU-only; GPU port is deferred to slice 4.

**Tech Stack:** Vanilla JS, Three.js r0.128 (jsDelivr CDN), Node built-in test runner (`node --test`). No build step, no new dependencies.

## Global Constraints

- Single-file HTML app opened via `file://` — no server, no bundler (suite rule).
- Sim core is a UMD-lite module usable as both `<script src>` global `LifeEco` and Node `require()` (mirror `life-stats.js`).
- Cell states: `0 = EMPTY, 1 = RED, 2 = GREY, 3 = MARTEN` (MARTEN unused until slice 2 — reserve the value now).
- Prey rule = contact process only: `β` birth per same-species neighbour, `σ` natural survival. No B/S digit-set, no overcrowding death.
- Grey advantage is a single knob: `β_grey > β_red`. Start `β_red = 0.10`, `β_grey = 0.18`, `σ = 0.92`.
- Colours: RED `#e0533a`, GREY `#9aa4b0`, EMPTY `[26,18,8]` dark (suite convention).
- All randomness through an injected `rng` (defaults to `Math.random`; tests pass a seeded generator).
- Commits: Conventional Commits scoped to the sim, e.g. `feat(life-ecology): ...`.

---

## File Structure

- **Create** `life-ecology-core.js` — the pure sim: `resolveCell` (topology) + `stepEcology` (contact-process prey). No DOM, no Three.js. One responsibility: advance the grid one generation.
- **Create** `life-ecology-core.test.js` — Node `node:test` asserts for the core (statistical + edge cases), with a seeded RNG.
- **Create** `life-ecology.html` — the browser shell: fork of `life-evolve.html`, stripped to 3 states, wired to `LifeEco.stepEcology`, red/grey colours, β/σ sliders, species pen, red/grey population readout + 2-line chart.

---

### Task 1: Sim core `life-ecology-core.js` (contact-process competition)

**Files:**
- Create: `life-ecology-core.js`
- Test: `life-ecology-core.test.js`

**Interfaces:**
- Consumes: nothing (leaf module).
- Produces:
  - `resolveCell(nc, nr, COLS, ROWS, Cwrap, Rwrap) -> index | null` — topology wrap; `Cwrap`/`Rwrap` ∈ `'straight'|'flip'|'none'`.
  - `stepEcology(grid, COLS, ROWS, Cwrap, Rwrap, params, rng) -> Uint8Array` — returns the next grid. `params = { betaRed, betaGrey, sigma }`. `grid` values `0/1/2` (3 reserved). Empty cells colonise via contact process (red partition first, then grey, in one `rng()` draw); prey survive with prob `sigma`, else die. No predation yet.
  - `STATES = { EMPTY:0, RED:1, GREY:2, MARTEN:3 }`.

- [ ] **Step 1: Write the failing test**

Create `life-ecology-core.test.js`:

```js
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test life-ecology-core.test.js`
Expected: FAIL — `Cannot find module './life-ecology-core.js'`.

- [ ] **Step 3: Write minimal implementation**

Create `life-ecology-core.js`:

```js
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test life-ecology-core.test.js`
Expected: PASS — 4 tests, 0 fail.

- [ ] **Step 5: Commit**

```bash
git add life-ecology-core.js life-ecology-core.test.js
git commit -m "feat(life-ecology): contact-process prey core + tests (slice 0/1)"
```

---

### Task 2: Browser shell `life-ecology.html` (fork of evolve, wired to the core)

**Files:**
- Create: `life-ecology.html` (start from a copy of `life-evolve.html`)

**Interfaces:**
- Consumes: `LifeEco.stepEcology`, `LifeEco.STATES`, `LifeEco.resolveCell` (Task 1).
- Produces: a running browser sim; no downstream code depends on it.

**Note:** HTML is edit-and-verify, not pure unit TDD. The check is a boot-time `selfTest()` (console asserts) plus a playwright screenshot. Keep the core untouched — all rule logic stays in `life-ecology-core.js`.

- [ ] **Step 1: Copy the scaffold**

```bash
cp life-evolve.html life-ecology.html
```

- [ ] **Step 2: Strip evolve-specific machinery**

In `life-ecology.html`, remove: the `<select id="preset">`, `<input id="rule">`, `<select id="neigh">`, the whole **Evolution** group (mut/seed/randTau/colorby), the whole **Environment** group (envw/genTerr/flatTerr), the Glider and Terrain tool buttons, the clan/terr button rows, and the `<dialog id="ref">`. In `<script>`, delete: `KERNELS`/`kernel`/`parseRule`/`birthOK`/`surviveOK`, `tau`/`theta`/`env`/`envWeight`, `gauss`/`seedCellTau`, `computeNext` (evolve's), terrain fns (`wrapDist2`/`generateTerrain`/`flatTerrain`/`terrainAt`), `stampGliderAt`, the colour ramp/heatmap block (`RAMP`/`VIRIDIS`/`MAGMA`/`ramp3`/`tauHeat`/`thetaHeat`/`terrainRGB`/`cellRGB`), and the GPU block (`EVOLVE_SIM_FRAG`/`EVOLVE_COLOR_FRAG`/`GPU = LifeGPU.create(...)` and its `<script src="life-gpu.js">`). GPU returns in slice 4.

- [ ] **Step 3: Add the core script + ecology controls to the panel**

Replace `<script src="life-gpu.js"></script>` with `<script src="life-ecology-core.js"></script>`.
Set `<title>Squirrels & Pine Martens — Ecology on a Torus</title>` and the `<summary>` to match.
Replace the stripped control groups with (place after the Topology select):

```html
  <div class="grp">Squirrels</div>
  <label>Red birth rate β (<span id="brv">0.10</span>) — native</label>
  <input type="range" id="br" min="0" max="100" value="10">
  <label>Grey birth rate β (<span id="bgv">0.18</span>) — invasive</label>
  <input type="range" id="bg" min="0" max="100" value="18">
  <label>Natural survival σ (<span id="sgv">0.92</span>)</label>
  <input type="range" id="sg" min="0" max="100" value="92">

  <label>Pen species</label>
  <div class="row" id="specRow">
    <button class="spec active" data-spec="1" style="color:#e0533a">■ Red</button>
    <button class="spec" data-spec="2" style="color:#9aa4b0">■ Grey</button>
  </div>
```

Update the status block to red/grey and keep the chart canvas:

```html
  <div id="status">
    Gen <span id="gen">0</span> · pop <span id="pop">0</span><br>
    <span style="color:#e0533a">● red</span> <span id="nred">—</span>&nbsp;&nbsp;
    <span style="color:#9aa4b0">● grey</span> <span id="ngrey">—</span>
    <div style="margin-top:4px; font-size:10px; color:#566">populations vs time →</div>
    <canvas id="chart" width="412" height="90"
            style="width:100%; margin-top:2px; background:#0e1218; border:1px solid #2a3340; border-radius:4px;"></canvas>
  </div>
```

- [ ] **Step 4: Rewrite the JS sim section to use the core**

Replace the grid/params globals and `step`/`randomize`/`clearGrid`/`paint`/colour/chart/status/pointer sections with ecology versions. Concretely, the sim globals + step become:

```js
const { STATES, stepEcology } = LifeEco;
let COLS = 80, ROWS = 40;
let grid = new Uint8Array(COLS * ROWS);          // 0 empty, 1 red, 2 grey (3 marten: slice 2)
let params = { betaRed: 0.10, betaGrey: 0.18, sigma: 0.92 };
let Cwrap = 'straight', Rwrap = 'straight';
const TOPOS = { torus:['straight','straight'], klein:['flip','straight'],
                mobius:['flip','none'], cylinder:['straight','none'] };
let generation = 0;
const idx = (c, r) => r * COLS + c;
let rng = Math.random;

function step() {
  grid = stepEcology(grid, COLS, ROWS, Cwrap, Rwrap, params, rng);
  generation++;
  paint(); updateStatus();
}
function randomize() {
  const d = +document.getElementById('dens').value / 100;
  for (let i = 0; i < grid.length; i++)
    grid[i] = rng() < d ? (rng() < 0.5 ? STATES.RED : STATES.GREY) : STATES.EMPTY;
  freshStart();
}
function clearGrid() { grid.fill(0); freshStart(); }
function freshStart() { generation = 0; popHist.length = 0;
  document.getElementById('gen').textContent = '0'; paint(); updateStatus(); }
```

Colour + paint (species colours, dead dark):

```js
const DEAD = [26, 18, 8], RED_RGB = [224, 83, 58], GREY_RGB = [154, 164, 176];
function cellRGB(v) { return v === STATES.RED ? RED_RGB : v === STATES.GREY ? GREY_RGB : DEAD; }
function paint() {
  for (let i = 0; i < grid.length; i++) {
    const rgb = cellRGB(grid[i]);
    texData[i*4] = rgb[0]; texData[i*4+1] = rgb[1]; texData[i*4+2] = rgb[2]; texData[i*4+3] = 255;
  }
  tex.needsUpdate = true;
}
```

Status + 2-line population chart (reuse evolve's chart shape; counts not τ):

```js
const popHist = [], HIST_MAX = 300;
function counts() { let nr = 0, ng = 0;
  for (const v of grid) { if (v === STATES.RED) nr++; else if (v === STATES.GREY) ng++; }
  return { nr, ng }; }
function updateStatus() {
  const { nr, ng } = counts();
  document.getElementById('gen').textContent = generation;
  document.getElementById('pop').textContent = nr + ng;
  document.getElementById('nred').textContent = nr;
  document.getElementById('ngrey').textContent = ng;
  popHist.push({ nr, ng });
  if (popHist.length > HIST_MAX) popHist.shift();
  drawChart();
}
function drawChart() {
  const cv = document.getElementById('chart'); if (!cv) return;
  const ctx = cv.getContext('2d'), W = cv.width, H = cv.height, pad = 8;
  const total = COLS * ROWS;
  const x = i => pad + (i / (HIST_MAX - 1)) * (W - 2*pad);
  const y = n => (H - pad) - (n / total) * (H - 2*pad);   // pop fraction 0..1
  ctx.clearRect(0, 0, W, H);
  ctx.strokeStyle = '#222c39'; ctx.lineWidth = 1;
  ctx.beginPath(); ctx.moveTo(pad, y(0)); ctx.lineTo(W-pad, y(0)); ctx.stroke();
  const line = (key, col) => { ctx.strokeStyle = col; ctx.lineWidth = 2; ctx.beginPath();
    popHist.forEach((h, i) => { const px = x(i), py = y(h[key]); i ? ctx.lineTo(px, py) : ctx.moveTo(px, py); });
    ctx.stroke(); };
  line('nr', '#e0533a'); line('ng', '#9aa4b0');
}
```

Pointer/pen (single-species pen; drop glider/terrain paths):

```js
let tool = 'orbit', penSpec = STATES.RED;
function setTool(t) { tool = t; controls.enabled = true;
  controls.mouseButtons.LEFT = (t === 'pen') ? -1 : THREE.MOUSE.ROTATE;
  document.querySelectorAll('.tool').forEach(b => b.classList.toggle('active', b.dataset.tool === t)); }
document.querySelectorAll('.tool').forEach(b => b.onclick = () => setTool(b.dataset.tool));
document.querySelectorAll('.spec').forEach(b => b.onclick = () => {
  penSpec = +b.dataset.spec;
  document.querySelectorAll('.spec').forEach(x => x.classList.toggle('active', x === b)); });

let painting = false, paintVal;
const dom = renderer.domElement;
dom.addEventListener('pointerdown', e => {
  if (tool !== 'pen' || e.button !== 0) return;
  const cell = cellAt(e); if (!cell) return;
  paintVal = grid[idx(cell[0], cell[1])] ? STATES.EMPTY : penSpec;   // first cell draw/erase
  grid[idx(cell[0], cell[1])] = paintVal; painting = true; paint(); updateStatus();
});
dom.addEventListener('pointermove', e => {
  if (!painting) return; const cell = cellAt(e); if (!cell) return;
  grid[idx(cell[0], cell[1])] = paintVal; paint(); updateStatus();
});
dom.addEventListener('pointerup', () => { painting = false; });
```

Keep verbatim from evolve: `resolve` copy is unnecessary here (the core owns topology for stepping; `cellAt` uses UV directly), Three.js scene/camera/renderer/controls/lights, `tex`/`texData`/`torus`/`plane`, `rebuild`, `setView`, `animate` timing loop (delete its GPU branch — always `step()`), `$`, play/step/rand/clear/help wiring, resize handler. In `rebuild`, drop the `tau`/`theta`/`env`/GPU lines — only `grid`, `texData`, `tex`, the two geometries.

- [ ] **Step 5: Wire the new sliders + boot**

```js
$('br').oninput = e => { params.betaRed  = +e.target.value/100; $('brv').textContent = params.betaRed.toFixed(2); };
$('bg').oninput = e => { params.betaGrey = +e.target.value/100; $('bgv').textContent = params.betaGrey.toFixed(2); };
$('sg').oninput = e => { params.sigma    = +e.target.value/100; $('sgv').textContent = params.sigma.toFixed(2); };
$('topo').onchange = e => { [Cwrap, Rwrap] = TOPOS[e.target.value]; };
$('speed').oninput = e => { gps = +e.target.value; $('spd').textContent = gps; };
$('dens').oninput  = e => $('densv').textContent = (+e.target.value/100).toFixed(2);

function selfTest() {   // boot check: core reachable + grey out-breeds red
  const g = new Uint8Array(9); g[3] = STATES.RED; g[5] = STATES.GREY;
  let nr = 0, ng = 0;
  for (let t = 0; t < 4000; t++) {
    const n = stepEcology(g, 3, 3, 'none', 'none', { betaRed: 0.1, betaGrey: 0.3, sigma: 1 }, Math.random);
    if (n[4] === STATES.RED) nr++; else if (n[4] === STATES.GREY) ng++;
  }
  console.assert(ng > nr, `selfTest: grey ${ng} should out-colonise red ${nr}`);
}
selfTest();
setView(false);
randomize();
requestAnimationFrame(animate);
```

- [ ] **Step 6: Verify in a browser (playwright)**

Run the existing playwright helper pattern (chromium) to load `file://.../life-ecology.html`, press Play ~200 gens, screenshot. Expected: grid fills with red+grey, greys visibly expand and reds shrink over time; the chart shows the grey line rising above a falling red line; DevTools console shows no `selfTest` assertion and no errors.

- [ ] **Step 7: Commit**

```bash
git add life-ecology.html
git commit -m "feat(life-ecology): browser shell — red/grey contact-process competition (slice 1)"
```

---

### Task 3: Headless competition assertion (the slice-1 evaluation instrument)

**Files:**
- Modify: `life-ecology-core.test.js` (add one integration test)

**Interfaces:**
- Consumes: `LifeEco.stepEcology` (Task 1).
- Produces: a machine-checked statement that grey competitively excludes red — the objective evidence you evaluate slice 1 against.

- [ ] **Step 1: Write the failing test**

Append to `life-ecology-core.test.js`:

```js
test('grey competitively excludes red on a torus over time', () => {
  const COLS = 60, ROWS = 30, N = COLS * ROWS;
  const params = { betaRed: 0.10, betaGrey: 0.18, sigma: 0.92 };
  const rng = mulberry32(12345);
  let grid = new Uint8Array(N);
  for (let i = 0; i < N; i++) grid[i] = rng() < 0.30 ? (rng() < 0.5 ? STATES.RED : STATES.GREY) : STATES.EMPTY;
  const count = g => { let nr = 0, ng = 0; for (const v of g) { if (v === STATES.RED) nr++; else if (v === STATES.GREY) ng++; } return { nr, ng }; };
  const start = count(grid);
  for (let t = 0; t < 400; t++) grid = stepEcology(grid, COLS, ROWS, 'straight', 'straight', params, rng);
  const end = count(grid);
  // grey share rises, red share falls — competitive exclusion of the native
  const startGreyShare = start.ng / (start.nr + start.ng);
  const endGreyShare   = end.ng / (end.nr + end.ng);
  assert.ok(endGreyShare > startGreyShare + 0.15, `grey share ${startGreyShare.toFixed(2)} -> ${endGreyShare.toFixed(2)}`);
  assert.ok(end.nr < start.nr, `red count should fall: ${start.nr} -> ${end.nr}`);
});
```

- [ ] **Step 2: Run test to verify it fails or passes honestly**

Run: `node --test life-ecology-core.test.js`
Expected: PASS if the β gap drives exclusion at these params. If it FAILS (red persists), that is real signal — do NOT force the assertion; report the numbers and treat it as slice-1 evaluation data (may need a wider β gap or lower σ). Adjust the *test's* params to a gap that does exclude, and record the working gap for the user.

- [ ] **Step 3: Commit**

```bash
git add life-ecology-core.test.js
git commit -m "test(life-ecology): headless competitive-exclusion assertion (slice 1)"
```

---

## Self-Review

**Spec coverage (slices 0–1 scope):**
- 3-state substrate rendering → Task 2 (paint/cellRGB, states reserved incl. MARTEN). ✓
- Contact-process prey (β birth, σ survival, no B/S) → Task 1 `stepEcology`. ✓
- Grey advantage = single knob `β_grey > β_red` → Task 1 + Task 2 sliders. ✓
- Competitive exclusion visible → Task 2 chart + Task 3 headless assertion. ✓
- Species colours, species pen, population readout → Task 2. ✓
- Fork of evolve, drop B/S+genes+terrain, self-contained → Task 2 strip steps. ✓
- Slices 2–4 (predator, evasion, instrumentation, GPU) → intentionally deferred to post-evaluation plans; MARTEN state + energy-ready core signature reserved. ✓ (noted, not built)

**Placeholder scan:** No TBD/TODO; every code step carries complete code; Task 2 strip/wire steps name exact identifiers to remove/add. ✓

**Type consistency:** `stepEcology(grid, COLS, ROWS, Cwrap, Rwrap, params, rng)` and `params = {betaRed, betaGrey, sigma}` and `STATES` identical across Tasks 1–3 and the HTML. `resolveCell` signature consistent. Chart keys `nr`/`ng` consistent between `counts()`, `updateStatus`, `drawChart`, and the tests' `count`. ✓

---

## Notes for the executor

- Task 1 is real TDD (pure module). Task 2 is fork-strip-wire + a boot self-test + playwright screenshot — verify by observation, keep all rule logic in the core. Task 3 is an integration assertion that doubles as slice-1 evidence.
- Do **not** add the predator, energy array, evasion, GPU shaders, or terrain — those are later slices. Reserving `MARTEN = 3` and keeping `params` an object are the only forward hooks.
- After Task 3, stop and hand back to the user for slice-1 evaluation before planning slice 2.
