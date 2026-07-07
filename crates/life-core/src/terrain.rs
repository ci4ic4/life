//! Static env field in [-1,1]: sum of signed Gaussian bumps (basins + ridges),
//! torus-wrapped, normalised. Port of life-evolve.html generateTerrain.

/// Torus-wrapped squared distance between two cells (seamless bumps).
fn wrap_dist2(c0: f32, r0: f32, c1: f32, r1: f32, cols: f32, rows: f32) -> f32 {
    let mut dc = (c0 - c1).abs();
    if dc > cols / 2.0 { dc = cols - dc; }
    let mut dr = (r0 - r1).abs();
    if dr > rows / 2.0 { dr = rows - dr; }
    dc * dc + dr * dr
}

/// Terrain-like relief: 9 Gaussian bumps of mixed sign, normalised to ~[-1,1].
pub fn generate_terrain(w: u32, h: u32, rng: &mut impl FnMut() -> f64) -> Vec<f32> {
    let (cols, rows) = (w as f32, h as f32);
    let sigma = cols.min(rows) / 4.0;
    let two_sig2 = 2.0 * sigma * sigma;
    let mut env = vec![0.0f32; (w * h) as usize];
    for _ in 0..9 {
        let bc = rng() as f32 * cols;
        let br = rng() as f32 * rows;
        let amp = (rng() as f32) * 2.0 - 1.0; // signed amplitude
        for r in 0..h {
            for c in 0..w {
                env[(r * w + c) as usize] +=
                    amp * (-wrap_dist2(c as f32, r as f32, bc, br, cols, rows) / two_sig2).exp();
            }
        }
    }
    let max = env.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if max > 0.0 {
        for v in env.iter_mut() { *v /= max; }
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_is_normalised_and_nonflat() {
        let mut s = 0xABCD_u64;
        let mut rng = move || {
            s ^= s << 13; s ^= s >> 7; s ^= s << 17;
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        let env = generate_terrain(64, 32, &mut rng);
        let max = env.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((max - 1.0).abs() < 1e-6, "normalised to unit max");
        assert!(env.iter().any(|&v| v > 0.1) && env.iter().any(|&v| v < -0.1), "mixed-sign relief");
    }
}
