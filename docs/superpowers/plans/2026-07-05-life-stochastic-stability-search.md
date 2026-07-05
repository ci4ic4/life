# life-stochastic.html Stability Metric + Config Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a live population-ratio stability indicator and a headless batch config-search to `life-stochastic.html`, per `docs/superpowers/specs/2026-07-05-life-stochastic-stability-search-design.md`.

**Architecture:** Extract the pure simulation math (probability curves, neighbor/topology resolution, one grid step, a rolling-stats ring buffer, dead/full/stable/chaotic classification, and a headless trial runner) out of `life-stochastic.html` into a new dependency-free module `life-stats.js`, loaded by both the browser (plain `<script src>`, no bundler — must work over `file://`) and Node's built-in test runner (`node --test`). The HTML file keeps all DOM/THREE.js/UI code and calls into `LifeStats.*` for the math, so the interactive sim and the headless search run through the *same* code path and can't drift apart.

**Tech Stack:** Vanilla JS, Three.js (existing CDN script tags, unchanged), Node.js built-in test runner (`node:test` + `node:assert/strict` — already available, Node v26 confirmed installed, no new dependency).

## Global Constants (from spec — exact values)

```
WINDOW        = 50     // rolling window size, live monitor + search scoring
DEAD_T        = 0.02   // mean ratio below this => "dead"
FULL_T        = 0.95   // mean ratio above this => "full"
STABLE_T      = 0.03   // stdDev below this (and not dead/full) => "stable"
SEARCH_TRIALS = 30
SEARCH_COLS   = 60
SEARCH_ROWS   = 30
SEARCH_GENS   = 300
```

---

### Task 1: `life-stats.js` — probability curves, topology resolution, ring stats, classification

**Files:**
- Create: `life-stats.js`
- Create: `life-stats.test.js`

**Interfaces:**
- Produces: `LifeStats.bumpMax(set, n, s)`, `LifeStats.curve(set, parm, out)`, `LifeStats.resolveCell(nc, nr, cols, rows, cwrap, rwrap)`, `LifeStats.countNeighbors(c, r, cols, rows, cwrap, rwrap, grid)`, `LifeStats.makeRingStats(windowSize)` → `{reset(), push(v), full(), mean(), stdDev()}`, `LifeStats.classify(mean, stdDev, thresholds)` → `'dead'|'full'|'stable'|'chaotic'`, `LifeStats.THRESHOLDS` (the constants above).
- Consumes: nothing (first task).

- [ ] **Step 1: Write the failing tests**

Create `C:/Users/xci/source/life-stats.test.js`:

```js
const test = require('node:test');
const assert = require('node:assert/strict');
const LifeStats = require('./life-stats.js');

test('bumpMax peaks at member of set, falls off with distance', () => {
  const p3 = LifeStats.bumpMax(new Set([3]), 3, 0.6);
  const p2 = LifeStats.bumpMax(new Set([3]), 2, 0.6);
  const p0 = LifeStats.bumpMax(new Set([3]), 0, 0.6);
  assert.equal(p3, 1);
  assert.ok(p2 < p3 && p0 < p2);
});

test('curve applies floor/ceiling as a clamp around the Gaussian bump', () => {
  const out = new Float64Array(9);
  LifeStats.curve(new Set([2, 3]), { sigma: 0.05, ceil: 0.7, floor: 0.2 }, out);
  assert.ok(Math.abs(out[2] - 0.7) < 1e-9, 'peak hits ceiling');
  assert.ok(Math.abs(out[0] - 0.2) < 1e-9, 'tail hits floor');
});

test('resolveCell wraps a torus on both axes', () => {
  assert.equal(LifeStats.resolveCell(-1, 0, 10, 5, 'straight', 'straight'), 0 * 10 + 9);
  assert.equal(LifeStats.resolveCell(10, 0, 10, 5, 'straight', 'straight'), 0 * 10 + 0);
});

test('resolveCell returns null crossing a "none" boundary', () => {
  assert.equal(LifeStats.resolveCell(-1, 0, 10, 5, 'none', 'straight'), null);
  assert.equal(LifeStats.resolveCell(0, -1, 10, 5, 'straight', 'none'), null);
});

test('resolveCell flips the other axis crossing a "flip" seam (Klein bottle)', () => {
  // crossing the C seam (cwrap='flip') mirrors r -> rows-1-r
  assert.equal(LifeStats.resolveCell(-1, 1, 10, 5, 'flip', 'straight'), (5 - 1 - 1) * 10 + 9);
});

test('countNeighbors counts live cells around a point on a small torus', () => {
  const cols = 3, rows = 3;
  const grid = new Uint8Array(cols * rows);
  grid[0] = 1; // (0,0)
  grid[1] = 1; // (1,0)
  const n = LifeStats.countNeighbors(0, 0, cols, rows, 'straight', 'straight', grid);
  // (1,0) is a direct neighbor; (0,0) itself doesn't count
  assert.equal(n, 1);
});

test('makeRingStats computes mean/stdDev over a fixed window, ring-overwrites old values', () => {
  const rs = LifeStats.makeRingStats(3);
  [1, 1, 1].forEach(v => rs.push(v));
  assert.ok(rs.full());
  assert.equal(rs.mean(), 1);
  assert.equal(rs.stdDev(), 0);
  rs.push(0); // overwrites the oldest 1 -> window is now [1,1,0]
  assert.ok(Math.abs(rs.mean() - (2 / 3)) < 1e-9);
  assert.ok(rs.stdDev() > 0);
});

test('classify labels dead/full/stable/chaotic by thresholds', () => {
  const t = LifeStats.THRESHOLDS;
  assert.equal(LifeStats.classify(0.01, 0, t), 'dead');
  assert.equal(LifeStats.classify(0.97, 0, t), 'full');
  assert.equal(LifeStats.classify(0.4, 0.01, t), 'stable');
  assert.equal(LifeStats.classify(0.4, 0.2, t), 'chaotic');
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `node --test life-stats.test.js`
Expected: fails with `Error: Cannot find module './life-stats.js'`

- [ ] **Step 3: Implement `life-stats.js`**

Create `C:/Users/xci/source/life-stats.js`:

```js
// UMD-lite: works as a plain <script src> global (browser, incl. file://) and as a
// CommonJS require() (Node test runner). No bundler, no build step.
(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.LifeStats = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  function bumpMax(set, n, s) {
    let p = 0;
    for (const c of set) { const d = n - c; p = Math.max(p, Math.exp(-(d * d) / (2 * s * s))); }
    return p;
  }

  function curve(set, parm, out) {
    const span = Math.max(0, parm.ceil - parm.floor);
    for (let n = 0; n <= 8; n++) out[n] = parm.floor + span * bumpMax(set, n, parm.sigma);
    return out;
  }

  function resolveCell(nc, nr, cols, rows, cwrap, rwrap) {
    if (nc < 0 || nc >= cols) {
      if (cwrap === 'none') return null;
      if (cwrap === 'flip') nr = rows - 1 - nr;
      nc = (nc + cols) % cols;
    }
    if (nr < 0 || nr >= rows) {
      if (rwrap === 'none') return null;
      if (rwrap === 'flip') nc = cols - 1 - nc;
      nr = (nr + rows) % rows;
    }
    return nr * cols + nc;
  }

  function countNeighbors(c, r, cols, rows, cwrap, rwrap, grid) {
    let n = 0;
    for (let dr = -1; dr <= 1; dr++)
      for (let dc = -1; dc <= 1; dc++) {
        if (!dr && !dc) continue;
        const i = resolveCell(c + dc, r + dr, cols, rows, cwrap, rwrap);
        if (i !== null) n += grid[i];
      }
    return n;
  }

  function makeRingStats(windowSize) {
    const buf = new Float64Array(windowSize);
    let count = 0, pos = 0, sum = 0, sumSq = 0;
    return {
      reset() { buf.fill(0); count = 0; pos = 0; sum = 0; sumSq = 0; },
      push(v) {
        if (count === windowSize) {
          const old = buf[pos];
          sum -= old; sumSq -= old * old;
        } else {
          count++;
        }
        buf[pos] = v;
        sum += v; sumSq += v * v;
        pos = (pos + 1) % windowSize;
      },
      full() { return count === windowSize; },
      mean() { return count === 0 ? 0 : sum / count; },
      stdDev() {
        if (count === 0) return 0;
        const m = sum / count;
        const variance = Math.max(0, sumSq / count - m * m);
        return Math.sqrt(variance);
      },
    };
  }

  const THRESHOLDS = { WINDOW: 50, DEAD_T: 0.02, FULL_T: 0.95, STABLE_T: 0.03 };

  function classify(mean, stdDev, thresholds) {
    const t = thresholds || THRESHOLDS;
    if (mean < t.DEAD_T) return 'dead';
    if (mean > t.FULL_T) return 'full';
    if (stdDev < t.STABLE_T) return 'stable';
    return 'chaotic';
  }

  return { bumpMax, curve, resolveCell, countNeighbors, makeRingStats, classify, THRESHOLDS };
});
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `node --test life-stats.test.js`
Expected: all 7 tests pass (`# pass 7`, `# fail 0`)

- [ ] **Step 5: Commit**

```bash
git add life-stats.js life-stats.test.js
git commit -m "feat: add life-stats pure sim/stats module with tests"
```

---

### Task 2: `stepGrid` + `runHeadlessTrial` (headless search core)

**Files:**
- Modify: `life-stats.js`
- Modify: `life-stats.test.js`

**Interfaces:**
- Consumes: `resolveCell`, `countNeighbors`, `curve`, `makeRingStats`, `classify`, `THRESHOLDS` (Task 1).
- Produces: `LifeStats.stepGrid(grid, cols, rows, cwrap, rwrap, pBirth, pSurvive, rng)` → `Uint8Array`, `LifeStats.runHeadlessTrial(opts)` → `{mean, stdDev, status, finalRatio}` where `opts = {cols, rows, cwrap, rwrap, gens, windowSize, thresholds, birth, survive, bParm, sParm, density, rng}`.

- [ ] **Step 1: Write the failing tests**

Append to `life-stats.test.js`:

```js
test('stepGrid: an isolated live cell with 0 neighbors dies under default B3/S23-shaped tables', () => {
  const cols = 5, rows = 5;
  const grid = new Uint8Array(cols * rows);
  grid[12] = 1; // center, no neighbors
  const pBirth = new Float64Array(9), pSurvive = new Float64Array(9);
  LifeStats.curve(new Set([3]), { sigma: 0.6, ceil: 1, floor: 0 }, pBirth);
  LifeStats.curve(new Set([2, 3]), { sigma: 0.6, ceil: 1, floor: 0 }, pSurvive);
  const next = LifeStats.stepGrid(grid, cols, rows, 'straight', 'straight', pBirth, pSurvive, () => 0.5);
  assert.equal(next[12], 0, 'survive prob at n=0 is ~0, rng()=0.5 should not pass');
});

test('runHeadlessTrial: rng that always fails every threshold keeps an empty grid dead', () => {
  const result = LifeStats.runHeadlessTrial({
    cols: 10, rows: 10, cwrap: 'straight', rwrap: 'straight',
    gens: 60, windowSize: LifeStats.THRESHOLDS.WINDOW, thresholds: LifeStats.THRESHOLDS,
    birth: new Set([3]), survive: new Set([2, 3]),
    bParm: { sigma: 0.6, ceil: 1, floor: 0 }, sParm: { sigma: 0.6, ceil: 1, floor: 0 },
    density: 0.3, rng: () => 0.99,
  });
  assert.equal(result.status, 'dead');
  assert.equal(result.mean, 0);
});

test('runHeadlessTrial: rng that always passes fills and stays full', () => {
  const result = LifeStats.runHeadlessTrial({
    cols: 10, rows: 10, cwrap: 'straight', rwrap: 'straight',
    gens: 60, windowSize: LifeStats.THRESHOLDS.WINDOW, thresholds: LifeStats.THRESHOLDS,
    birth: new Set([3]), survive: new Set([2, 3]),
    bParm: { sigma: 0.6, ceil: 1, floor: 0 }, sParm: { sigma: 0.6, ceil: 1, floor: 0 },
    density: 0.3, rng: () => 0,
  });
  assert.equal(result.status, 'full');
  assert.equal(result.mean, 1);
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `node --test life-stats.test.js`
Expected: `TypeError: LifeStats.stepGrid is not a function`

- [ ] **Step 3: Implement `stepGrid` + `runHeadlessTrial`**

In `life-stats.js`, add inside the factory function (after `makeRingStats`/`classify`, before the final `return`):

```js
  function stepGrid(grid, cols, rows, cwrap, rwrap, pBirth, pSurvive, rng) {
    const next = new Uint8Array(cols * rows);
    for (let r = 0; r < rows; r++)
      for (let c = 0; c < cols; c++) {
        const n = countNeighbors(c, r, cols, rows, cwrap, rwrap, grid);
        const idx = r * cols + c;
        const p = grid[idx] ? pSurvive[n] : pBirth[n];
        next[idx] = rng() < p ? 1 : 0;
      }
    return next;
  }

  function runHeadlessTrial(opts) {
    const { cols, rows, cwrap, rwrap, gens, windowSize, thresholds, birth, survive, bParm, sParm, density, rng } = opts;
    let grid = new Uint8Array(cols * rows);
    for (let i = 0; i < grid.length; i++) grid[i] = rng() < density ? 1 : 0;
    const pBirth = new Float64Array(9), pSurvive = new Float64Array(9);
    curve(birth, bParm, pBirth);
    curve(survive, sParm, pSurvive);
    const stats = makeRingStats(windowSize);
    for (let g = 0; g < gens; g++) {
      grid = stepGrid(grid, cols, rows, cwrap, rwrap, pBirth, pSurvive, rng);
      let pop = 0; for (const v of grid) pop += v;
      stats.push(pop / (cols * rows));
    }
    const mean = stats.mean(), stdDev = stats.stdDev();
    return { mean, stdDev, status: classify(mean, stdDev, thresholds), finalRatio: mean };
  }
```

Update the final `return` statement to:

```js
  return {
    bumpMax, curve, resolveCell, countNeighbors, stepGrid,
    makeRingStats, classify, runHeadlessTrial, THRESHOLDS,
  };
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `node --test life-stats.test.js`
Expected: all 10 tests pass (`# pass 10`, `# fail 0`)

- [ ] **Step 5: Commit**

```bash
git add life-stats.js life-stats.test.js
git commit -m "feat: add stepGrid and runHeadlessTrial to life-stats"
```

---

### Task 3: Wire `life-stochastic.html` to `LifeStats` (replace local duplicate logic)

**Files:**
- Modify: `life-stochastic.html`

**Interfaces:**
- Consumes: `LifeStats.curve`, `LifeStats.resolveCell`, `LifeStats.countNeighbors`, `LifeStats.stepGrid` (Tasks 1-2).
- Produces: same global `birth`, `survive`, `bParm`, `sParm`, `pBirth`, `pSurvive`, `grid`, `step()`, `buildTables()` as before — this task only changes *implementation*, not any external behavior or names other function outside this file depend on.

This task removes the duplicated `bumpMax`, `curve`, `resolve`, `neighbors` functions from the HTML and calls the shared module instead, and rewrites `step()`'s inner loop to call `LifeStats.stepGrid`.

- [ ] **Step 1: Load the shared module**

In `life-stochastic.html`, find:

```html
<script src="https://cdn.jsdelivr.net/npm/three@0.128.0/build/three.min.js"></script>
<script src="https://cdn.jsdelivr.net/npm/three@0.128.0/examples/js/controls/OrbitControls.js"></script>
<script>
```

Replace with:

```html
<script src="https://cdn.jsdelivr.net/npm/three@0.128.0/build/three.min.js"></script>
<script src="https://cdn.jsdelivr.net/npm/three@0.128.0/examples/js/controls/OrbitControls.js"></script>
<script src="life-stats.js"></script>
<script>
```

- [ ] **Step 2: Remove the now-duplicated pure functions, delegate to `LifeStats`**

Find (the whole block from `bumpMax` through `buildTables`):

```js
function bumpMax(set, n, s) {
  let p = 0;
  for (const c of set) { const d = n - c; p = Math.max(p, Math.exp(-(d*d) / (2*s*s))); }
  return p;
}
function curve(set, parm, out) {
  const span = Math.max(0, parm.ceil - parm.floor);   // floor>ceil -> flat line at floor
  for (let n = 0; n <= 8; n++) out[n] = parm.floor + span * bumpMax(set, n, parm.sigma);
}
function buildTables() {
  curve(birth,   bParm, pBirth);
  curve(survive, sParm, pSurvive);
  drawChart();
}
```

Replace with:

```js
function buildTables() {
  LifeStats.curve(birth,   bParm, pBirth);
  LifeStats.curve(survive, sParm, pSurvive);
  drawChart();
}
```

- [ ] **Step 3: Delete the local `resolve`/`neighbors`, delegate `step()`**

Find:

```js
// Map a (possibly off-grid by one) neighbor coord back onto the grid per topology.
// Returns null if it falls off an open ('none') boundary -> counts as dead.
function resolve(nc, nr) {
  if (nc < 0 || nc >= COLS) {
    if (Cwrap === 'none') return null;
    if (Cwrap === 'flip') nr = ROWS - 1 - nr;   // crossing C seam flips R
    nc = (nc + COLS) % COLS;
  }
  if (nr < 0 || nr >= ROWS) {
    if (Rwrap === 'none') return null;
    if (Rwrap === 'flip') nc = COLS - 1 - nc;    // crossing R seam flips C
    nr = (nr + ROWS) % ROWS;
  }
  return idx(nc, nr);
}

function neighbors(c, r) {
  let n = 0;
  for (let dr = -1; dr <= 1; dr++)
    for (let dc = -1; dc <= 1; dc++) {
      if (!dr && !dc) continue;
      const i = resolve(c + dc, r + dr);
      if (i !== null) n += grid[i];
    }
  return n;
}

function step() {
  const next = new Uint8Array(COLS * ROWS);
  for (let r = 0; r < ROWS; r++)
    for (let c = 0; c < COLS; c++) {
      const n = neighbors(c, r);
      const p = grid[idx(c, r)] ? pSurvive[n] : pBirth[n];   // stochastic: roll against p
      next[idx(c, r)] = Math.random() < p ? 1 : 0;
    }
  grid = next;
  generation++;
  paint();
  updateStatus();
}
```

Replace with:

```js
function step() {
  grid = LifeStats.stepGrid(grid, COLS, ROWS, Cwrap, Rwrap, pBirth, pSurvive, Math.random);
  generation++;
  paint();
  updateStatus();
}
```

(`idx` stays — it's still used elsewhere in this file for painting/tools.)

- [ ] **Step 4: Update `selfTest()` to call through `LifeStats`**

Find:

```js
  Object.assign(bParm, { sigma: 0.6, ceil: 1, floor: 0 });
  Object.assign(sParm, { sigma: 0.6, ceil: 1, floor: 0 }); buildTables();
```

(no change needed here — `buildTables()` already delegates as of Step 2. `selfTest()` needs no edits since it only calls `buildTables()` and reads `pBirth`/`pSurvive`, never called `curve`/`bumpMax` directly.)

Confirm by searching the file for any remaining bare (non-`LifeStats.`) calls to `curve(`, `bumpMax(`, `resolve(`, `neighbors(` — there should be none left outside `life-stats.js` itself.

- [ ] **Step 5: Manual browser verification**

Open `life-stochastic.html` directly in Chrome (double-click, `file://` URL is fine — plain `<script src>` is not subject to module CORS restrictions).

Expected, via DevTools console (F12):
- No red errors on load.
- No `console.assert` failures printed (the existing `selfTest()` runs at boot — a failed assert prints `Assertion failed: <message>`).
- The torus renders and animates (Play button starts/stops motion as before).
- Switch the rule dropdown (e.g. to HighLife) and confirm the chart curves update and the sim's behavior visibly changes — proves `LifeStats.curve`/`stepGrid` are wired correctly end-to-end.

- [ ] **Step 6: Commit**

```bash
git add life-stochastic.html
git commit -m "refactor: delegate life-stochastic sim math to shared life-stats module"
```

---

### Task 4: Live stability monitor (ring stats + badge)

**Files:**
- Modify: `life-stochastic.html`

**Interfaces:**
- Consumes: `LifeStats.makeRingStats`, `LifeStats.classify`, `LifeStats.THRESHOLDS` (Task 1).
- Produces: a global `liveStats` ring-stats instance and a `updateLiveBadge()` function, called from `step()`/`updateStatus()`, reset from `freshStart()`/`rebuild()`.

- [ ] **Step 1: Add the status badge markup**

Find (in the `#status` div):

```html
  <div id="status">
    Gen <span id="gen">0</span> · pop <span id="pop">0</span>
    <div style="margin-top:6px; font-size:10px;">
```

Replace with:

```html
  <div id="status">
    Gen <span id="gen">0</span> · pop <span id="pop">0</span>
    <div style="margin-top:6px; font-size:11px;">
      <span id="stabBadge" style="padding:2px 6px; border-radius:4px; background:#2a3340;">warming up</span>
      &nbsp;stdDev <span id="stabStd">—</span>
    </div>
    <div style="margin-top:6px; font-size:10px;">
```

- [ ] **Step 2: Add badge styling per status**

Find (in the `<style>` block, after `#gen { color:#cdd6e0; }`):

```css
  #gen { color:#cdd6e0; }
```

Replace with:

```css
  #gen { color:#cdd6e0; }
  #stabBadge.dead    { background:#4a2020; color:#e08a8a; }
  #stabBadge.full    { background:#203a4a; color:#8ac6e0; }
  #stabBadge.stable  { background:#204a2a; color:#8ae0a0; }
  #stabBadge.chaotic { background:#4a3a20; color:#e0c48a; }
```

- [ ] **Step 3: Create the ring-stats instance and update function**

Find:

```js
let generation = 0;
```

Replace with:

```js
let generation = 0;
const liveStats = LifeStats.makeRingStats(LifeStats.THRESHOLDS.WINDOW);
function updateLiveBadge() {
  const badge = document.getElementById('stabBadge'), std = document.getElementById('stabStd');
  if (!liveStats.full()) { badge.textContent = 'warming up'; badge.className = ''; std.textContent = '—'; return; }
  const status = LifeStats.classify(liveStats.mean(), liveStats.stdDev(), LifeStats.THRESHOLDS);
  badge.textContent = status;
  badge.className = status;
  std.textContent = liveStats.stdDev().toFixed(3);
}
```

- [ ] **Step 4: Feed the ring buffer each generation, reset on Clear/Random/Apply-size**

Find:

```js
function updateStatus() {
  let pop = 0; for (const v of grid) pop += v;
  document.getElementById('gen').textContent = generation;
  document.getElementById('pop').textContent = pop;
}
```

Replace with:

```js
function updateStatus() {
  let pop = 0; for (const v of grid) pop += v;
  document.getElementById('gen').textContent = generation;
  document.getElementById('pop').textContent = pop;
  liveStats.push(pop / (COLS * ROWS));
  updateLiveBadge();
}
```

Find:

```js
function freshStart() {
  generation = 0;
  paint(); updateStatus();
}
```

Replace with:

```js
function freshStart() {
  generation = 0;
  liveStats.reset();
  paint(); updateStatus();
}
```

- [ ] **Step 5: Manual browser verification**

Open `life-stochastic.html`, click Play.
Expected:
- Badge shows "warming up" for the first `WINDOW` (50) generations, then flips to one of dead/full/stable/chaotic with a matching background color and a numeric stdDev.
- Click Random then Clear: badge resets to "warming up" each time (ring buffer reset confirmed).
- Leave the default B3/S23 rule running long enough (a couple hundred generations, bump Speed slider up) — population should trend toward "dead" or "chaotic", since pure deterministic-shaped B3/S23 with default density isn't tuned for long-term stability; this is expected and exactly the situation Task 5's search is meant to help escape.

- [ ] **Step 6: Commit**

```bash
git add life-stochastic.html
git commit -m "feat: live population-ratio stability badge"
```

---

### Task 5: Batch config search (headless trials, results list, click-to-apply)

**Files:**
- Modify: `life-stochastic.html`

**Interfaces:**
- Consumes: `LifeStats.runHeadlessTrial`, `LifeStats.THRESHOLDS` (Tasks 1-2), `bParm`/`sParm`/`buildTables()`/`wireSlider` targets (existing file).
- Produces: `runSearch()` function and a results-list UI section; no new interfaces needed by later tasks (this is the last task).

- [ ] **Step 1: Add the Search button + results panel markup**

Find:

```html
  <button id="view2d" style="margin-top:8px; width:100%">▦ 2D view</button>
```

Replace with:

```html
  <button id="search" style="margin-top:8px; width:100%">🔍 Search stable configs</button>
  <div id="searchResults" style="display:none; margin-top:6px; max-height:220px; overflow-y:auto;"></div>

  <button id="view2d" style="margin-top:8px; width:100%">▦ 2D view</button>
```

- [ ] **Step 2: Add result-row styling**

Find:

```css
  .row button { flex:1; }
```

Replace with:

```css
  .row button { flex:1; }
  .searchRow { display:block; width:100%; text-align:left; margin-top:4px; font-size:10px;
        font-family:ui-monospace,monospace; padding:4px 6px; }
```

- [ ] **Step 3: Implement `runSearch()`**

Find:

```js
$('view2d').onclick = () => setView(!is2D);
```

Add immediately before it:

```js
const SEARCH_TRIALS = 30, SEARCH_COLS = 60, SEARCH_ROWS = 30, SEARCH_GENS = 300;
function randSlider(min, max) { return min + Math.random() * (max - min); } // matches slider ranges below
function runSearch() {
  const results = [];
  for (let i = 0; i < SEARCH_TRIALS; i++) {
    const trialBParm = { sigma: randSlider(0.05, 2.00), ceil: randSlider(0, 1), floor: randSlider(0, 1) };
    const trialSParm = { sigma: randSlider(0.05, 2.00), ceil: randSlider(0, 1), floor: randSlider(0, 1) };
    const density = +$('dens').value / 100;
    const r = LifeStats.runHeadlessTrial({
      cols: SEARCH_COLS, rows: SEARCH_ROWS, cwrap: Cwrap, rwrap: Rwrap,
      gens: SEARCH_GENS, windowSize: LifeStats.THRESHOLDS.WINDOW, thresholds: LifeStats.THRESHOLDS,
      birth, survive, bParm: trialBParm, sParm: trialSParm, density, rng: Math.random,
    });
    if (r.status === 'stable') results.push({ ...r, bParm: trialBParm, sParm: trialSParm });
  }
  results.sort((a, b) => a.stdDev - b.stdDev);
  renderSearchResults(results);
}
function renderSearchResults(results) {
  const panel = $('searchResults');
  panel.style.display = 'block';
  if (!results.length) { panel.textContent = 'No stable configs found in 30 trials — try again.'; return; }
  panel.innerHTML = '';
  results.forEach(res => {
    const btn = document.createElement('button');
    btn.className = 'searchRow';
    btn.textContent = `σ ${res.bParm.sigma.toFixed(2)}/${res.sParm.sigma.toFixed(2)} · ` +
      `ratio ${res.finalRatio.toFixed(2)} · stdDev ${res.stdDev.toFixed(3)}`;
    btn.onclick = () => {
      Object.assign(bParm, res.bParm);
      Object.assign(sParm, res.sParm);
      $('sigC').value = res.bParm.sigma * 100; $('sigCv').textContent = res.bParm.sigma.toFixed(2);
      $('ceilC').value = res.bParm.ceil * 100; $('ceilCv').textContent = res.bParm.ceil.toFixed(2);
      $('floorC').value = res.bParm.floor * 100; $('floorCv').textContent = res.bParm.floor.toFixed(2);
      $('sigS').value = res.sParm.sigma * 100; $('sigSv').textContent = res.sParm.sigma.toFixed(2);
      $('ceilS').value = res.sParm.ceil * 100; $('ceilSv').textContent = res.sParm.ceil.toFixed(2);
      $('floorS').value = res.sParm.floor * 100; $('floorSv').textContent = res.sParm.floor.toFixed(2);
      buildTables();
      panel.style.display = 'none';
    };
    panel.appendChild(btn);
  });
}
$('search').onclick = runSearch;
```

- [ ] **Step 4: Manual browser verification**

Open `life-stochastic.html`, click "🔍 Search stable configs".
Expected:
- Page freezes briefly (sub-second to a couple seconds — 30 × 60×30×300-generation headless trials) then a results list appears below the button, sorted best (lowest stdDev) first, or the "No stable configs found" message.
- Click a result row: sliders for both curves visibly jump to the clicked values, the chart updates, panel closes, and Play shows the live sim now running with those params — confirm the live badge trends toward "stable" faster than the B3/S23 default did in Task 4.
- Click Search again with different current density: results vary run to run (non-deterministic `Math.random`), consistent with the spec (no persistence required).

- [ ] **Step 5: Commit**

```bash
git add life-stochastic.html
git commit -m "feat: headless batch search for stable stochastic-life configs"
```

---

## Self-Review Notes

- **Spec coverage:** §1 (metric) → Task 1 (`classify`/`makeRingStats`) + Task 4 (wiring). §2 (live monitor) → Task 4. §3 (batch search) → Task 2 (`runHeadlessTrial`) + Task 5 (UI/orchestration). §4 (results UI) → Task 5. Constants table → Global Constants section + `THRESHOLDS`/`SEARCH_*` literals, values match exactly.
- **Type consistency:** `bParm`/`sParm` shape `{sigma, ceil, floor}` used identically in existing code (Task 3), `runHeadlessTrial` opts (Task 2), and search trial construction (Task 5). `classify()` return strings (`dead`/`full`/`stable`/`chaotic`) used identically in Task 4 badge and Task 5 filter.
- **No git repo currently exists at `C:\Users\xci\source`** (confirmed at session start) — the `git add`/`git commit` steps in this plan assume one exists by execution time. If not, either run `git init` first or skip the commit steps and note it — flag this to the user before Task 1's commit step.
