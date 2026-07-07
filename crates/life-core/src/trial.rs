//! Headless stability trial (port of life-stats.js runHeadlessTrial).

use crate::grid::{step_stochastic, Grid};
use crate::prob::{curve, CurveParams};
use crate::stats::{classify, RingStats, Thresholds, Verdict};
use crate::topology::Topology;

#[derive(Clone, Copy, Debug)]
pub struct TrialOpts {
    pub w: u32,
    pub h: u32,
    pub topo: Topology,
    pub gens: u32,
    pub thresholds: Thresholds,
    pub birth: u16,
    pub survive: u16,
    pub b_parm: CurveParams,
    pub s_parm: CurveParams,
    pub density: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct TrialResult { pub mean: f64, pub std_dev: f64, pub status: Verdict }

pub fn run_trial(o: &TrialOpts, rng: &mut impl FnMut() -> f64) -> TrialResult {
    let mut grid = Grid::new(o.w, o.h);
    for c in grid.cells.iter_mut() {
        *c = (rng() < o.density) as u8;
    }
    let p_birth = curve(o.birth, o.b_parm);
    let p_survive = curve(o.survive, o.s_parm);
    let mut stats = RingStats::new(o.thresholds.window);
    let area = (o.w * o.h) as f64;
    for _ in 0..o.gens {
        grid = step_stochastic(&grid, &p_birth, &p_survive, o.topo, rng);
        let pop: u32 = grid.cells.iter().map(|&v| v as u32).sum();
        stats.push(pop as f64 / area);
    }
    let (mean, std_dev) = (stats.mean(), stats.std_dev());
    TrialResult { mean, std_dev, status: classify(mean, std_dev, &o.thresholds) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::Wrap;

    fn opts() -> TrialOpts {
        TrialOpts {
            w: 10, h: 10,
            topo: Topology { x: Wrap::Straight, y: Wrap::Straight },
            gens: 60,
            thresholds: Thresholds::DEFAULT,
            birth: 1 << 3,
            survive: (1 << 2) | (1 << 3),
            b_parm: CurveParams { sigma: 0.6, ceil: 1.0, floor: 0.0 },
            s_parm: CurveParams { sigma: 0.6, ceil: 1.0, floor: 0.0 },
            density: 0.3,
        }
    }

    #[test]
    fn always_failing_rng_keeps_grid_dead() {
        let r = run_trial(&opts(), &mut || 0.99);
        assert_eq!(r.status, Verdict::Dead);
        assert_eq!(r.mean, 0.0);
    }

    #[test]
    fn always_passing_rng_fills_and_stays_full() {
        let r = run_trial(&opts(), &mut || 0.0);
        assert_eq!(r.status, Verdict::Full);
        assert_eq!(r.mean, 1.0);
    }
}
