# Life-Cube — Eigencube in the Browser (3D Rubik's cube + linear-algebra solver)

**Date:** 2026-10-10
**Status:** approved (design)
**Files:** `life-cube-core.js`, `life-cube.html`, `life-cube-core.test.js` (all new); `index.html`, `CLAUDE.md` (edited)

## Goal

A JavaScript port of [eigencube](https://github.com/smolkaj/eigencube) (`eigencube.py`) as a
browser simulation: a 3D Rubik's cube you can turn by hand, scramble, and hand to the solver
at any point, with the solution played back move by move.

The point is the representation, not the puzzle. Eigencube replaces sticker bookkeeping with
linear algebra:

- a cubelet is its home vector `c ∈ {-1,0,1}³` plus a rotation matrix `R`;
- its position is `Rc`, its sticker directions are the columns of `R·diag(c)`;
- a move on the face with axis `v` multiplies `R ← M·R` for every cubelet with `v·(Rc) > 0`;
- the cube is solved when `R·diag(c) = diag(c)` for every cubelet;
- the six centres are fixed eigenvectors; colours are the unit vectors `±e`.

The page should make that model visible. A spinning cube on its own is not interesting
(there are hundreds on the web); a cube whose every frame is drawn from `(c, R)` and whose
cubelets show their matrices is.

The solver is still a search: five phases of A\* with move-budgeted randomized restarts and
a per-cubelet heuristic that is itself a tiny A\* over rotations. The linear algebra replaces
the bookkeeping, not the search.

Secondary goal: get a feel for where a browser JS implementation sits relative to the Python
one, as input to a possible later Rust/wasm port. This is not a benchmark; the solving panel
just shows moves simulated, elapsed time and restarts.

## Non-goals

- Bit-exact replay of the Python solver. Python's `random` is Mersenne Twister; this port
  uses `mulberry32` like the other sims. Same algorithm, not same run.
- Mouse-drag face turning. Keyboard only, with an on-screen key legend.
- Optimal or short solutions. The solver produces what eigencube produces (hundreds of moves).

## The move-matrix convention (a fix, not a copy)

`eigencube.py`'s `rotation_matrix` ignores the sign of `v` and builds the quarter turn from
the index order of the two rotating axes, so each face shares its opposite face's matrix.
As a result "direction +1" is U, F and L as labelled, but R′, D′ and B′ for right, bottom
and back. The solver doesn't care (it uses both directions), but `describe_move`'s
"clockwise" is wrong for three of six faces.

The JS core derives every rotation from the axis–angle (Rodrigues) formula:

```
M(v, θ) = I + sin θ·[v]ₓ + (1 − cos θ)·[v]ₓ²
```

with "clockwise as seen from face `v`" being `θ = −90°`. At `±90°` this yields the exact
integer matrices (rounded to kill float noise); at intermediate `θ` it is the animation frame.
One formula for both the move and its animation.

Coordinates follow eigencube: `x` = front, `y` = right, `z` = top. Standard notation:
`U = +z`, `D = −z`, `F = +x`, `B = −x`, `R = +y`, `L = −y`; a prime is `θ = +90°`.

## Architecture

### `life-cube-core.js` — pure model and solver

UMD-lite like the other cores (browser global and Node `require()`), with one twist: the whole
module body is a single factory function, so the page can ship it into a Worker via
`factory.toString()` (see below). No DOM, no globals, RNG injected.

Target: under 400 lines, mirroring eigencube's own budget.

- `cubelets` (26 home vectors `c`), `unitVectors`, `solvedCube` — a cube is an array of 26
  interned 3×3 integer matrices `R`, parallel to `cubelets` (so `c` is stored once).
- `moves` — 12 named moves `{name, v, theta}` (`U`, `U'`, …).
- `rotation(v, theta)` — Rodrigues; `quarter(v, sign)` the rounded integer case.
- `applyMove(move, cube)`, `isCubeletSolved`, `isSolved`, `key(cube)` (string state key for
  the visited map).
- `shuffle(cube, n, rng)`.
- `astar(...)` and the five phases, ported from `eigencube.py`: top & middle layer (17
  cubelets), bottom cross (edges), bottom corners (positions), endgame (the `(L′ U′ L U)×2`
  routine plus D turns). Same heuristics (`min_moves_to_solved`, `min_moves_to_position`,
  the p = 0.5 norms and their divisors), same budgets (100 000, ×1.5 on restart), same
  inverse-move and commuting-opposite-face pruning. Memoization by `Map` keyed on
  `(cubelet, R)` strings in place of `functools.cache`.
- `solve(cube, {rng, onProgress})` → array of move names. `onProgress` receives
  `{phase, step, movesSimulated, restarts}`.

Integer matrices are plain nested arrays, so a later Rust port (`life-core` style:
dependency-free, RNG as a closure) is mechanical.

### Worker from a Blob

The solver runs 5–25 s in Python; on the main thread it would freeze the page. Chrome refuses
`new Worker('file.js')` from `file://`, and these pages must run from `file://`. So the page
builds the worker from source text: `new Worker(URL.createObjectURL(new Blob([...])))`,
where the blob contains the core factory's `toString()` plus a ~15-line message handler.
Blob workers work from `file://`. No separate worker file, no main-thread fallback.

### `life-cube.html` — the shell

Three.js 0.128 + OrbitControls from jsdelivr, same as the CA sims. `:root { color-scheme: dark }`
kept per the repo invariant.

- **Scene.** 26 cubelet meshes (dark rounded body, coloured sticker quads on the faces in
  `diag(c)`). Each mesh's matrix is `M(v, θ(t))·R` when it's in the turning slice, else `R`,
  with translation `Rc`. Eigencube's `(x, y, z)` maps to three.js `(y, z, x)` (a cyclic
  permutation, det +1) so front faces the camera, top is up. Default camera shows F, U, R.
- **Modes.**

  | Mode | Keys | Behaviour |
  |---|---|---|
  | manual | `U D F B L R` clockwise, Shift for prime; Space scramble; Enter solve | Presses join a queue; each quarter turn animates ~150 ms. Nothing dropped, state never mid-turn. |
  | solving | Esc cancels (`worker.terminate()`) | Face keys disabled. Panel: phase 1–5, step, moves simulated, elapsed, restarts. |
  | playback | ←/→ step; holding repeats fast | "move k / n" plus the move in notation. Any face key or scramble discards the solution → manual. |

- **Scramble.** 25 random moves from `mulberry32(seed)`, applied instantly; seed shown and
  editable, auto-increments.
- **Inspector.** Click a cubelet (three.js `Raycaster`): shows `c`, `R`, `Rc`, `R·diag(c)`,
  type by `‖c‖₁` (centre/edge/corner), solved or not. Live-updated after each move.
- **Key legend.** Always visible beside the canvas.

## Testing

`life-cube-core.test.js`, run with `node --test`. Round-trip tests that catch whole classes
of rotation bugs, not single cases:

- every move applied 4× is the identity;
- `(R U R′ U′)` applied 6× is the identity;
- every move followed by its inverse is the identity; a random sequence followed by its
  reversed inverses is the identity;
- `quarter(v, ±1)` is orthogonal with determinant 1 and fixes `v`;
- `rotation(v, ±90°)` rounds to `quarter(v, ∓1)`;
- standard-notation sanity: `U` sends the front-top edge to left-top, `R` sends front-right
  to top-right, `F` sends top-front to right-front;
- solve a seeded scramble, replay the returned moves, end solved;
- parity fixture: a few scrambles produced by `eigencube.py`, translated to named moves
  (accounting for the Python convention above) and committed as JSON inside the test; the
  JS solver must solve each.

Visual check: headless Chromium screenshot of the page (scrambled, inspector open), attached
to the PR.

## Docs

- `CLAUDE.md`: add `life-cube-core.js | life-cube.html | life-cube-core.test.js` to the
  core/shell table and a file-list entry; bump the JS test count.
- `index.html`: a card for `life-cube`.
