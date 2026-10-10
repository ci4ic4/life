// Eigencube core (shared by life-cube.html, its solver Worker and its Node tests).
// A JS port of github.com/smolkaj/eigencube: the Rubik's cube as linear algebra.
// A cubelet is its home vector c in {-1,0,1}^3; the cube state is one rotation
// matrix R per cubelet. The cubelet sits at R c, its stickers face the columns
// of R diag(c), and it is solved when R diag(c) = diag(c).
// Pure: no DOM, no Three.js. The module body is one function so the page can
// ship it into a Worker as source text (factory.toString()).
(function (root, factory) {
  const mod = factory();
  mod.factory = factory;
  if (typeof module !== 'undefined' && module.exports) module.exports = mod;
  else root.LifeCube = mod;
})(typeof self !== 'undefined' ? self : this, function () {
  // ---- vectors and matrices ----------------------------------------------
  // Coordinates: x = front, y = right, z = top.
  const range = [-1, 0, 1];
  const vectors = [];
  for (const x of range) for (const y of range) for (const z of range) vectors.push([x, y, z]);
  const norm1 = (v) => Math.abs(v[0]) + Math.abs(v[1]) + Math.abs(v[2]);
  const unitVectors = vectors.filter((v) => norm1(v) === 1);
  const cubelets = vectors.filter((v) => norm1(v) > 0); // 26; the hidden core is not a cubelet

  const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const matVec = (M, v) => M.map((row) => dot(row, v));
  const matMul = (A, B) => A.map((row) => [0, 1, 2].map((j) => row[0] * B[0][j] + row[1] * B[1][j] + row[2] * B[2][j]));
  const transpose = (M) => [0, 1, 2].map((i) => M.map((row) => row[i]));
  const diag = (v) => [[v[0], 0, 0], [0, v[1], 0], [0, 0, v[2]]];
  const identity = diag([1, 1, 1]);
  const sameVec = (a, b) => a[0] === b[0] && a[1] === b[1] && a[2] === b[2];

  // Rodrigues: rotation by angle theta about unit axis v,
  //   M = I + sin(theta) [v]x + (1 - cos(theta)) [v]x^2.
  // theta = -pi/2 is a clockwise quarter turn as seen from the face v points at.
  function rotation(v, theta) {
    const K = [[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]];
    const K2 = matMul(K, K), s = Math.sin(theta), c = 1 - Math.cos(theta);
    return [0, 1, 2].map((i) => [0, 1, 2].map((j) => identity[i][j] + s * K[i][j] + c * K2[i][j]));
  }

  // Interning: every reachable R is one of the 24 cube rotations; give each
  // distinct matrix one shared array so states can be keyed and cached cheaply.
  const internedByKey = new Map(), idOf = new Map();
  function intern(M) {
    const key = M.join();
    let R = internedByKey.get(key);
    if (!R) { R = M; internedByKey.set(key, R); idOf.set(R, String.fromCharCode(65 + idOf.size)); }
    return R;
  }
  const quarterTurn = (v, sign) => intern(rotation(v, sign * Math.PI / 2).map((row) => row.map(Math.round)));
  const I = intern(identity);

  // ---- moves ----------------------------------------------------------------
  const faceNames = { U: [0, 0, 1], D: [0, 0, -1], F: [1, 0, 0], B: [-1, 0, 0], R: [0, 1, 0], L: [0, -1, 0] };
  const moves = [];
  for (const [face, v] of Object.entries(faceNames)) {
    for (const prime of [false, true]) {
      moves.push({ name: face + (prime ? "'" : ''), face, v, theta: prime ? Math.PI / 2 : -Math.PI / 2,
        M: quarterTurn(v, prime ? 1 : -1), index: moves.length });
    }
  }
  const moveByName = Object.fromEntries(moves.map((m) => [m.name, m]));
  const inverse = (move) => moveByName[move.name.endsWith("'") ? move.face : move.face + "'"];
  const opposite = (a, b) => a.v[0] === -b.v[0] && a.v[1] === -b.v[1] && a.v[2] === -b.v[2];

  // ---- cube state -----------------------------------------------------------
  // A cube is an array of 26 interned rotation matrices, parallel to `cubelets`.
  const solvedCube = cubelets.map(() => I);
  const position = (c, R) => matVec(R, c);
  const turnCache = moves.map(() => cubelets.map(() => new Map()));
  function turnCubelet(move, i, R) {
    const cache = turnCache[move.index][i];
    let next = cache.get(R);
    if (!next) {
      next = dot(move.v, position(cubelets[i], R)) > 0 ? intern(matMul(move.M, R)) : R;
      cache.set(R, next);
    }
    return next;
  }
  let movesSimulated = 0;
  function applyMove(move, cube) {
    movesSimulated++;
    return cube.map((R, i) => turnCubelet(move, i, R));
  }
  const cubeFromMatrices = (Rs) => Rs.map((R) => intern(R.map((row) => row.slice())));
  const applyMoves = (names, cube) => names.reduce((c, n) => applyMove(moveByName[n], c), cube);
  const stateKey = (cube) => cube.map((R) => idOf.get(R)).join('');

  const solvedCache = new Map();
  function isCubeletSolved(i, R) {
    const key = i + idOf.get(R);
    let solved = solvedCache.get(key);
    if (solved === undefined) {
      const colors = diag(cubelets[i]);
      solved = matMul(R, colors).join() === colors.join();
      solvedCache.set(key, solved);
    }
    return solved;
  }
  const isSolved = (cube) => cube.every((R, i) => isCubeletSolved(i, R));
  const countSolved = (cube, criterion) => cube.reduce((n, R, i) => n + (criterion(cubelets[i]) && isCubeletSolved(i, R) ? 1 : 0), 0);

  function shuffle(cube, n, rng) {
    const names = [];
    for (let k = 0; k < n; k++) names.push(moves[Math.floor(rng() * moves.length)].name);
    return { cube: applyMoves(names, cube), names };
  }

  // ---- A* search --------------------------------------------------------------
  function heapPush(heap, item) {
    heap.push(item);
    let k = heap.length - 1;
    while (k > 0) {
      const p = (k - 1) >> 1;
      if (heap[p].priority <= item.priority) break;
      heap[k] = heap[p]; k = p;
    }
    heap[k] = item;
  }
  function heapPop(heap) {
    const top = heap[0], last = heap.pop();
    if (heap.length) {
      let k = 0;
      for (;;) {
        let child = 2 * k + 1;
        if (child >= heap.length) break;
        if (child + 1 < heap.length && heap[child + 1].priority < heap[child].priority) child++;
        if (heap[child].priority >= last.priority) break;
        heap[k] = heap[child]; k = child;
      }
      heap[k] = last;
    }
    return top;
  }
  function gauss(rng, mean, sd) {
    const u = 1 - rng(), v = rng();
    return mean + sd * Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
  }

  // Best-first search over move sequences. `key` maps a state to a string.
  // When the move budget runs out and randomWeight > 0, restart with 1.5x the
  // budget and freshly randomized heuristic weights (eigencube's escape from plateaus).
  function astar({ start, isGoal, apply, key, heuristic = () => 0, randomWeight = 0,
                   maxMoves = 100000, rng = Math.random, onRestart = () => {} }) {
    if (isGoal(start)) return { state: start, path: [] };
    let budget = maxMoves;
    for (;;) {
      const visited = new Map([[key(start), { cost: 0, state: start, from: null, move: null }]]);
      const frontier = [{ node: visited.get(key(start)), priority: 0 }];
      let simulated = 0, exceeded = false;
      while (frontier.length && !exceeded) {
        const src = heapPop(frontier).node;
        for (const move of moves) {
          const last = src.move;
          // Never undo the last move; opposite faces commute, so fix their order.
          if (last && (move === inverse(last) || (opposite(last, move) && last.index > move.index))) continue;
          const state = apply(move, src.state), cost = src.cost + 1, k = key(state);
          exceeded = ++simulated >= budget;
          const seen = visited.get(k);
          if (seen && seen.cost <= cost) { if (exceeded) break; continue; }
          const node = { cost, state, from: src, move };
          visited.set(k, node);
          if (isGoal(state)) {
            const path = [];
            for (let n = node; n.move; n = n.from) path.push(n.move);
            return { state, path: path.reverse() };
          }
          if (exceeded) break;
          const weight = randomWeight ? gauss(rng, 1, randomWeight) : 1;
          heapPush(frontier, { node, priority: cost + weight * heuristic(state) });
        }
      }
      if (!exceeded || !randomWeight) return null;
      onRestart(budget);
      budget = Math.max(Math.floor(1.5 * budget), budget + 1);
    }
  }

  // ---- heuristics: how far is each cubelet from home, alone? -----------------
  const cubeletSearch = (isDone) => {
    const cache = new Map();
    return (i, R) => {
      const k = i + idOf.get(R);
      let d = cache.get(k);
      if (d === undefined) {
        d = astar({ start: R, isGoal: (r) => isDone(i, r), key: (r) => idOf.get(r),
                    apply: (move, r) => intern(matMul(move.M, r)) }).path.length;
        cache.set(k, d);
      }
      return d;
    };
  };
  const minMovesToSolved = cubeletSearch(isCubeletSolved);
  const minMovesToPosition = cubeletSearch((i, R) => sameVec(position(cubelets[i], R), cubelets[i]));

  // p-norm with p = 1/2 over the selected cubelets' distances, scaled by n.
  const halfNorm = (cube, select, dist = minMovesToSolved) => {
    let s = 0;
    cube.forEach((R, i) => { if (select(cubelets[i])) s += Math.sqrt(dist(i, R)); });
    return s * s;
  };
  const isTop = (c) => c[2] === 1, isTopOrMiddle = (c) => c[2] >= 0;
  const isTopEdge = (c) => c[2] === 1 && norm1(c) === 2;
  const isBottomEdge = (c) => c[2] === -1 && norm1(c) === 2;
  const isBottomCorner = (c) => c[2] === -1 && norm1(c) === 3;
  const topLayerHeuristic = (cube) => halfNorm(cube, isTop) / 8;
  const middleLayerHeuristic = (cube) => halfNorm(cube, isTopOrMiddle) / 4;
  const bottomEdgeHeuristic = (cube) => halfNorm(cube, (c) => !isBottomCorner(c)) / 3;
  const bottomCornerHeuristic = (cube) =>
    halfNorm(cube, isTop) / 5 + halfNorm(cube, (c) => c[2] === 0) / 3 +
    halfNorm(cube, (c) => c[2] === -1, (i, R) => (norm1(cubelets[i]) === 3 ? minMovesToPosition : minMovesToSolved)(i, R)) / 8;

  const down = [0, 0, -1];
  const hasYellowDown = (i, R) => sameVec(matVec(R, down), down);
  const edgesPositioned = (cube) => cube.reduce((n, R, i) => n + (isBottomEdge(cubelets[i]) && hasYellowDown(i, R) ? 1 : 0), 0);
  const cornersPositioned = (cube) => cube.reduce((n, R, i) =>
    n + (isBottomCorner(cubelets[i]) && sameVec(position(cubelets[i], R), cubelets[i]) ? 1 : 0), 0);

  // ---- the five phases ----------------------------------------------------------
  function solve(cube, { rng = Math.random, onProgress = () => {} } = {}) {
    const solution = [];
    let restarts = 0;
    const search = (phase, step, isGoal, heuristic, randomWeight) => {
      onProgress({ phase, step, movesSimulated, restarts, cube });
      const found = astar({ start: cube, isGoal, apply: applyMove, key: stateKey, heuristic,
                            randomWeight, rng, onRestart: () => restarts++ });
      cube = found.state;
      solution.push(...found.path.map((m) => m.name));
    };
    // Phase 1-2: top layer then middle layer, one more cubelet per search.
    for (let n = 0; n < 17; n++) {
      search(n < 9 ? 1 : 2, n + 1, (c) =>
        countSolved(c, isTopEdge) >= Math.min(4, n + 1) &&
        countSolved(c, isTop) >= Math.min(9, n + 1) &&
        countSolved(c, isTopOrMiddle) >= Math.min(17, n + 1),
      n < 9 ? topLayerHeuristic : middleLayerHeuristic, 0.25);
    }
    // Phase 3: bottom cross — first yellow faces down, then fully solved.
    for (let n = 0; n < 8; n++) {
      search(3, n + 1, (c) =>
        countSolved(c, isTopOrMiddle) === 17 && edgesPositioned(c) >= Math.min(4, n + 1) &&
        countSolved(c, isBottomEdge) >= Math.min(4, n - 3),
      bottomEdgeHeuristic, 0.25);
    }
    // Phase 4: bottom corners into their places (twist ignored).
    for (let n = 0; n < 4; n++) {
      search(4, n + 1, (c) =>
        countSolved(c, isTopOrMiddle) === 17 && countSolved(c, isBottomEdge) === 4 &&
        cornersPositioned(c) >= Math.min(4, n + 1),
      bottomCornerHeuristic, 0.3);
    }
    // Phase 5: twist each bottom corner home with (L' U' L U) x 2, turning D between.
    const apply = (name) => { solution.push(name); cube = applyMove(moveByName[name], cube); };
    const frontLeftDown = [1, -1, -1];
    const cornerOriented = () => {
      const i = cube.findIndex((R, j) => sameVec(position(cubelets[j], R), frontLeftDown));
      let R = cube[i];
      for (let k = 0; k < 4; k++) {
        if (isCubeletSolved(i, R)) return true;
        R = turnCubelet(moveByName["D'"], i, R);
      }
      return false;
    };
    for (let n = 0; n < 4; n++) {
      onProgress({ phase: 5, step: n + 1, movesSimulated, restarts, cube });
      while (!cornerOriented()) for (const name of ["L'", "U'", 'L', 'U', "L'", "U'", 'L', 'U']) apply(name);
      apply("D'");
    }
    while (!isSolved(cube)) apply("D'");
    onProgress({ phase: 5, step: 4, movesSimulated, restarts, cube });
    return solution;
  }

  function mulberry32(a) {
    return function () {
      a |= 0; a = (a + 0x6D2B79F5) | 0;
      let t = Math.imul(a ^ (a >>> 15), 1 | a);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  return { cubelets, unitVectors, moves, moveByName, inverse, rotation, quarterTurn, solvedCube,
           position, diag, matMul, transpose, applyMove, applyMoves, isCubeletSolved, isSolved,
           stateKey, cubeFromMatrices, shuffle, solve, mulberry32, norm1, movesSimulated: () => movesSimulated };
});
