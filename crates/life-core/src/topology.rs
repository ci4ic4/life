#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Wrap {
    #[default]
    Straight,
    None,
    Flip,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Topology { pub x: Wrap, pub y: Wrap }

/// Resolve a possibly out-of-bounds coordinate against the grid topology.
/// Returns `None` when a coordinate falls off a hard (`Wrap::None`) edge.
/// `Flip` wraps its own axis and mirrors the *other* axis (Klein seam).
pub fn resolve(mut x: i32, mut y: i32, w: u32, h: u32, t: Topology) -> Option<(u32, u32)> {
    let (wi, hi) = (w as i32, h as i32);
    // Resolve one axis; return (wrapped coord, whether the *other* axis mirrors).
    fn axis(v: i32, n: i32, wrap: Wrap) -> Option<(i32, bool)> {
        if v >= 0 && v < n {
            return Some((v, false));
        }
        match wrap {
            Wrap::None => None,
            Wrap::Straight => Some((v.rem_euclid(n), false)),
            // wrapped, and signal the perpendicular axis to mirror once per crossing
            Wrap::Flip => {
                let crossings = if v < 0 { (-v - 1) / n + 1 } else { v / n };
                Some((v.rem_euclid(n), crossings % 2 == 1))
            }
        }
    }
    let (rx, mirror_y) = axis(x, wi, t.x)?;
    let (ry, mirror_x) = axis(y, hi, t.y)?;
    x = rx;
    y = ry;
    if mirror_y { y = hi - 1 - y; }
    if mirror_x { x = wi - 1 - x; }
    Some((x as u32, y as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    const TORUS: Topology = Topology { x: Wrap::Straight, y: Wrap::Straight };

    #[test]
    fn in_bounds_is_identity() {
        assert_eq!(resolve(3, 4, 10, 10, TORUS), Some((3, 4)));
    }
    #[test]
    fn straight_wraps_both_ways() {
        assert_eq!(resolve(-1, 0, 10, 10, TORUS), Some((9, 0)));
        assert_eq!(resolve(10, 0, 10, 10, TORUS), Some((0, 0)));
        assert_eq!(resolve(0, -1, 10, 10, TORUS), Some((0, 9)));
    }
    #[test]
    fn none_falls_off_edge() {
        let t = Topology { x: Wrap::None, y: Wrap::None };
        assert_eq!(resolve(-1, 5, 10, 10, t), None);
        assert_eq!(resolve(10, 5, 10, 10, t), None);
        assert_eq!(resolve(5, 5, 10, 10, t), Some((5, 5)));
    }
    #[test]
    fn flip_x_mirrors_y() {
        // crossing the x seam wraps x and mirrors the y coordinate
        let t = Topology { x: Wrap::Flip, y: Wrap::Straight };
        assert_eq!(resolve(-1, 0, 10, 10, t), Some((9, 9)));
        assert_eq!(resolve(-1, 2, 10, 10, t), Some((9, 7)));
    }
}
