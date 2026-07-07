//! Gaussian-bump probability curves over neighbor counts (port of life-stats.js curve/bumpMax).

#[derive(Clone, Copy, Debug)]
pub struct CurveParams { pub sigma: f64, pub floor: f64, pub ceil: f64 }

/// Max Gaussian bump over the digit-set members (bitmask, bit n = count n is a member).
pub fn bump_max(mask: u16, n: u8, sigma: f64) -> f64 {
    let mut p = 0.0f64;
    for c in 0..=8u8 {
        if mask & (1 << c) == 0 { continue; }
        let d = n as f64 - c as f64;
        p = p.max((-(d * d) / (2.0 * sigma * sigma)).exp());
    }
    p
}

/// Probability per neighbor count 0..=8: floor + (ceil-floor) * bump.
pub fn curve(mask: u16, prm: CurveParams) -> [f64; 9] {
    let span = (prm.ceil - prm.floor).max(0.0);
    std::array::from_fn(|n| prm.floor + span * bump_max(mask, n as u8, prm.sigma))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_peaks_at_member_falls_off_with_distance() {
        let m = 1u16 << 3; // {3}
        let p3 = bump_max(m, 3, 0.6);
        let p2 = bump_max(m, 2, 0.6);
        let p0 = bump_max(m, 0, 0.6);
        assert_eq!(p3, 1.0);
        assert!(p2 < p3 && p0 < p2);
    }

    #[test]
    fn curve_clamps_between_floor_and_ceiling() {
        let m = (1u16 << 2) | (1 << 3); // {2,3}
        let out = curve(m, CurveParams { sigma: 0.05, ceil: 0.7, floor: 0.2 });
        assert!((out[2] - 0.7).abs() < 1e-9, "peak hits ceiling");
        assert!((out[0] - 0.2).abs() < 1e-9, "tail hits floor");
    }
}
