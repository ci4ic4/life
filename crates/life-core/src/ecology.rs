//! Spatial-ecology CA: squirrels and pine martens on a torus of trees.
//!
//! A port of `life-ecology-core.js`, which stays the reference implementation —
//! the page runs it, and a seed must replay identically in both. That is what
//! fixes the numeric types here: the JS arrays are `Uint8Array`/`Float32Array`,
//! so `grid`/`infected` are `u8` and `energy`/`env` are `f32`, but every
//! *parameter* is a plain JS number, so parameters and all intermediate
//! arithmetic are `f64` and round to `f32` only on store. Computing in `f32`
//! throughout tracks the reference for a while and then drifts;
//! `tests/ecology_parity.rs` is the check that keeps it honest.
//!
//! Prey grow by contact process; the marten is a conserved predator with an
//! energy counter. No B/S rule.

use crate::topology::{resolve_idx, Topology};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    Empty = 0,
    Red = 1,
    Grey = 2,
    Marten = 3,
}

impl CellState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => CellState::Red,
            2 => CellState::Grey,
            3 => CellState::Marten,
            _ => CellState::Empty,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EcologyParams {
    pub beta_red: f64,
    pub beta_grey: f64,
    pub sigma: f64,
    pub delta: f64,
    pub g: f64,
    pub e_breed: f64,
    pub e0: f64,
    pub breed_cost: f64,
    pub e_cap: f64,
    pub mu: f64,
    pub evade_red: f64,
    pub evade_grey: f64,
    pub forage: f64,
    pub mate: bool,
    pub mortality: f64,
    pub pox_beta: f64,
    pub pox_lethal: f64,
    pub terrain: f64,
    pub gen: u64,
}

impl Default for EcologyParams {
    fn default() -> Self {
        Self {
            beta_red: 0.10,
            beta_grey: 0.14,
            sigma: 0.92,
            delta: 1.0,
            g: 3.0,
            e_breed: 10.0,
            e0: 5.0,
            breed_cost: 5.0,
            e_cap: 15.0,
            mu: 0.5,
            evade_red: 0.0,
            evade_grey: 0.0,
            forage: 0.0,
            mate: false,
            mortality: 0.0,
            pox_beta: 0.0,
            pox_lethal: 0.0,
            terrain: 0.0,
            gen: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EcologyState {
    pub cols: u32,
    pub rows: u32,
    pub grid: Vec<u8>,
    pub energy: Vec<f32>,
    pub infected: Vec<u8>,
    pub env: Option<Vec<f32>>,
}

impl EcologyState {
    pub fn new(cols: u32, rows: u32) -> Self {
        let n = (cols * rows) as usize;
        Self {
            cols,
            rows,
            grid: vec![0; n],
            energy: vec![0.0; n],
            infected: vec![0; n],
            env: None,
        }
    }

    #[inline]
    pub fn idx(&self, c: u32, r: u32) -> usize {
        (r as usize) * (self.cols as usize) + (c as usize)
    }

    #[inline]
    pub fn get(&self, c: u32, r: u32) -> CellState {
        CellState::from_u8(self.grid[self.idx(c, r)])
    }

    #[inline]
    pub fn set(&mut self, c: u32, r: u32, state: CellState) {
        let i = self.idx(c, r);
        self.grid[i] = state as u8;
    }

    #[inline]
    pub fn get_energy(&self, c: u32, r: u32) -> f32 {
        self.energy[self.idx(c, r)]
    }

    #[inline]
    pub fn set_energy(&mut self, c: u32, r: u32, energy: f32) {
        let i = self.idx(c, r);
        self.energy[i] = energy;
    }

    #[inline]
    pub fn get_infected(&self, c: u32, r: u32) -> u8 {
        self.infected[self.idx(c, r)]
    }

    #[inline]
    pub fn set_infected(&mut self, c: u32, r: u32, inf: u8) {
        let i = self.idx(c, r);
        self.infected[i] = inf;
    }
}

const MOORE: [(i32, i32); 8] = [
    (-1, -1), (0, -1), (1, -1),
    (-1,  0),          (1,  0),
    (-1,  1), (0,  1), (1,  1),
];

pub fn has_empty_neighbour(grid: &[u8], c: i32, r: i32, cols: u32, rows: u32, topo: Topology) -> bool {
    for &(dc, dr) in &MOORE {
        if let Some(i) = resolve_idx(c + dc, r + dr, cols, rows, topo) {
            if grid[i] == CellState::Empty as u8 {
                return true;
            }
        }
    }
    false
}

pub fn count_marten_neighbours(grid: &[u8], c: i32, r: i32, cols: u32, rows: u32, topo: Topology) -> u32 {
    let mut n = 0;
    for &(dc, dr) in &MOORE {
        if let Some(i) = resolve_idx(c + dc, r + dr, cols, rows, topo) {
            if grid[i] == CellState::Marten as u8 {
                n += 1;
            }
        }
    }
    n
}

pub fn count_infected_neighbours(grid: &[u8], infected: &[u8], c: i32, r: i32, cols: u32, rows: u32, topo: Topology) -> u32 {
    let mut n = 0;
    for &(dc, dr) in &MOORE {
        if let Some(i) = resolve_idx(c + dc, r + dr, cols, rows, topo) {
            if infected[i] != 0 && (grid[i] == CellState::Red as u8 || grid[i] == CellState::Grey as u8) {
                n += 1;
            }
        }
    }
    n
}

/// Deterministic arbitration priority for the ordered pair (marten, prey, gen).
pub fn prio(m_idx: usize, p_idx: usize, gen: u64) -> u32 {
    let mut h = (((m_idx + 1) as u32).wrapping_mul(1973))
        ^ (((p_idx + 1) as u32).wrapping_mul(9277))
        ^ (((gen + 1) as u32).wrapping_mul(26699));
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846ca68b);
    h ^= h >> 16;
    h
}

/// Advance the simulation one generation.
///
/// Two phases with an intent buffer (`claim`): (1) each marten claims its
/// highest-priority prey neighbour; (2) each prey is eaten by the
/// highest-priority marten that claimed it, unless it evades. So one prey dies
/// at most once and one marten eats at most once, and a missed hunt — lost the
/// arbitration, or the prey escaped — feeds nobody.
pub fn step_ecology<R: FnMut() -> f64>(
    state: &EcologyState,
    topo: Topology,
    params: &EcologyParams,
    rng: &mut R,
) -> EcologyState {
    let cols = state.cols;
    let rows = state.rows;
    let n = (cols * rows) as usize;

    let mut next_grid = vec![0u8; n];
    let mut next_energy = vec![0.0f32; n];
    let mut next_infected = vec![0u8; n];

    let is_prey = |v: u8| v == CellState::Red as u8 || v == CellState::Grey as u8;

    // PHASE 1: each hungry marten claims one prey neighbour (highest priority)
    let mut claim = vec![-1i32; n];
    for r in 0..rows as i32 {
        for c in 0..cols as i32 {
            let m = (r * cols as i32 + c) as usize;
            if state.grid[m] != CellState::Marten as u8 || state.energy[m] as f64 >= params.e_cap {
                continue;
            }
            let mut best = -1i32;
            let mut best_p = 0u32;
            for &(dc, dr) in &MOORE {
                if let Some(i) = resolve_idx(c + dc, r + dr, cols, rows, topo) {
                    if !is_prey(state.grid[i]) {
                        continue;
                    }
                    // NB: prio() takes (marten, prey) here and in phase 2a. Both
                    // directions must pass the same ordered pair or the two
                    // phases disagree on the winner.
                    let pr = prio(m, i, params.gen);
                    if best < 0 || pr > best_p {
                        best = i as i32;
                        best_p = pr;
                    }
                }
            }
            claim[m] = best;
        }
    }

    // PHASE 2a: resolve who eats whom
    let mut eaten = vec![0u8; n];
    let mut ate = vec![0u8; n];
    for r in 0..rows as i32 {
        for c in 0..cols as i32 {
            let p = (r * cols as i32 + c) as usize;
            if !is_prey(state.grid[p]) {
                continue;
            }
            let mut winner = -1i32;
            let mut best_p = 0u32;
            for &(dc, dr) in &MOORE {
                if let Some(i) = resolve_idx(c + dc, r + dr, cols, rows, topo) {
                    if state.grid[i] != CellState::Marten as u8 || claim[i] != p as i32 {
                        continue;
                    }
                    let pr = prio(i, p, params.gen);
                    if winner < 0 || pr > best_p {
                        winner = i as i32;
                        best_p = pr;
                    }
                }
            }
            if winner < 0 {
                continue;
            }
            let evade = if state.grid[p] == CellState::Red as u8 {
                params.evade_red
            } else {
                params.evade_grey
            };
            if evade > 0.0 && rng() < evade {
                continue;
            }
            eaten[p] = 1;
            ate[winner as usize] = 1;
        }
    }

    // Breeding eligibility, computed once because both the parent (which pays the
    // breed cost) and the empty cell (which counts eligible neighbours) must agree.
    // With `mate`, a marten also needs an adjacent marten: one animal cannot found
    // a population, so a reintroduction needs a viable *group* rather than a lucky
    // individual. It pulls against forage, which is divided by that same crowding.
    let mut can_breed = vec![0u8; n];
    for r in 0..rows as i32 {
        for c in 0..cols as i32 {
            let i = (r * cols as i32 + c) as usize;
            if state.grid[i] != CellState::Marten as u8 || (state.energy[i] as f64) < params.e_breed {
                continue;
            }
            if !params.mate || count_marten_neighbours(&state.grid, c, r, cols, rows, topo) > 0 {
                can_breed[i] = 1;
            }
        }
    }

    // PHASE 2b: build next grid + energy
    for r in 0..rows as i32 {
        for c in 0..cols as i32 {
            let here = (r * cols as i32 + c) as usize;
            let v = state.grid[here];

            if v == CellState::Marten as u8 {
                if params.mortality > 0.0 && rng() < params.mortality {
                    next_grid[here] = CellState::Empty as u8;
                    next_energy[here] = 0.0;
                    continue;
                }
                let mut e = state.energy[here] as f64 - params.delta + if ate[here] != 0 { params.g } else { 0.0 };
                // Alternative prey (voles, birds, berries). It must be a SHARED
                // resource: a flat term would be algebraically identical to
                // lowering delta, so it would add no dynamics at all. Dividing by
                // the local marten count makes it saturate with crowding, which
                // gives martens a baseline density set by the background food
                // rather than by squirrels. forage=0 disables.
                if params.forage > 0.0 {
                    e += params.forage / (1.0 + count_marten_neighbours(&state.grid, c, r, cols, rows, topo) as f64);
                }
                if can_breed[here] != 0 && has_empty_neighbour(&state.grid, c, r, cols, rows, topo) {
                    e -= params.breed_cost;
                }
                if e <= 0.0 {
                    next_grid[here] = CellState::Empty as u8;
                    next_energy[here] = 0.0;
                } else {
                    next_grid[here] = CellState::Marten as u8;
                    next_energy[here] = if e < params.e_cap { e as f32 } else { params.e_cap as f32 };
                }
                continue;
            }

            if is_prey(v) {
                if !(eaten[here] == 0 && rng() < params.sigma) {
                    next_grid[here] = CellState::Empty as u8;
                    continue;
                }
                next_grid[here] = v;
                // Squirrelpox. Greys are an asymptomatic reservoir: they carry it
                // for life and pay nothing. Reds are killed by it. That asymmetry
                // runs opposite to the evasion asymmetry, which is the point.
                if params.pox_beta > 0.0 {
                    let mut inf = state.infected[here];
                    if inf == 0 {
                        let n_inf = count_infected_neighbours(&state.grid, &state.infected, c, r, cols, rows, topo);
                        if n_inf > 0 {
                            let p_trans = 1.0 - (1.0 - params.pox_beta).powi(n_inf as i32);
                            if rng() < p_trans {
                                inf = 1;
                            }
                        }
                    }
                    if inf != 0 && v == CellState::Red as u8 && rng() < params.pox_lethal {
                        next_grid[here] = CellState::Empty as u8;
                        continue;
                    }
                    next_infected[here] = inf;
                }
                continue;
            }

            // EMPTY cell colonization
            let mut n_red = 0u32;
            let mut n_grey = 0u32;
            let mut n_elig = 0u32;
            for &(dc, dr) in &MOORE {
                if let Some(i) = resolve_idx(c + dc, r + dr, cols, rows, topo) {
                    let g = state.grid[i];
                    if g == CellState::Red as u8 {
                        n_red += 1;
                    } else if g == CellState::Grey as u8 {
                        n_grey += 1;
                    } else if g == CellState::Marten as u8 && can_breed[i] != 0 {
                        n_elig += 1;
                    }
                }
            }

            // Terrain modulates the GREY only. The grey's edge is an acorn-and-hazel
            // edge: a property of broadleaf, not of the animal. env<0 is conifer,
            // where that edge is worth nothing; env>0 is broadleaf, where it is worth
            // a lot. So beta_grey > beta_red is not intrinsic — the invasive advantage
            // exists only in the habitat that grants it.
            //
            // Red stays flat on purpose. Reds held the whole country, broadleaf
            // included, before the greys came: they are outcompeted there, not
            // excluded. Scaling red down in broadleaf would brick the door behind
            // them, and they could never recolonise ground the marten had cleared.
            // The marten works both habitats equally well, so p_mart is untouched.
            let env_val = if params.terrain > 0.0 {
                state.env.as_ref().map_or(0.0, |e| e[here] as f64)
            } else {
                0.0
            };
            let e_term = if params.terrain > 0.0 { params.terrain * env_val } else { 0.0 };
            let b_grey = if e_term != 0.0 {
                (params.beta_grey * (1.0 + e_term)).clamp(0.0, 1.0)
            } else {
                params.beta_grey
            };

            let p_red = if n_red > 0 { 1.0 - (1.0 - params.beta_red).powi(n_red as i32) } else { 0.0 };
            let p_grey = if n_grey > 0 { 1.0 - (1.0 - b_grey).powi(n_grey as i32) } else { 0.0 };
            let p_mart = if n_elig > 0 { 1.0 - (1.0 - params.mu).powi(n_elig as i32) } else { 0.0 };

            // One draw partitions the empty cell: red slice first, then grey, then
            // marten. Red-first is a fixed convention; grey still wins via higher
            // beta when reds are scarce, which is the invasive-advantage story.
            let roll = rng();
            if roll < p_red {
                next_grid[here] = CellState::Red as u8;
            } else if roll < p_red + p_grey {
                next_grid[here] = CellState::Grey as u8;
            } else if roll < p_red + p_grey + p_mart {
                next_grid[here] = CellState::Marten as u8;
                next_energy[here] = params.e0 as f32;
            } else {
                next_grid[here] = CellState::Empty as u8;
            }
        }
    }

    EcologyState {
        cols,
        rows,
        grid: next_grid,
        energy: next_energy,
        infected: next_infected,
        env: state.env.clone(),
    }
}
