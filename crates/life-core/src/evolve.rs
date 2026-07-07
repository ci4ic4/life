//! Darwinian variant: two clans, heritable τ (foe tolerance) and θ (terrain
//! tolerance) genes under spatial selection. Port of life-evolve.html's
//! computeNext — read the design specs before touching the selection math:
//! birth deliberately ignores τ (a newborn has no gene yet), and the terrain
//! effect on birth is shielded by the *parents'* mean θ.
//!
//! All arithmetic is f32 in CPU-kernel iteration order so the GPU pipeline
//! can assert exact parity at σ_mut = 0.

use crate::topology::{resolve, Topology};

#[derive(Clone, Debug)]
pub struct Kernel {
    pub cells: Vec<(i8, i8, f32)>, // (dc, dr, weight)
    pub max: f32,
}

impl Kernel {
    /// Classic Moore neighborhood, all weights 1 (max support 8).
    pub fn moore() -> Kernel {
        let mut cells = Vec::new();
        for dr in -1i8..=1 {
            for dc in -1i8..=1 {
                if dc != 0 || dr != 0 {
                    cells.push((dc, dr, 1.0));
                }
            }
        }
        Kernel { cells, max: 8.0 }
    }

    /// Weighted 5×5 (sums to 40); sustains dense contested borders where
    /// tolerance actually evolves. Rules are ranges like B14-18/S12-24.
    pub fn weighted5x5() -> Kernel {
        const MAT: [[f32; 5]; 5] = [
            [1.0, 1.0, 2.0, 1.0, 1.0],
            [1.0, 2.0, 3.0, 2.0, 1.0],
            [2.0, 3.0, 0.0, 3.0, 2.0],
            [1.0, 2.0, 3.0, 2.0, 1.0],
            [1.0, 1.0, 2.0, 1.0, 1.0],
        ];
        let mut cells = Vec::new();
        for (r, row) in MAT.iter().enumerate() {
            for (c, &w) in row.iter().enumerate() {
                if w != 0.0 {
                    cells.push((c as i8 - 2, r as i8 - 2, w));
                }
            }
        }
        Kernel { cells, max: 40.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum EvolveRule {
    /// Digit-set B/S over integer counts; survival support is fractional and
    /// rounded JS-style (floor(s + 0.5)).
    DigitSet { birth: u16, survive: u16 },
    /// Inclusive ranges for weighted kernels.
    Ranged { b_lo: f32, b_hi: f32, s_lo: f32, s_hi: f32 },
}

impl EvolveRule {
    fn birth_ok(&self, n: f32) -> bool {
        match *self {
            // birth support is an integer weighted difference in Moore mode
            EvolveRule::DigitSet { birth, .. } => {
                let k = js_round(n);
                (0.0..=8.0).contains(&k) && birth & (1 << k as u16) != 0
            }
            EvolveRule::Ranged { b_lo, b_hi, .. } => n >= b_lo && n <= b_hi,
        }
    }
    fn survive_ok(&self, s: f32) -> bool {
        match *self {
            EvolveRule::DigitSet { survive, .. } => {
                let k = js_round(s);
                (0.0..=8.0).contains(&k) && survive & (1 << k as u16) != 0
            }
            EvolveRule::Ranged { s_lo, s_hi, .. } => s >= s_lo && s <= s_hi,
        }
    }
}

/// JS Math.round semantics: half rounds toward +infinity (floor(x + 0.5)).
/// f32::round (half away from zero) differs on negative halves — don't swap.
#[inline]
fn js_round(x: f32) -> f32 {
    (x + 0.5).floor()
}

/// Parse a rule string for the given kernel family: digit-set ("B3/S23") for
/// Moore, inclusive ranges ("B14-18/S12-24", single value = degenerate range)
/// for weighted kernels.
pub fn parse_evolve_rule(s: &str, ranged: bool) -> Option<EvolveRule> {
    if !ranged {
        let bs = crate::rule::parse_bs(s).ok()?;
        return Some(EvolveRule::DigitSet { birth: bs.birth, survive: bs.survive });
    }
    let s = s.trim().to_uppercase();
    let (b, sv) = s.split_once('/')?;
    fn range(part: &str, tag: char) -> Option<(f32, f32)> {
        let rest = part.trim().strip_prefix(tag)?;
        let (lo, hi) = match rest.split_once('-') {
            Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
            None => {
                let v: f32 = rest.trim().parse().ok()?;
                (v, v)
            }
        };
        (lo <= hi && lo >= 0.0).then_some((lo, hi))
    }
    let (b_lo, b_hi) = range(b, 'B')?;
    let (s_lo, s_hi) = range(sv, 'S')?;
    Some(EvolveRule::Ranged { b_lo, b_hi, s_lo, s_hi })
}

#[derive(Clone, Copy, Debug)]
pub struct EvolveParams {
    pub rule: EvolveRule,
    pub env_weight: f32,
    pub mut_sigma: f32,
}

#[derive(Clone, Debug)]
pub struct EvolveState {
    pub w: u32,
    pub h: u32,
    pub grid: Vec<u8>,   // 0 dead, 1 warm clan, 2 cool clan
    pub tau: Vec<f32>,   // meaningless where dead
    pub theta: Vec<f32>, // meaningless where dead
}

impl EvolveState {
    pub fn new(w: u32, h: u32) -> Self {
        let n = (w * h) as usize;
        EvolveState { w, h, grid: vec![0; n], tau: vec![0.0; n], theta: vec![0.0; n] }
    }
    pub fn idx(&self, x: u32, y: u32) -> usize { (y * self.w + x) as usize }
}

/// Gaussian noise via Box–Muller, identical form to the JS gauss().
pub fn gauss(sigma: f32, rng: &mut impl FnMut() -> f64) -> f32 {
    if sigma <= 0.0 {
        return 0.0;
    }
    let u = 1.0 - rng(); // (0, 1] — safe for ln
    let v = rng();
    sigma * ((-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()) as f32
}

fn clamp01(x: f32) -> f32 { x.clamp(0.0, 1.0) }

/// One generation. See module docs for the deliberate asymmetries.
pub fn step_evolve(
    st: &EvolveState,
    env: &[f32],
    kernel: &Kernel,
    p: &EvolveParams,
    t: Topology,
    rng: &mut impl FnMut() -> f64,
) -> EvolveState {
    let mut out = EvolveState::new(st.w, st.h);
    let (w, h) = (st.w, st.h);
    for r in 0..h {
        for c in 0..w {
            // weighted per-clan support + weighted gene sums (closer parents
            // contribute more weight, so they pass proportionally more gene)
            let (mut a0, mut a1) = (0.0f32, 0.0f32);
            let (mut t_sum0, mut t_sum1) = (0.0f32, 0.0f32);
            let (mut th_sum0, mut th_sum1) = (0.0f32, 0.0f32);
            for &(dc, dr, kw) in &kernel.cells {
                let Some((nx, ny)) = resolve(c as i32 + dc as i32, r as i32 + dr as i32, w, h, t) else { continue };
                let i = (ny * w + nx) as usize;
                match st.grid[i] {
                    1 => { a0 += kw; t_sum0 += kw * st.tau[i]; th_sum0 += kw * st.theta[i]; }
                    2 => { a1 += kw; t_sum1 += kw * st.tau[i]; th_sum1 += kw * st.theta[i]; }
                    _ => {}
                }
            }
            let here = (r * w + c) as usize;
            let e_full = p.env_weight * env[here]; // full terrain effect (birth, θ-blind)
            let alive = st.grid[here];
            if alive != 0 {
                let (allies, foes) = if alive == 1 { (a0, a1) } else { (a1, a0) };
                // τ shields from foes; θ scales the terrain effect
                let s = allies - (1.0 - st.tau[here]) * foes + e_full * (1.0 - st.theta[here]);
                if p.rule.survive_ok(s) {
                    out.grid[here] = alive;
                    out.tau[here] = st.tau[here];
                    out.theta[here] = st.theta[here];
                }
            } else {
                // birth: rivals count full (no τ); terrain penalty shielded by
                // the parent clan's mean θ. Both clans qualifying -> contested,
                // stays dead.
                let eb0 = if a0 > 0.0 { e_full * (1.0 - th_sum0 / a0) } else { e_full };
                let eb1 = if a1 > 0.0 { e_full * (1.0 - th_sum1 / a1) } else { e_full };
                let b0 = p.rule.birth_ok(a0 - a1 + eb0) && a0 > 0.0;
                let b1 = p.rule.birth_ok(a1 - a0 + eb1) && a1 > 0.0;
                if b0 != b1 {
                    let (clan, mt, mth) = if b0 {
                        (1u8, t_sum0 / a0, th_sum0 / a0)
                    } else {
                        (2u8, t_sum1 / a1, th_sum1 / a1)
                    };
                    out.grid[here] = clan;
                    out.tau[here] = clamp01(mt + gauss(p.mut_sigma, rng));
                    out.theta[here] = clamp01(mth + gauss(p.mut_sigma, rng));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::Wrap;

    const TORUS: Topology = Topology { x: Wrap::Straight, y: Wrap::Straight };
    const B3S23: EvolveRule = EvolveRule::DigitSet { birth: 1 << 3, survive: (1 << 2) | (1 << 3) };

    fn params(rule: EvolveRule) -> EvolveParams {
        EvolveParams { rule, env_weight: 4.0, mut_sigma: 0.0 }
    }
    fn setup(w: u32, h: u32) -> (EvolveState, Vec<f32>) {
        (EvolveState::new(w, h), vec![0.0; (w * h) as usize])
    }
    fn no_rng() -> impl FnMut() -> f64 { || 0.5 }

    #[test]
    fn tau0_rival_kills_blinker_centre() {
        let (mut st, env) = setup(30, 30);
        // warm vertical blinker + one cool foe by the centre
        for y in 10..=12 { let i = st.idx(10, y); st.grid[i] = 1; }
        let i = st.idx(11, 11); st.grid[i] = 2;
        // centre (10,11): allies=2, foes=1, τ=0 -> s=1, not in S23 -> dies
        let next = step_evolve(&st, &env, &Kernel::moore(), &params(B3S23), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(10, 11)], 0);
    }

    #[test]
    fn tau1_shields_same_foe() {
        let (mut st, env) = setup(30, 30);
        for y in 10..=12 { let i = st.idx(10, y); st.grid[i] = 1; st.tau[i] = 1.0; }
        let i = st.idx(11, 11); st.grid[i] = 2;
        // τ=1 -> s=2 -> survives
        let next = step_evolve(&st, &env, &Kernel::moore(), &params(B3S23), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(10, 11)], 1);
    }

    #[test]
    fn mutation_output_clamps_to_unit_interval() {
        let mut s = 0x5EED_u64;
        let mut rng = move || {
            s ^= s << 13; s ^= s >> 7; s ^= s << 17;
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        for _ in 0..5000 {
            let v = clamp01(0.5 + gauss(5.0, &mut rng));
            assert!((0.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn newborn_tau_is_parents_mean() {
        let (mut st, env) = setup(40, 40);
        // 3 warm parents around (21,21)
        for (x, y, tv) in [(20, 20, 0.2f32), (22, 20, 0.4), (21, 22, 0.9)] {
            let i = st.idx(x, y); st.grid[i] = 1; st.tau[i] = tv;
        }
        let next = step_evolve(&st, &env, &Kernel::moore(), &params(B3S23), TORUS, &mut no_rng());
        let i = st.idx(21, 21);
        assert_eq!(next.grid[i], 1, "3 warm parents -> warm newborn");
        assert!((next.tau[i] - (0.2 + 0.4 + 0.9) / 3.0).abs() < 1e-6, "newborn τ = parents mean");
    }

    #[test]
    fn weighted_ranged_tau_shields_too() {
        // Coral-ish B6-14/S4-8 on the weighted 5x5.
        let rule = EvolveRule::Ranged { b_lo: 6.0, b_hi: 14.0, s_lo: 4.0, s_hi: 8.0 };
        let (mut st, env) = setup(60, 60);
        // centre warm, two orthogonal warm allies (w3+w3=6), one orthogonal cool foe (w3)
        let i = st.idx(30, 30); st.grid[i] = 1;
        let i = st.idx(29, 30); st.grid[i] = 1;
        let i = st.idx(30, 29); st.grid[i] = 1;
        let i = st.idx(31, 30); st.grid[i] = 2;
        let k = Kernel::weighted5x5();
        // τ=0: s = 6 − 3 = 3 < 4 → dies
        let next = step_evolve(&st, &env, &k, &params(rule), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(30, 30)], 0);
        // τ=1: s = 6 → in [4,8] → survives
        let i = st.idx(30, 30); st.tau[i] = 1.0;
        let next = step_evolve(&st, &env, &k, &params(rule), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(30, 30)], 1);
    }

    #[test]
    fn environment_bias_rescues_and_kills() {
        let k = Kernel::moore();
        // 1 ally (s=1, dies under S23) — fertile ground rescues
        let (mut st, mut env) = setup(60, 40);
        let i = st.idx(40, 20); st.grid[i] = 1;
        let i = st.idx(41, 20); st.grid[i] = 1;
        let next = step_evolve(&st, &env, &k, &params(B3S23), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(40, 20)], 0, "env=0: s=1 dies under S23");
        env[st.idx(40, 20)] = 0.5; // +4*0.5 = +2 -> s=3, in S23
        let next = step_evolve(&st, &env, &k, &params(B3S23), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(40, 20)], 1, "fertile terrain rescues");
        // hostile: comfortable cell (2 allies, s=2) pushed out of band
        let (mut st, mut env) = setup(60, 40);
        for x in [39, 40, 41] { let i = st.idx(x, 20); st.grid[i] = 1; }
        env[st.idx(40, 20)] = -0.5; // -2 -> s=0 -> dies
        let next = step_evolve(&st, &env, &k, &params(B3S23), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(40, 20)], 0, "hostile terrain kills");
    }

    #[test]
    fn theta_shields_hostile_and_forgoes_fertile() {
        let k = Kernel::moore();
        // θ shields hostile: 2-ally cell on env=-0.5 (eFull=-2)
        let (mut st, mut env) = setup(60, 40);
        for x in [39, 40, 41] { let i = st.idx(x, 20); st.grid[i] = 1; }
        env[st.idx(40, 20)] = -0.5;
        let next = step_evolve(&st, &env, &k, &params(B3S23), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(40, 20)], 0, "θ=0 specialist dies on hostile ground");
        let i = st.idx(40, 20); st.theta[i] = 1.0; // s = 2 + (−2)·0 = 2 → survives
        let next = step_evolve(&st, &env, &k, &params(B3S23), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(40, 20)], 1, "θ=1 generalist survives hostile ground");
        // θ forgoes fertile: 1-ally cell kept alive only by env=0.5 (eFull=+2)
        let (mut st, mut env) = setup(60, 40);
        let i = st.idx(40, 20); st.grid[i] = 1;
        let i = st.idx(41, 20); st.grid[i] = 1;
        env[st.idx(40, 20)] = 0.5;
        let next = step_evolve(&st, &env, &k, &params(B3S23), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(40, 20)], 1, "θ=0 specialist rides the fertile bonus");
        let i = st.idx(40, 20); st.theta[i] = 1.0; // s = 1 + 2·0 = 1 → dies
        let next = step_evolve(&st, &env, &k, &params(B3S23), TORUS, &mut no_rng());
        assert_eq!(next.grid[st.idx(40, 20)], 0, "θ=1 generalist forgoes the bonus and dies");
    }

    #[test]
    fn newborn_theta_is_parents_mean() {
        let (mut st, env) = setup(40, 40);
        for (x, y, thv) in [(20, 20, 0.1f32), (22, 20, 0.5), (21, 22, 0.9)] {
            let i = st.idx(x, y); st.grid[i] = 1; st.theta[i] = thv;
        }
        let next = step_evolve(&st, &env, &Kernel::moore(), &params(B3S23), TORUS, &mut no_rng());
        let i = st.idx(21, 21);
        assert_eq!(next.grid[i], 1);
        assert!((next.theta[i] - (0.1 + 0.5 + 0.9) / 3.0).abs() < 1e-6, "newborn θ = parents mean");
    }
}
