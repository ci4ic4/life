//! Windowed population statistics + verdict classification (port of life-stats.js).

pub struct RingStats {
    buf: Vec<f64>,
    count: usize,
    pos: usize,
    sum: f64,
    sum_sq: f64,
}

impl RingStats {
    pub fn new(window: usize) -> Self {
        RingStats { buf: vec![0.0; window], count: 0, pos: 0, sum: 0.0, sum_sq: 0.0 }
    }
    pub fn reset(&mut self) {
        self.buf.fill(0.0);
        self.count = 0;
        self.pos = 0;
        self.sum = 0.0;
        self.sum_sq = 0.0;
    }
    pub fn push(&mut self, v: f64) {
        if self.count == self.buf.len() {
            let old = self.buf[self.pos];
            self.sum -= old;
            self.sum_sq -= old * old;
        } else {
            self.count += 1;
        }
        self.buf[self.pos] = v;
        self.sum += v;
        self.sum_sq += v * v;
        self.pos = (self.pos + 1) % self.buf.len();
    }
    pub fn full(&self) -> bool { self.count == self.buf.len() }
    pub fn mean(&self) -> f64 {
        if self.count == 0 { 0.0 } else { self.sum / self.count as f64 }
    }
    pub fn std_dev(&self) -> f64 {
        if self.count == 0 { return 0.0; }
        let m = self.sum / self.count as f64;
        (self.sum_sq / self.count as f64 - m * m).max(0.0).sqrt()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Thresholds { pub window: usize, pub dead: f64, pub full: f64, pub stable: f64 }

impl Thresholds {
    pub const DEFAULT: Thresholds = Thresholds { window: 50, dead: 0.02, full: 0.95, stable: 0.03 };
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict { Dead, Full, Stable, Chaotic }

pub fn classify(mean: f64, std_dev: f64, t: &Thresholds) -> Verdict {
    if mean < t.dead { return Verdict::Dead; }
    if mean > t.full { return Verdict::Full; }
    if std_dev < t.stable { return Verdict::Stable; }
    Verdict::Chaotic
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_stats_windowed_mean_and_overwrite() {
        let mut rs = RingStats::new(3);
        for v in [1.0, 1.0, 1.0] { rs.push(v); }
        assert!(rs.full());
        assert_eq!(rs.mean(), 1.0);
        assert_eq!(rs.std_dev(), 0.0);
        rs.push(0.0); // overwrites oldest -> window [1,1,0]
        assert!((rs.mean() - 2.0 / 3.0).abs() < 1e-9);
        assert!(rs.std_dev() > 0.0);
    }

    #[test]
    fn classify_labels_by_thresholds() {
        let t = Thresholds::DEFAULT;
        assert_eq!(classify(0.01, 0.0, &t), Verdict::Dead);
        assert_eq!(classify(0.97, 0.0, &t), Verdict::Full);
        assert_eq!(classify(0.4, 0.01, &t), Verdict::Stable);
        assert_eq!(classify(0.4, 0.2, &t), Verdict::Chaotic);
    }
}
