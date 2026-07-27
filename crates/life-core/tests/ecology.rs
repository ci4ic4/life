use life_core::{
    resolve_idx, step_ecology, CellState, EcologyParams, EcologyState, Topology, Wrap,
};

fn mulberry32(mut a: u32) -> impl FnMut() -> f64 {
    move || {
        a = a.wrapping_add(0x6D2B79F5);
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        let h = t ^ (t >> 14);
        h as f64 / 4294967296.0
    }
}

const NONE_TOPO: Topology = Topology {
    x: Wrap::None,
    y: Wrap::None,
};

const TORUS_TOPO: Topology = Topology {
    x: Wrap::Straight,
    y: Wrap::Straight,
};

const PRED: EcologyParams = EcologyParams {
    beta_red: 0.0,
    beta_grey: 0.0,
    sigma: 1.0,
    delta: 1.0,
    g: 3.0,
    e_breed: 100.0,
    e0: 5.0,
    breed_cost: 5.0,
    e_cap: 100.0,
    mu: 0.0,
    evade_red: 0.0,
    evade_grey: 0.0,
    forage: 0.0,
    mate: false,
    mortality: 0.0,
    pox_beta: 0.0,
    pox_lethal: 0.0,
    terrain: 0.0,
    gen: 1,
};

const POX: EcologyParams = EcologyParams {
    beta_red: 0.0,
    beta_grey: 0.0,
    sigma: 1.0,
    delta: 1.0,
    g: 3.0,
    e_breed: 100.0,
    e0: 5.0,
    breed_cost: 5.0,
    e_cap: 100.0,
    mu: 0.0,
    evade_red: 0.0,
    evade_grey: 0.0,
    forage: 0.0,
    mate: false,
    mortality: 0.0,
    pox_beta: 0.0,
    pox_lethal: 0.0,
    terrain: 0.0,
    gen: 1,
};

#[test]
fn empty_cell_with_k_red_neighbours_colonises_red() {
    let cols = 3;
    let rows = 3;
    let params = EcologyParams {
        beta_red: 0.3,
        beta_grey: 0.0,
        sigma: 1.0,
        ..Default::default()
    };
    let k = 2.0f64;
    let beta = 0.3f64;
    let expected = 1.0 - (1.0 - beta).powf(k);
    let mut reds = 0;
    let trials = 20000;

    for t in 0..trials {
        let mut state = EcologyState::new(cols, rows);
        state.set(0, 1, CellState::Red);
        state.set(2, 1, CellState::Red);
        let next = step_ecology(&state, NONE_TOPO, &params, &mut mulberry32((t + 1) as u32));
        if next.get(1, 1) == CellState::Red {
            reds += 1;
        }
    }
    let rate = reds as f64 / trials as f64;
    assert!((rate - expected).abs() < 0.02, "red rate {} vs {}", rate, expected);
}

#[test]
fn grey_outcolonises_red_when_beta_grey_gt_beta_red() {
    let cols = 3;
    let rows = 3;
    let params = EcologyParams {
        beta_red: 0.10,
        beta_grey: 0.30,
        sigma: 1.0,
        ..Default::default()
    };
    let mut reds = 0;
    let mut greys = 0;
    let trials = 20000;

    for t in 0..trials {
        let mut state = EcologyState::new(cols, rows);
        state.set(0, 1, CellState::Red);
        state.set(2, 1, CellState::Grey);
        let next = step_ecology(&state, NONE_TOPO, &params, &mut mulberry32((t + 1) as u32));
        match next.get(1, 1) {
            CellState::Red => reds += 1,
            CellState::Grey => greys += 1,
            _ => {}
        }
    }
    assert!(greys > (reds as f32 * 1.5) as i32, "grey {} should dominate red {}", greys, reds);
}

#[test]
fn prey_dies_at_rate_1_minus_sigma_with_no_neighbours() {
    let cols = 3;
    let rows = 3;
    let params = EcologyParams {
        beta_red: 0.0,
        beta_grey: 0.0,
        sigma: 0.8,
        ..Default::default()
    };
    let mut survived = 0;
    let trials = 20000;

    for t in 0..trials {
        let mut state = EcologyState::new(cols, rows);
        state.set(1, 1, CellState::Red);
        let next = step_ecology(&state, NONE_TOPO, &params, &mut mulberry32((t + 1) as u32));
        if next.get(1, 1) == CellState::Red {
            survived += 1;
        }
    }
    let rate = survived as f32 / trials as f32;
    assert!((rate - 0.8).abs() < 0.02, "survival rate {}", rate);
}

#[test]
fn resolve_idx_wraps_straight_and_returns_null_on_none_edge() {
    assert_eq!(resolve_idx(-1, 0, 4, 4, Topology { x: Wrap::Straight, y: Wrap::Straight }), Some(3));
    assert_eq!(resolve_idx(-1, 0, 4, 4, NONE_TOPO), None);
}

#[test]
fn grey_competitively_excludes_red_on_a_torus_over_time() {
    let cols = 60;
    let rows = 30;
    let params = EcologyParams {
        beta_red: 0.10,
        beta_grey: 0.18,
        sigma: 0.92,
        ..Default::default()
    };
    let mut rng = mulberry32(12345);
    let mut state = EcologyState::new(cols, rows);
    for i in 0..(cols * rows) as usize {
        if rng() < 0.30 {
            state.grid[i] = if rng() < 0.5 { CellState::Red as u8 } else { CellState::Grey as u8 };
        }
    }
    let count = |st: &EcologyState| {
        let (mut nr, mut ng) = (0, 0);
        for &v in &st.grid {
            if v == CellState::Red as u8 { nr += 1; }
            else if v == CellState::Grey as u8 { ng += 1; }
        }
        (nr, ng)
    };
    let start = count(&state);
    for _ in 0..400 {
        state = step_ecology(&state, TORUS_TOPO, &params, &mut rng);
    }
    let end = count(&state);
    let start_grey_share = start.1 as f32 / (start.0 + start.1) as f32;
    let end_grey_share = end.1 as f32 / (end.0 + end.1) as f32;
    assert!(end_grey_share > start_grey_share + 0.15, "grey share {:.2} -> {:.2}", start_grey_share, end_grey_share);
    assert!(end.0 < start.0, "red count should fall: {} -> {}", start.0, end.0);
}

#[test]
fn flip_topology_wraps_column_and_mirrors_row() {
    let flip_topo = Topology { x: Wrap::Flip, y: Wrap::Straight };
    assert_eq!(resolve_idx(-1, 0, 4, 4, flip_topo), Some(15));
    assert_eq!(resolve_idx(-1, 0, 4, 4, TORUS_TOPO), Some(3));
}

#[test]
fn empty_cell_with_zero_prey_neighbours_stays_empty() {
    let cols = 3;
    let rows = 3;
    let params = EcologyParams {
        beta_red: 0.5,
        beta_grey: 0.5,
        sigma: 1.0,
        ..Default::default()
    };
    let state = EcologyState::new(cols, rows);
    let next = step_ecology(&state, NONE_TOPO, &params, &mut mulberry32(1));
    assert_eq!(next.get(1, 1), CellState::Empty);
}

#[test]
fn conservation_prey_eaten_equals_martens_that_fed() {
    let cols = 8;
    let rows = 8;
    let n = (cols * rows) as usize;
    let mut state = EcologyState::new(cols, rows);
    let put = |st: &mut EcologyState, c: u32, r: u32, val: CellState, energy: Option<f32>| {
        st.set(c, r, val);
        if let Some(e) = energy {
            st.set_energy(c, r, e);
        }
    };
    put(&mut state, 2, 2, CellState::Marten, Some(5.0));
    put(&mut state, 4, 2, CellState::Marten, Some(5.0));
    put(&mut state, 3, 2, CellState::Red, None);
    put(&mut state, 3, 3, CellState::Grey, None);
    put(&mut state, 2, 3, CellState::Marten, Some(5.0));
    put(&mut state, 6, 6, CellState::Marten, Some(5.0));
    put(&mut state, 6, 5, CellState::Grey, None);

    let before_grid = state.grid.clone();
    let before_energy = state.energy.clone();
    let out = step_ecology(&state, TORUS_TOPO, &PRED, &mut || 0.5);

    let mut prey_gone = 0;
    for i in 0..n {
        let is_p = before_grid[i] == CellState::Red as u8 || before_grid[i] == CellState::Grey as u8;
        if is_p && out.grid[i] != before_grid[i] {
            prey_gone += 1;
        }
    }
    let mut fed = 0;
    for i in 0..n {
        if before_grid[i] == CellState::Marten as u8 && out.energy[i] as f64 > before_energy[i] as f64 - PRED.delta + 1e-6 {
            fed += 1;
        }
    }
    assert_eq!(prey_gone, fed, "prey removed ({}) must equal martens fed ({})", prey_gone, fed);

    let mut gained = 0.0;
    for i in 0..n {
        if before_grid[i] == CellState::Marten as u8 {
            gained += out.energy[i] as f64 - (before_energy[i] as f64 - PRED.delta);
        }
    }
    assert!((gained - prey_gone as f64 * PRED.g).abs() < 1e-5, "gained energy {} vs expected {}", gained, prey_gone as f64 * PRED.g);
}

#[test]
fn missed_hunt_two_martens_one_prey() {
    let cols = 5;
    let rows = 5;
    let mut state = EcologyState::new(cols, rows);
    state.set(1, 2, CellState::Marten); state.set_energy(1, 2, 5.0);
    state.set(3, 2, CellState::Marten); state.set_energy(3, 2, 5.0);
    state.set(2, 2, CellState::Red);

    let out = step_ecology(&state, TORUS_TOPO, &PRED, &mut || 0.5);
    assert_eq!(out.get(2, 2), CellState::Empty, "prey eaten");

    let e1 = out.get_energy(1, 2);
    let e2 = out.get_energy(3, 2);
    let gained = [e1, e2].iter().filter(|&&e| e as f64 > 5.0 - PRED.delta + 1e-6).count();
    let starved = [e1, e2].iter().filter(|&&e| (e as f64 - (5.0 - PRED.delta)).abs() < 1e-5).count();

    assert_eq!(gained, 1, "exactly one marten fed");
    assert_eq!(starved, 1, "exactly one marten missed and only drained delta");
}

#[test]
fn starvation_marten_with_no_prey_dies() {
    let cols = 5;
    let rows = 5;
    let mut state = EcologyState::new(cols, rows);
    state.set(2, 2, CellState::Marten);
    state.set_energy(2, 2, 3.0);

    for _ in 0..3 {
        state = step_ecology(&state, TORUS_TOPO, &PRED, &mut || 0.5);
    }
    assert_eq!(state.get(2, 2), CellState::Empty, "marten starved to death");
}

#[test]
fn breeding_eligible_marten_spawns_and_pays_breed_cost() {
    let cols = 5;
    let rows = 5;
    let mut state = EcologyState::new(cols, rows);
    state.set(2, 2, CellState::Marten);
    state.set_energy(2, 2, 12.0);

    let p = EcologyParams {
        e_breed: 10.0,
        breed_cost: 5.0,
        mu: 1.0,
        e0: 5.0,
        ..PRED
    };
    let out = step_ecology(&state, TORUS_TOPO, &p, &mut || 0.0);
    let mut born = 0;
    for i in 0..(cols * rows) as usize {
        if i != state.idx(2, 2) && out.grid[i] == CellState::Marten as u8 {
            born += 1;
        }
    }
    assert!(born >= 1, "at least one offspring marten");
    let parent_e = out.get_energy(2, 2);
    assert!((parent_e - 6.0).abs() < 1e-5, "parent energy {} should be 6", parent_e);
}

#[test]
fn feasibility_predator_and_prey_persist_and_oscillate() {
    let cols = 60;
    let rows = 30;
    let n = (cols * rows) as usize;
    let mut params = EcologyParams {
        beta_red: 0.14,
        beta_grey: 0.14,
        sigma: 0.92,
        delta: 1.0,
        g: 3.0,
        e_breed: 10.0,
        e0: 5.0,
        breed_cost: 5.0,
        e_cap: 15.0,
        mu: 0.5,
        gen: 0,
        ..Default::default()
    };
    let mut rng = mulberry32(7);
    let mut state = EcologyState::new(cols, rows);
    for i in 0..n {
        if rng() < 0.30 {
            state.grid[i] = CellState::Red as u8;
        }
    }
    for i in 0..n {
        if rng() < 0.02 {
            state.grid[i] = CellState::Marten as u8;
            state.energy[i] = 8.0;
        }
    }
    let prey_of = |st: &EcologyState| st.grid.iter().filter(|&&v| v == CellState::Red as u8 || v == CellState::Grey as u8).count();
    let mart_of = |st: &EcologyState| st.grid.iter().filter(|&&v| v == CellState::Marten as u8).count();

    let mut mart_history = Vec::new();
    for t in 0..400 {
        params.gen = t;
        state = step_ecology(&state, TORUS_TOPO, &params, &mut rng);
        if t >= 200 {
            mart_history.push(mart_of(&state) as f32);
        }
    }
    let mean = mart_history.iter().sum::<f32>() / mart_history.len() as f32;
    let variance = mart_history.iter().map(|&v| (v - mean).powi(2)).sum::<f32>() / mart_history.len() as f32;
    let sd = variance.sqrt();

    assert!(prey_of(&state) > 0, "prey must survive");
    assert!(mean > 0.0, "martens must persist (mean {:.1})", mean);
    assert!(mean < n as f32 * 0.9, "martens must not fill grid (mean {:.1} of {})", mean, n);
    assert!(sd > 1.0, "population should oscillate (stdDev {:.2})", sd);
}

#[test]
fn evade_prey_that_evades_is_not_eaten() {
    let cols = 5;
    let rows = 5;
    let mut state = EcologyState::new(cols, rows);
    state.set(2, 2, CellState::Marten); state.set_energy(2, 2, 5.0);
    state.set(2, 3, CellState::Red);

    let p1 = EcologyParams { evade_red: 1.0, evade_grey: 0.0, ..PRED };
    let out1 = step_ecology(&state, TORUS_TOPO, &p1, &mut || 0.5);
    assert_eq!(out1.get(2, 3), CellState::Red, "evading prey survives");
    assert_eq!(out1.get_energy(2, 2) as f64, 5.0 - PRED.delta, "hunter gained nothing");

    let p2 = EcologyParams { evade_red: 0.0, evade_grey: 0.0, ..PRED };
    let out2 = step_ecology(&state, TORUS_TOPO, &p2, &mut || 0.5);
    assert_eq!(out2.get(2, 3), CellState::Empty, "non-evading prey is eaten");
    assert_eq!(out2.get_energy(2, 2) as f64, 5.0 - PRED.delta + PRED.g, "hunter fed");
}

#[test]
fn evade_is_perspecies() {
    let cols = 5;
    let rows = 5;
    let mut state = EcologyState::new(cols, rows);
    state.set(1, 1, CellState::Marten); state.set_energy(1, 1, 5.0);
    state.set(1, 2, CellState::Red);
    state.set(3, 3, CellState::Marten); state.set_energy(3, 3, 5.0);
    state.set(3, 4, CellState::Grey);

    let p = EcologyParams { evade_red: 1.0, evade_grey: 0.0, ..PRED };
    let out = step_ecology(&state, TORUS_TOPO, &p, &mut || 0.5);
    assert_eq!(out.get(1, 2), CellState::Red, "red evades");
    assert_eq!(out.get(3, 4), CellState::Empty, "grey does not");
}

fn cascade_trial(evade_red: f64, evade_grey: f64) -> ((i32, i32), (i32, i32)) {
    let cols = 60;
    let rows = 30;
    let n = (cols * rows) as usize;
    let intro = 300;
    let end = 800;
    let mut params = EcologyParams {
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
        evade_red,
        evade_grey,
        gen: 0,
        ..Default::default()
    };
    let mut rng = mulberry32(12345);
    let mut seed_rng = mulberry32(999);
    let mut state = EcologyState::new(cols, rows);
    for i in 0..n {
        let r = seed_rng();
        if r < 0.30 {
            state.grid[i] = CellState::Red as u8;
        } else if r < 0.60 {
            state.grid[i] = CellState::Grey as u8;
        }
    }
    let tally = |st: &EcologyState| {
        let (mut red, mut grey) = (0, 0);
        for &v in &st.grid {
            if v == CellState::Red as u8 { red += 1; }
            else if v == CellState::Grey as u8 { grey += 1; }
        }
        (red, grey)
    };
    let mut at_intro = (0, 0);
    for gen in 0..end {
        if gen == intro {
            at_intro = tally(&state);
            for i in 0..n {
                if state.grid[i] != CellState::Marten as u8 && rng() < 0.02 {
                    state.grid[i] = CellState::Marten as u8;
                    state.energy[i] = 8.0;
                }
            }
        }
        params.gen = gen;
        state = step_ecology(&state, TORUS_TOPO, &params, &mut rng);
    }
    (at_intro, tally(&state))
}

#[test]
fn cascade_asymmetric_evasion_lets_martens_reverse_grey_invasion() {
    let (at_intro, final_tally) = cascade_trial(0.70, 0.05);
    assert!(at_intro.0 < (at_intro.1 as f32 * 0.1) as i32, "reds must be near-extinct at intro");
    assert!(final_tally.0 > at_intro.0 * 5, "reds must recover strongly: {} -> {}", at_intro.0, final_tally.0);
    assert!(final_tally.1 < (at_intro.1 as f32 * 0.5) as i32, "greys knocked back: {} -> {}", at_intro.1, final_tally.1);
    assert!(final_tally.0 > final_tally.1, "reds must end dominant (red {} vs grey {})", final_tally.0, final_tally.1);
}

#[test]
fn cascade_control_symmetric_evasion_does_not_save_reds() {
    let (at_intro, final_tally) = cascade_trial(0.05, 0.05);
    assert!(final_tally.1 < (at_intro.1 as f32 * 0.9) as i32, "greys suppressed");
    assert!(final_tally.0 <= at_intro.0, "reds must NOT recover without asymmetry");
    assert!(final_tally.0 < final_tally.1, "reds must not become dominant");
}

#[test]
fn forage_zero_changes_nothing() {
    let cols = 20;
    let rows = 20;
    let n = (cols * rows) as usize;
    let build = || {
        let mut rng = mulberry32(3);
        let mut state = EcologyState::new(cols, rows);
        for i in 0..n {
            let r = rng();
            if r < 0.25 {
                state.grid[i] = CellState::Red as u8;
            } else if r < 0.45 {
                state.grid[i] = CellState::Grey as u8;
            } else if r < 0.55 {
                state.grid[i] = CellState::Marten as u8;
                state.energy[i] = 6.0;
            }
        }
        state
    };
    let p = EcologyParams {
        beta_red: 0.1,
        beta_grey: 0.14,
        sigma: 0.92,
        delta: 1.0,
        g: 3.0,
        e_breed: 10.0,
        e0: 5.0,
        breed_cost: 5.0,
        e_cap: 15.0,
        mu: 0.5,
        evade_red: 0.7,
        evade_grey: 0.05,
        gen: 5,
        ..Default::default()
    };
    let s_a = build();
    let s_b = build();
    let out_a = step_ecology(&s_a, TORUS_TOPO, &p, &mut mulberry32(9));
    let out_b = step_ecology(&s_b, TORUS_TOPO, &EcologyParams { forage: 0.0, ..p }, &mut mulberry32(9));
    assert_eq!(out_a.grid, out_b.grid, "grid must match");
    assert_eq!(out_a.energy, out_b.energy, "energy must match");
}

#[test]
fn forage_is_a_shared_resource() {
    let cols = 9;
    let rows = 9;
    let p = EcologyParams {
        beta_red: 0.0,
        beta_grey: 0.0,
        sigma: 1.0,
        delta: 1.0,
        g: 3.0,
        e_breed: 100.0,
        e0: 5.0,
        breed_cost: 5.0,
        e_cap: 100.0,
        mu: 0.0,
        forage: 4.0,
        gen: 1,
        ..Default::default()
    };
    let mut state = EcologyState::new(cols, rows);
    state.set(1, 1, CellState::Marten); state.set_energy(1, 1, 5.0); // isolated
    state.set(5, 5, CellState::Marten); state.set_energy(5, 5, 5.0); // crowded
    state.set(6, 5, CellState::Marten); state.set_energy(6, 5, 5.0);

    let out = step_ecology(&state, TORUS_TOPO, &p, &mut || 0.5);
    let lone = out.get_energy(1, 1);
    let crowded = out.get_energy(5, 5);

    assert_eq!(lone, 5.0 - 1.0 + 4.0 / 1.0);
    assert_eq!(crowded, 5.0 - 1.0 + 4.0 / 2.0);
    assert!(crowded < lone);
}

#[test]
fn forage_lets_martens_outlive_prey_and_self_caps() {
    let cols = 40;
    let rows = 20;
    let n = (cols * rows) as usize;
    let run = |forage: f64| {
        let mut rng = mulberry32(11);
        let mut params = EcologyParams {
            beta_red: 0.0,
            beta_grey: 0.0,
            sigma: 1.0,
            delta: 1.0,
            g: 3.0,
            e_breed: 10.0,
            e0: 5.0,
            breed_cost: 5.0,
            e_cap: 15.0,
            mu: 0.5,
            forage,
            gen: 0,
            ..Default::default()
        };
        let mut state = EcologyState::new(cols, rows);
        for i in 0..n {
            if rng() < 0.10 {
                state.grid[i] = CellState::Marten as u8;
                state.energy[i] = 8.0;
            }
        }
        for gen in 0..400 {
            params.gen = gen;
            state = step_ecology(&state, TORUS_TOPO, &params, &mut rng);
        }
        state.grid.iter().filter(|&&v| v == CellState::Marten as u8).count()
    };
    assert_eq!(run(0.0), 0, "without forage martens starve");
    let fed = run(2.0);
    assert!(fed > 0, "with forage martens persist (got {})", fed);
    assert!(fed < (n as f32 * 0.5) as usize, "density must self-cap (got {} of {})", fed, n);
}

#[test]
fn hyperpredation_subsidised_martens_extirpate_greys() {
    let cols = 60;
    let rows = 30;
    let n = (cols * rows) as usize;
    let greys_after = |forage: f64| {
        let mut params = EcologyParams {
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
            evade_red: 0.70,
            evade_grey: 0.05,
            forage,
            gen: 0,
            ..Default::default()
        };
        let mut rng = mulberry32(12345);
        let mut seed_rng = mulberry32(999);
        let mut state = EcologyState::new(cols, rows);
        for i in 0..n {
            let r = seed_rng();
            if r < 0.30 { state.grid[i] = CellState::Red as u8; }
            else if r < 0.60 { state.grid[i] = CellState::Grey as u8; }
        }
        for gen in 0..1000 {
            if gen == 300 {
                for i in 0..n {
                    if state.grid[i] != CellState::Marten as u8 && rng() < 0.02 {
                        state.grid[i] = CellState::Marten as u8;
                        state.energy[i] = 8.0;
                    }
                }
            }
            params.gen = gen;
            state = step_ecology(&state, TORUS_TOPO, &params, &mut rng);
        }
        state.grid.iter().filter(|&&v| v == CellState::Grey as u8).count()
    };
    assert!(greys_after(0.0) > 0, "without forage greys survive");
    assert_eq!(greys_after(1.0), 0, "with forage greys are extirpated");
}

#[test]
fn mate_rule_lone_marten_cannot_breed() {
    let cols = 9;
    let rows = 9;
    let p = EcologyParams {
        beta_red: 0.0,
        beta_grey: 0.0,
        sigma: 1.0,
        delta: 1.0,
        g: 3.0,
        e_breed: 10.0,
        e0: 5.0,
        breed_cost: 5.0,
        e_cap: 100.0,
        mu: 1.0,
        gen: 1,
        ..Default::default()
    };
    let martens_after = |mate: bool, paired: bool| {
        let mut state = EcologyState::new(cols, rows);
        state.set(4, 4, CellState::Marten); state.set_energy(4, 4, 50.0);
        if paired {
            state.set(5, 4, CellState::Marten); state.set_energy(5, 4, 50.0);
        }
        let out = step_ecology(&state, TORUS_TOPO, &EcologyParams { mate, ..p.clone() }, &mut || 0.5);
        out.grid.iter().filter(|&&v| v == CellState::Marten as u8).count()
    };
    assert!(martens_after(false, false) > 1);
    assert_eq!(martens_after(true, false), 1);
    assert!(martens_after(true, true) > 2);
}

#[test]
fn mate_parent_not_charged_breed_cost_when_alone() {
    let cols = 9;
    let rows = 9;
    let mut state = EcologyState::new(cols, rows);
    state.set(4, 4, CellState::Marten); state.set_energy(4, 4, 12.0);
    let p = EcologyParams {
        beta_red: 0.0,
        beta_grey: 0.0,
        sigma: 1.0,
        delta: 1.0,
        g: 3.0,
        e_breed: 10.0,
        e0: 5.0,
        breed_cost: 5.0,
        e_cap: 100.0,
        mu: 0.5,
        mate: true,
        gen: 1,
        ..Default::default()
    };
    let out = step_ecology(&state, TORUS_TOPO, &p, &mut || 0.9);
    let e = out.get_energy(4, 4);
    assert_eq!(e, 12.0 - 1.0);
}

#[test]
fn mortality_martens_die_at_background_rate() {
    let cols = 40;
    let rows = 40;
    let n = (cols * rows) as usize;
    let p = EcologyParams {
        beta_red: 0.0,
        beta_grey: 0.0,
        sigma: 1.0,
        delta: 0.0,
        g: 3.0,
        e_breed: 1000.0,
        e0: 5.0,
        breed_cost: 5.0,
        e_cap: 100.0,
        mu: 0.0,
        mortality: 0.10,
        gen: 1,
        ..Default::default()
    };
    let mut state = EcologyState::new(cols, rows);
    for i in 0..n {
        state.grid[i] = CellState::Marten as u8;
        state.energy[i] = 99.0;
    }
    let out = step_ecology(&state, TORUS_TOPO, &p, &mut mulberry32(5));
    let alive = out.grid.iter().filter(|&&v| v == CellState::Marten as u8).count();
    let died_frac = (n - alive) as f32 / n as f32;
    assert!((died_frac - 0.10).abs() < 0.02, "~10% background mortality");
}

#[test]
fn mortality_breakeven_marten_not_immortal() {
    let cols = 9;
    let rows = 9;
    let p = EcologyParams {
        beta_red: 0.0,
        beta_grey: 0.0,
        sigma: 1.0,
        delta: 1.0,
        g: 3.0,
        e_breed: 100.0,
        e0: 5.0,
        breed_cost: 5.0,
        e_cap: 15.0,
        mu: 0.0,
        forage: 1.0,
        gen: 0,
        ..Default::default()
    };
    let survive_check = |mortality: f64| {
        let mut state = EcologyState::new(cols, rows);
        state.set(4, 4, CellState::Marten); state.set_energy(4, 4, 5.0);
        let mut rng = mulberry32(2);
        let mut params = EcologyParams { mortality, ..p.clone() };
        for gen in 0..2000 {
            params.gen = gen;
            state = step_ecology(&state, TORUS_TOPO, &params, &mut rng);
        }
        state.get(4, 4) == CellState::Marten
    };
    assert_eq!(survive_check(0.0), true);
    assert_eq!(survive_check(0.005), false);
}

#[test]
fn pox_beta_zero_changes_nothing() {
    let cols = 20;
    let rows = 20;
    let n = (cols * rows) as usize;
    let build = || {
        let mut rng = mulberry32(3);
        let mut state = EcologyState::new(cols, rows);
        for i in 0..n {
            let r = rng();
            if r < 0.25 { state.grid[i] = CellState::Red as u8; }
            else if r < 0.45 { state.grid[i] = CellState::Grey as u8; }
            else if r < 0.55 { state.grid[i] = CellState::Marten as u8; state.energy[i] = 6.0; }
            if state.grid[i] == CellState::Grey as u8 { state.infected[i] = 1; }
        }
        state
    };
    let p = EcologyParams {
        beta_red: 0.1,
        beta_grey: 0.14,
        sigma: 0.92,
        delta: 1.0,
        g: 3.0,
        e_breed: 10.0,
        e0: 5.0,
        breed_cost: 5.0,
        e_cap: 15.0,
        mu: 0.5,
        evade_red: 0.9,
        evade_grey: 0.05,
        forage: 2.0,
        gen: 5,
        ..Default::default()
    };
    let s_a = build();
    let s_b = build();
    let out_a = step_ecology(&s_a, TORUS_TOPO, &p, &mut mulberry32(9));
    let out_b = step_ecology(&s_b, TORUS_TOPO, &EcologyParams { pox_beta: 0.0, pox_lethal: 0.5, ..p }, &mut mulberry32(9));
    assert_eq!(out_a.grid, out_b.grid);
    assert_eq!(out_a.energy, out_b.energy);
}

#[test]
fn pox_greys_asymptomatic_reservoir_reds_killed() {
    let cols = 5;
    let rows = 5;
    let p = EcologyParams { pox_beta: 1e-9, pox_lethal: 1.0, ..POX };
    let mut state = EcologyState::new(cols, rows);
    state.set(1, 1, CellState::Grey); state.set_infected(1, 1, 1);
    state.set(3, 3, CellState::Red);  state.set_infected(3, 3, 1);

    let out = step_ecology(&state, TORUS_TOPO, &p, &mut || 0.5);
    assert_eq!(out.get(1, 1), CellState::Grey);
    assert_eq!(out.get_infected(1, 1), 1);
    assert_eq!(out.get(3, 3), CellState::Empty);
}

#[test]
fn pox_uninfected_prey_catches_it() {
    let cols = 3;
    let rows = 3;
    let k = 2.0f64;
    let pox_beta = 0.3f64;
    let expected = 1.0 - (1.0 - pox_beta).powf(k);
    let mut caught = 0;
    let trials = 20000;

    for t in 0..trials {
        let mut state = EcologyState::new(cols, rows);
        state.set(1, 1, CellState::Grey);
        state.set(0, 1, CellState::Grey); state.set_infected(0, 1, 1);
        state.set(2, 1, CellState::Grey); state.set_infected(2, 1, 1);
        let out = step_ecology(&state, NONE_TOPO, &EcologyParams { pox_beta, pox_lethal: 0.0, ..POX }, &mut mulberry32((t + 1) as u32));
        if out.get_infected(1, 1) != 0 {
            caught += 1;
        }
    }
    let rate = caught as f64 / trials as f64;
    assert!((rate - expected).abs() < 0.02, "infection rate {}", rate);
}

#[test]
fn pox_newborn_prey_not_born_infected() {
    let cols = 3;
    let rows = 3;
    let mut state = EcologyState::new(cols, rows);
    state.set(0, 1, CellState::Grey); state.set_infected(0, 1, 1);
    state.set(2, 1, CellState::Grey); state.set_infected(2, 1, 1);
    let out = step_ecology(&state, NONE_TOPO, &EcologyParams { beta_grey: 1.0, pox_beta: 0.5, pox_lethal: 0.0, ..POX }, &mut || 0.01);
    assert_eq!(out.get(1, 1), CellState::Grey);
    assert_eq!(out.get_infected(1, 1), 0);
}

#[test]
fn pox_cannot_persist_without_grey_reservoir() {
    let cols = 30;
    let rows = 30;
    let n = (cols * rows) as usize;
    let mut rng = mulberry32(4);
    let mut state = EcologyState::new(cols, rows);
    for i in 0..n {
        if rng() < 0.5 {
            state.grid[i] = CellState::Red as u8;
            if rng() < 0.3 {
                state.infected[i] = 1;
            }
        }
    }
    let mut params = EcologyParams {
        beta_red: 0.10,
        beta_grey: 0.0,
        sigma: 0.92,
        delta: 1.0,
        g: 3.0,
        e_breed: 10.0,
        e0: 5.0,
        breed_cost: 5.0,
        e_cap: 15.0,
        mu: 0.5,
        pox_beta: 0.3,
        pox_lethal: 0.5,
        gen: 0,
        ..Default::default()
    };
    for gen in 0..300 {
        params.gen = gen;
        state = step_ecology(&state, TORUS_TOPO, &params, &mut rng);
    }
    let mut still_infected = 0;
    let mut reds = 0;
    for i in 0..n {
        if state.infected[i] != 0 { still_infected += 1; }
        if state.grid[i] == CellState::Red as u8 { reds += 1; }
    }
    assert_eq!(still_infected, 0, "pox burns out without greys");
    assert!(reds > 0, "surviving reds recover");
}

#[test]
fn pox_acceleration_pathogen_replaces_reds_faster() {
    let cols = 60;
    let rows = 30;
    let n = (cols * rows) as usize;
    let reds_left = |pox_beta: f64| {
        let mut params = EcologyParams {
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
            evade_red: 0.90,
            evade_grey: 0.05,
            pox_beta,
            pox_lethal: 0.5,
            gen: 0,
            ..Default::default()
        };
        let mut rng = mulberry32(12345);
        let mut seed_rng = mulberry32(999);
        let mut state = EcologyState::new(cols, rows);
        for i in 0..n {
            let r = seed_rng();
            if r < 0.30 { state.grid[i] = CellState::Red as u8; }
            else if r < 0.60 { state.grid[i] = CellState::Grey as u8; }
        }
        for i in 0..n {
            if state.grid[i] == CellState::Grey as u8 {
                state.infected[i] = 1;
                break;
            }
        }
        let mut trace = Vec::new();
        for gen in 0..200 {
            params.gen = gen;
            state = step_ecology(&state, TORUS_TOPO, &params, &mut rng);
            let reds = state.grid.iter().filter(|&&v| v == CellState::Red as u8).count();
            trace.push(reds);
        }
        trace
    };
    let clean = reds_left(0.0);
    let poxed = reds_left(0.25);
    assert!(poxed[199] < (clean[199] as f32 * 0.5) as usize);

    let half_of = |t: &[usize]| {
        let start = t[0];
        t.iter().position(|&v| v < start / 2).unwrap_or(t.len())
    };
    assert!(half_of(&poxed) < half_of(&clean));
}

#[test]
fn terrain_zero_changes_nothing() {
    let cols = 20;
    let rows = 20;
    let n = (cols * rows) as usize;
    let mut env = vec![0.0f32; n];
    for i in 0..n {
        env[i] = (i as f32).sin();
    }
    let build = || {
        let mut rng = mulberry32(3);
        let mut state = EcologyState::new(cols, rows);
        for i in 0..n {
            let r = rng();
            if r < 0.25 { state.grid[i] = CellState::Red as u8; }
            else if r < 0.45 { state.grid[i] = CellState::Grey as u8; }
            else if r < 0.55 { state.grid[i] = CellState::Marten as u8; state.energy[i] = 6.0; }
        }
        state
    };
    let p = EcologyParams {
        beta_red: 0.1,
        beta_grey: 0.14,
        sigma: 0.92,
        delta: 1.0,
        g: 3.0,
        e_breed: 10.0,
        e0: 5.0,
        breed_cost: 5.0,
        e_cap: 15.0,
        mu: 0.5,
        evade_red: 0.9,
        evade_grey: 0.05,
        forage: 2.0,
        gen: 5,
        ..Default::default()
    };
    let s_a = build();
    let mut s_b = build();
    s_b.env = Some(env.clone());
    let out_a = step_ecology(&s_a, TORUS_TOPO, &p, &mut mulberry32(9));
    let out_b = step_ecology(&s_b, TORUS_TOPO, &EcologyParams { terrain: 0.0, ..p }, &mut mulberry32(9));
    assert_eq!(out_a.grid, out_b.grid);
    assert_eq!(out_a.energy, out_b.energy);
}

#[test]
fn terrain_conifer_favours_red_broadleaf_favours_grey() {
    let cols = 3;
    let rows = 3;
    let n = (cols * rows) as usize;
    let contest = |env_val: f64| {
        let env = vec![env_val as f32; n];
        let mut reds = 0;
        let mut greys = 0;
        let p = EcologyParams {
            beta_red: 0.10,
            beta_grey: 0.14,
            sigma: 1.0,
            terrain: 0.8,
            ..Default::default()
        };
        for t in 0..20000 {
            let mut state = EcologyState::new(cols, rows);
            state.set(0, 1, CellState::Red);
            state.set(2, 1, CellState::Grey);
            state.env = Some(env.clone());
            let out = step_ecology(&state, NONE_TOPO, &p, &mut mulberry32((t + 1) as u32));
            match out.get(1, 1) {
                CellState::Red => reds += 1,
                CellState::Grey => greys += 1,
                _ => {}
            }
        }
        (reds, greys)
    };
    let conifer = contest(-1.0);
    let broadleaf = contest(1.0);
    assert!(conifer.0 > conifer.1, "conifer favours red: red {} vs grey {}", conifer.0, conifer.1);
    assert!(broadleaf.1 > broadleaf.0 * 2, "broadleaf favours grey: grey {} vs red {}", broadleaf.1, broadleaf.0);
}

#[test]
fn terrain_red_is_never_excluded_from_broadleaf() {
    let cols = 3;
    let rows = 3;
    let n = (cols * rows) as usize;
    let red_alone = |env_val: f64| {
        let env = vec![env_val as f32; n];
        let mut reds = 0;
        let trials = 20000;
        let p = EcologyParams {
            beta_red: 0.30,
            beta_grey: 0.14,
            sigma: 1.0,
            terrain: 1.0,
            ..Default::default()
        };
        for t in 0..trials {
            let mut state = EcologyState::new(cols, rows);
            state.set(0, 1, CellState::Red);
            state.set(2, 1, CellState::Red);
            state.env = Some(env.clone());
            let out = step_ecology(&state, NONE_TOPO, &p, &mut mulberry32((t + 1) as u32));
            if out.get(1, 1) == CellState::Red {
                reds += 1;
            }
        }
        reds as f64 / trials as f64
    };
    let in_conifer = red_alone(-1.0);
    let in_broadleaf = red_alone(1.0);
    let expected = 1.0 - (1.0 - 0.30f64).powi(2);
    assert!((in_broadleaf - expected).abs() < 0.02);
    assert!((in_conifer - in_broadleaf).abs() < 0.02);
}

#[test]
fn terrain_marten_indifferent_to_it() {
    let cols = 12;
    let rows = 12;
    let n = (cols * rows) as usize;
    let run = |env_val: f64| {
        let env = vec![env_val as f32; n];
        let mut rng = mulberry32(7);
        let mut state = EcologyState::new(cols, rows);
        for i in 0..n {
            if i % 3 == 0 {
                state.grid[i] = CellState::Marten as u8;
                state.energy[i] = 12.0;
            }
        }
        state.env = Some(env);
        let p = EcologyParams {
            beta_red: 0.1,
            beta_grey: 0.14,
            sigma: 0.92,
            delta: 1.0,
            g: 3.0,
            e_breed: 10.0,
            e0: 5.0,
            breed_cost: 5.0,
            e_cap: 15.0,
            mu: 0.5,
            forage: 2.0,
            terrain: 1.0,
            gen: 3,
            ..Default::default()
        };
        let out = step_ecology(&state, TORUS_TOPO, &p, &mut rng);
        let count = out.grid.iter().filter(|&&v| v == CellState::Marten as u8).count();
        (count, out.energy)
    };
    let conifer = run(-1.0);
    let broadleaf = run(1.0);
    assert_eq!(conifer.0, broadleaf.0);
    assert_eq!(conifer.1, broadleaf.1);
}

#[test]
fn refuge_conifer_patch_keeps_reds_alive_through_pox() {
    let cols = 80;
    let rows = 40;
    let n = (cols * rows) as usize;
    let survivors = |terrain: f64| {
        let mut env = vec![0.0f32; n];
        for r in 0..rows {
            for c in 0..cols {
                let in_refuge = c < cols / 3;
                env[(r * cols + c) as usize] = if in_refuge { -1.0 } else { 1.0 };
            }
        }
        let mut params = EcologyParams {
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
            forage: 2.0,
            evade_red: 0.90,
            evade_grey: 0.05,
            pox_beta: 0.25,
            pox_lethal: 0.5,
            terrain,
            gen: 0,
            ..Default::default()
        };
        let mut rng = mulberry32(12345);
        let mut seed_rng = mulberry32(999);
        let mut state = EcologyState::new(cols, rows);
        state.env = Some(env);
        for i in 0..n {
            let r = seed_rng();
            if r < 0.30 { state.grid[i] = CellState::Red as u8; }
            else if r < 0.60 { state.grid[i] = CellState::Grey as u8; }
        }
        for gen in 0..600 {
            if gen == 200 {
                let mut count = 0;
                for i in 0..n {
                    if count >= 20 { break; }
                    if state.grid[i] == CellState::Grey as u8 && rng() < 0.02 {
                        state.infected[i] = 1;
                        count += 1;
                    }
                }
            }
            params.gen = gen;
            state = step_ecology(&state, TORUS_TOPO, &params, &mut rng);
        }
        state.grid.iter().filter(|&&v| v == CellState::Red as u8).count()
    };
    assert_eq!(survivors(0.0), 0, "without terrain pox takes every red");
    assert!(survivors(0.9) > 0, "conifer refuge leaves reds alive");
}

#[test]
fn breakout_reds_locked_in_stronghold_until_martens_arrive() {
    let cols = 120;
    let rows = 60;
    let n = (cols * rows) as usize;
    let frac = cols / 3;
    let mut env = vec![0.0f32; n];
    for r in 0..rows {
        for c in 0..cols {
            env[(r * cols + c) as usize] = if c < frac { -1.0 } else { 1.0 };
        }
    }
    let outside_reds = |martens: bool| {
        let mut params = EcologyParams {
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
            forage: 2.0,
            mate: true,
            mortality: 0.005,
            evade_red: 0.90,
            evade_grey: 0.05,
            pox_beta: 0.25,
            pox_lethal: 0.5,
            terrain: 0.9,
            gen: 0,
        };
        let mut rng = mulberry32(31);
        let mut seed_rng = mulberry32(901);
        let mut state = EcologyState::new(cols, rows);
        state.env = Some(env.clone());
        for i in 0..n {
            let r = seed_rng();
            if r < 0.30 { state.grid[i] = CellState::Red as u8; }
            else if r < 0.60 { state.grid[i] = CellState::Grey as u8; }
        }
        for gen in 0..2200 {
            if gen == 200 {
                let mut count = 0;
                for i in 0..n {
                    if count >= 20 { break; }
                    if state.grid[i] == CellState::Grey as u8 && rng() < 0.02 {
                        state.infected[i] = 1;
                        count += 1;
                    }
                }
            }
            if gen == 800 && martens {
                let mut placed = 0;
                let c0 = (cols as f32 * 0.75) as i32;
                let r0 = (rows >> 1) as i32;
                for rad in 0..40 {
                    let rad_i = rad as i32;
                    for dr in -rad_i..=rad_i {
                        if placed >= 40 { break; }
                        for dc in -rad_i..=rad_i {
                            if placed >= 40 { break; }
                            if dr.abs().max(dc.abs()) != rad_i { continue; }
                            let c_wrap = (c0 + dc).rem_euclid(cols as i32) as u32;
                            let r_wrap = (r0 + dr).rem_euclid(rows as i32) as u32;
                            let i = state.idx(c_wrap, r_wrap);
                            if state.grid[i] != CellState::Marten as u8 {
                                state.grid[i] = CellState::Marten as u8;
                                state.energy[i] = 8.0;
                                placed += 1;
                            }
                        }
                    }
                }
            }
            params.gen = gen;
            state = step_ecology(&state, TORUS_TOPO, &params, &mut rng);
        }
        let (mut in_r, mut out_r) = (0, 0);
        for r in 0..rows {
            for c in 0..cols {
                if state.get(c, r) == CellState::Red {
                    if c < frac { in_r += 1; } else { out_r += 1; }
                }
            }
        }
        (in_r, out_r)
    };
    let stuck = outside_reds(false);
    assert!(stuck.0 > 500, "reds survive in refuge (got {})", stuck.0);
    assert!(stuck.1 < (stuck.0 as f32 * 0.05) as i32, "stay locked inside without martens ({} escaped)", stuck.1);

    let freed = outside_reds(true);
    assert!(freed.1 > stuck.0, "martens let reds break out ({} outside vs {} penned)", freed.1, stuck.0);
}
