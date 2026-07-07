use crate::rule::BS;
use crate::topology::{resolve, Topology};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Grid { pub w: u32, pub h: u32, pub cells: Vec<u8> }

impl Grid {
    pub fn new(w: u32, h: u32) -> Self { Grid { w, h, cells: vec![0; (w * h) as usize] } }
    pub fn get(&self, x: u32, y: u32) -> u8 { self.cells[(y * self.w + x) as usize] }
    pub fn set(&mut self, x: u32, y: u32, v: u8) { self.cells[(y * self.w + x) as usize] = v; }
}

/// 8-neighborhood live count around (x, y), honoring the topology.
pub fn count_neighbors(g: &Grid, x: u32, y: u32, t: Topology) -> u8 {
    let mut n = 0u8;
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 { continue; }
            if let Some((nx, ny)) = resolve(x as i32 + dx, y as i32 + dy, g.w, g.h, t) {
                n += g.get(nx, ny);
            }
        }
    }
    n
}

/// One generation, double-buffered.
pub fn step_deterministic(g: &Grid, bs: &BS, t: Topology) -> Grid {
    let mut out = Grid::new(g.w, g.h);
    for y in 0..g.h {
        for x in 0..g.w {
            let n = count_neighbors(g, x, y, t);
            let alive = g.get(x, y) == 1;
            let next = if alive { bs.survives(n) } else { bs.born(n) };
            out.set(x, y, next as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::parse_bs;
    use crate::topology::Wrap;
    const TORUS: Topology = Topology { x: Wrap::Straight, y: Wrap::Straight };

    fn blinker() -> Grid {
        // vertical 3-cell blinker centred in a 5x5 torus
        let mut g = Grid::new(5, 5);
        g.set(2, 1, 1); g.set(2, 2, 1); g.set(2, 3, 1);
        g
    }

    #[test]
    fn counts_eight_neighborhood() {
        let g = blinker();
        assert_eq!(count_neighbors(&g, 2, 2, TORUS), 2); // above+below
        assert_eq!(count_neighbors(&g, 1, 2, TORUS), 3); // sees all three
    }
    #[test]
    fn blinker_oscillates() {
        let bs = parse_bs("B3/S23").unwrap();
        let g1 = step_deterministic(&blinker(), &bs, TORUS);
        // vertical blinker -> horizontal blinker
        assert_eq!(g1.get(1, 2), 1);
        assert_eq!(g1.get(2, 2), 1);
        assert_eq!(g1.get(3, 2), 1);
        assert_eq!(g1.get(2, 1), 0);
        // and back
        let g2 = step_deterministic(&g1, &bs, TORUS);
        assert_eq!(g2, blinker());
    }
    #[test]
    fn hard_edge_kills_wrap_neighbors() {
        let t = Topology { x: Wrap::None, y: Wrap::None };
        let mut g = Grid::new(3, 3);
        g.set(0, 0, 1);
        // corner cell has only 3 in-bounds neighbor slots, all dead here
        assert_eq!(count_neighbors(&g, 0, 0, t), 0);
    }
}
