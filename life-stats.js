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
