//! Bit-parity against the canonical JS core.
//!
//! `life-ecology-core.js` is the reference implementation and the seeded-replay
//! contract is repo policy, so a port is only correct if the same seed produces
//! the same board. The expected rows below were produced by driving
//! `life-ecology-core.js` from Node with `mulberry32(12345)`, every optional
//! mechanic switched on so the whole step is exercised; see the header of
//! `step_ecology` for what each does.

use life_core::{step_ecology, CellState, EcologyParams, EcologyState, Topology, Wrap};

const COLS: u32 = 24;
const ROWS: u32 = 24;
const N: usize = (COLS * ROWS) as usize;

/// The generator the page and the JS tests both use. f64 throughout: JS has no
/// f32, so rounding here would move every `rng() < p` comparison off the reference.
fn mulberry32(mut a: u32) -> impl FnMut() -> f64 {
    move || {
        a = a.wrapping_add(0x6D2B79F5);
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
}

// gen, red, grey, marten, infected, round(total marten energy * 1000)
const EXPECTED: [(u64, u32, u32, u32, u32, i64); 10] = [
    (19, 115, 299, 55, 262, 425995),
    (39, 12, 281, 109, 229, 624706),
    (59, 0, 113, 168, 41, 829544),
    (79, 0, 142, 86, 0, 442964),
    (99, 0, 213, 159, 0, 792977),
    (119, 0, 169, 110, 0, 501194),
    (139, 0, 223, 133, 0, 742697),
    (159, 0, 161, 152, 0, 829893),
    (179, 0, 88, 123, 0, 572299),
    (199, 0, 308, 64, 0, 453371),
];

#[test]
fn matches_the_js_core_step_for_step() {
    let mut rng = mulberry32(12345);
    let mut st = EcologyState::new(COLS, ROWS);

    // seed the board off the same stream, exactly as the page does
    for i in 0..N {
        let u = rng();
        if u < 0.20 {
            st.grid[i] = CellState::Red as u8;
        } else if u < 0.35 {
            st.grid[i] = CellState::Grey as u8;
        } else if u < 0.40 {
            st.grid[i] = CellState::Marten as u8;
            st.energy[i] = 5.0;
        }
    }
    let mut env = vec![0.0f32; N];
    for (i, e) in env.iter_mut().enumerate() {
        *e = ((i as f64 * 0.1).sin() * 0.8) as f32;
    }
    st.env = Some(env);
    for i in 0..N {
        if st.grid[i] == CellState::Grey as u8 && i % 7 == 0 {
            st.infected[i] = 1;
        }
    }

    let mut p = EcologyParams {
        beta_red: 0.10,
        beta_grey: 0.14,
        sigma: 0.92,
        evade_red: 0.3,
        evade_grey: 0.05,
        forage: 0.4,
        mate: true,
        mortality: 0.02,
        pox_beta: 0.15,
        pox_lethal: 0.25,
        terrain: 0.6,
        ..Default::default()
    };
    let topo = Topology { x: Wrap::Straight, y: Wrap::Straight };

    let mut expected = EXPECTED.iter();
    for gen in 0..200u64 {
        p.gen = gen;
        st = step_ecology(&st, topo, &p, &mut rng);
        if gen % 20 != 19 {
            continue;
        }
        let (mut red, mut grey, mut mart, mut inf) = (0, 0, 0, 0);
        let mut e = 0.0f64;
        for i in 0..N {
            match CellState::from_u8(st.grid[i]) {
                CellState::Red => red += 1,
                CellState::Grey => grey += 1,
                CellState::Marten => {
                    mart += 1;
                    e += st.energy[i] as f64;
                }
                CellState::Empty => {}
            }
            if st.infected[i] != 0 {
                inf += 1;
            }
        }
        let got = (gen, red, grey, mart, inf, (e * 1000.0).round() as i64);
        assert_eq!(got, *expected.next().unwrap(), "diverged from the JS core");
    }
}
