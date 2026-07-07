use crate::grid::Grid;
use std::collections::{HashMap, VecDeque};

/// Ring history of recent grid states; reports the period if the current
/// state has been seen before within the window (1 = still life).
pub struct CycleDetector {
    seen: HashMap<Vec<u8>, u64>,
    generation: u64,
    window: usize,
    order: VecDeque<Vec<u8>>,
}

impl CycleDetector {
    pub fn new(window: usize) -> Self {
        CycleDetector { seen: HashMap::new(), generation: 0, window, order: VecDeque::new() }
    }

    /// Feed the current grid; returns Some(period) if a cycle is detected this generation.
    pub fn observe(&mut self, g: &Grid) -> Option<u64> {
        let key = g.cells.clone();
        let period = self.seen.get(&key).map(|&first| self.generation - first);
        if !self.seen.contains_key(&key) {
            self.seen.insert(key.clone(), self.generation);
            self.order.push_back(key);
            if self.order.len() > self.window {
                if let Some(old) = self.order.pop_front() {
                    self.seen.remove(&old);
                }
            }
        }
        self.generation += 1;
        period
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_bs, step_deterministic, Topology, Wrap};

    #[test]
    fn detects_blinker_period_2() {
        let bs = parse_bs("B3/S23").unwrap();
        let t = Topology { x: Wrap::Straight, y: Wrap::Straight };
        let mut g = Grid::new(5, 5);
        g.set(2, 1, 1); g.set(2, 2, 1); g.set(2, 3, 1);
        let mut det = CycleDetector::new(16);
        let mut period = None;
        for _ in 0..4 {
            if let Some(p) = det.observe(&g) { period = Some(p); break; }
            g = step_deterministic(&g, &bs, t);
        }
        assert_eq!(period, Some(2));
    }

    #[test]
    fn detects_still_life_period_1() {
        // 2x2 block is stable under B3/S23
        let bs = parse_bs("B3/S23").unwrap();
        let t = Topology { x: Wrap::Straight, y: Wrap::Straight };
        let mut g = Grid::new(4, 4);
        g.set(1, 1, 1); g.set(2, 1, 1); g.set(1, 2, 1); g.set(2, 2, 1);
        let mut det = CycleDetector::new(16);
        det.observe(&g);
        let g2 = step_deterministic(&g, &bs, t);
        assert_eq!(det.observe(&g2), Some(1));
    }
}
