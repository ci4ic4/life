const test = require('node:test');
const assert = require('node:assert');
const C = require('./life-cube-core.js');
const { moves, moveByName, inverse, solvedCube, applyMove, applyMoves, isSolved, stateKey } = C;

const det = (M) =>
  M[0][0] * (M[1][1] * M[2][2] - M[1][2] * M[2][1]) -
  M[0][1] * (M[1][0] * M[2][2] - M[1][2] * M[2][0]) +
  M[0][2] * (M[1][0] * M[2][1] - M[1][1] * M[2][0]);
const close = (A, B) => A.every((row, i) => row.every((x, j) => Math.abs(x - B[i][j]) < 1e-12));
const I = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
const randomNames = (n, rng) => Array.from({ length: n }, () => moves[Math.floor(rng() * moves.length)].name);

test('26 cubelets: 6 centres, 12 edges, 8 corners', () => {
  const byType = [0, 0, 0, 0];
  for (const c of C.cubelets) byType[C.norm1(c)]++;
  assert.deepStrictEqual(byType, [0, 6, 12, 8]);
  assert.ok(isSolved(solvedCube));
});

test('every quarter turn is a proper rotation fixing its axis', () => {
  for (const m of moves) {
    assert.ok(close(C.matMul(C.transpose(m.M), m.M), I), m.name + ' orthogonal');
    assert.strictEqual(det(m.M), 1, m.name + ' det');
    assert.deepStrictEqual(C.position(m.v, m.M), m.v, m.name + ' fixes axis');
  }
});

test('Rodrigues frames are rigid at any angle and compose back to identity', () => {
  for (const v of C.unitVectors) {
    for (const theta of [0.1, 0.7, -1.3, Math.PI / 2]) {
      const M = C.rotation(v, theta);
      assert.ok(close(C.matMul(C.transpose(M), M), I));
      assert.ok(Math.abs(det(M) - 1) < 1e-12);
      assert.ok(close(C.matMul(M, C.rotation(v, -theta)), I));
    }
  }
});

test('standard notation: clockwise as seen from the turned face', () => {
  const at = (name, c) => C.position(c, moveByName[name].M);
  assert.deepStrictEqual(at('U', [1, 0, 1]), [0, -1, 1]);   // front-top edge -> left-top
  assert.deepStrictEqual(at('R', [1, 1, 0]), [0, 1, 1]);    // front-right -> top-right
  assert.deepStrictEqual(at('F', [1, 0, 1]), [1, 1, 0]);    // top-front -> right-front
  assert.deepStrictEqual(at('D', [1, 0, -1]), [0, 1, -1]);  // front-down -> right-down
  assert.deepStrictEqual(at('L', [1, -1, 0]), [0, -1, -1]); // front-left -> down-left
  assert.deepStrictEqual(at('B', [-1, 0, 1]), [-1, -1, 0]); // back-top -> back-left
});

test('every move has cycle length exactly 4', () => {
  for (const m of moves) {
    let cube = solvedCube;
    const seen = new Set();
    for (let k = 0; k < 4; k++) { seen.add(stateKey(cube)); cube = applyMove(m, cube); }
    assert.strictEqual(seen.size, 4, m.name);
    assert.strictEqual(stateKey(cube), stateKey(solvedCube), m.name);
  }
});

test('a move followed by its inverse cancels; so does a reversed inverse sequence', () => {
  for (const m of moves) assert.ok(isSolved(applyMove(inverse(m), applyMove(m, solvedCube))), m.name);
  const rng = C.mulberry32(11);
  for (let trial = 0; trial < 20; trial++) {
    const names = randomNames(40, rng);
    const undo = names.slice().reverse().map((n) => inverse(moveByName[n]).name);
    assert.ok(isSolved(applyMoves(undo, applyMoves(names, solvedCube))));
  }
});

test("(R U R' U') six times is the identity", () => {
  for (const [a, b] of [['R', 'U'], ['F', 'D'], ['L', 'B']]) {
    const sexy = [a, b, a + "'", b + "'"];
    assert.ok(isSolved(applyMoves(Array(6).fill(sexy).flat(), solvedCube)), a + b);
    assert.ok(!isSolved(applyMoves(Array(3).fill(sexy).flat(), solvedCube)), a + b + ' x3');
  }
});

test('solve a seeded scramble, replay the moves, end solved', () => {
  const rng = C.mulberry32(2);
  const { cube } = C.shuffle(solvedCube, 25, rng);
  assert.ok(!isSolved(cube));
  const phases = new Set();
  const solution = C.solve(cube, { rng, onProgress: (p) => phases.add(p.phase) });
  assert.deepStrictEqual([...phases].sort(), [1, 2, 3, 4, 5]);
  assert.ok(isSolved(applyMoves(solution, cube)));
});

// Cube states scrambled by eigencube.py (shuffle(solved_cube, iterations=25, seed)), one 3x3
// matrix per cubelet in the shared cubelet order, each written row-major as 9 of '-', '0', '+'.
// Same frame (x front, y right, z top) and same R c convention, so they import as-is.
const pythonScrambles = {
  7: '0-000-+00 00--000+0 0+000--00 +000-000- +000-000- 0+0+0000- -000+000- 0+000++00 -0000-0-0 00-+000-0 00+0+0-00 00-0-0-00 -000-000+ +000+000+ 0+000++00 00-0+0+00 0-000-+00 -0000+0+0 00-0+0+00 00-+000-0 -000-000+ +0000-0+0 00++000+0 00--000+0 00-0+0+00 0-000+-00',
  8: '00+0+0-00 00+0+0-00 00++000+0 -0000-0-0 +000+000+ 00+0-0+00 -000+000- +0000+0-0 00+-000-0 -0000+0+0 00-0+0+00 00-+000-0 0+0-0000+ 0+0-0000+ -000+000- +000+000+ 0+0-0000+ -000-000+ -000-000+ 00-+000-0 00+-000-0 +000+000+ +0000-0+0 00+-000-0 00+0+0-00 +0000-0+0',
  9: '0-000+-00 +000-000- +000+000+ 0+0-0000+ +0000-0+0 00+0-0+00 00+0+0-00 0-000+-00 -000+000- 00+0+0-00 00+0+0-00 +0000-0+0 -000-000+ 0+0-0000+ -0000+0+0 00+0+0-00 0+000--00 00+0+0-00 +000-000- 0+0+0000- 0-000-+00 +0000-0+0 -000+000- 00-0-0-00 +0000-0+0 +000+000+',
};
const decode = (text) => text.split(' ').map((m) => {
  const d = [...m].map((ch) => '-0+'.indexOf(ch) - 1);
  return [d.slice(0, 3), d.slice(3, 6), d.slice(6, 9)];
});

test('solves cube states scrambled by eigencube.py', () => {
  for (const [seed, text] of Object.entries(pythonScrambles)) {
    const cube = C.cubeFromMatrices(decode(text));
    assert.strictEqual(cube.length, 26);
    assert.ok(!isSolved(cube), 'seed ' + seed);
    const solution = C.solve(cube, { rng: C.mulberry32(+seed) });
    assert.ok(isSolved(applyMoves(solution, cube)), 'seed ' + seed);
  }
});
