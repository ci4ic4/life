//! The one thing slice 2 adds that can be tested without a GPU: the packing.
//!
//! `life-core` already covers the rule. What is new here is the layout handed to
//! `write_texture` — four floats per cell, in the order the fragment shader reads
//! them. A transposed or mis-strided pack renders as garbage on the torus and
//! nothing else would catch it, since a bad upload is still a valid upload.

use life_core::ecology::{CellState, EcologyState};
use life_gpu::ecology_sim::pack;

#[test]
fn pack_lays_out_four_floats_per_cell_in_shader_order() {
    let (cols, rows) = (4u32, 3u32);
    let mut st = EcologyState::new(cols, rows);
    let n = (cols * rows) as usize;

    st.set(1, 0, CellState::Red);
    st.set(2, 1, CellState::Grey);
    st.set(3, 2, CellState::Marten);
    st.set_energy(3, 2, 7.5);
    st.set_infected(2, 1, 1);
    let mut env = vec![0.0f32; n];
    env[st.idx(0, 0)] = -0.5;
    st.env = Some(env);

    let p = pack(&st);
    assert_eq!(p.len(), n * 4, "four channels per cell");

    // r = species, in row-major order matching the texture upload.
    // Indexed by arithmetic rather than st.idx() so the closure holds no borrow on
    // `st` — the last case below mutates it.
    let at = |c: u32, r: u32| (r * cols + c) as usize * 4;
    assert_eq!(p[at(1, 0)], 1.0, "red");
    assert_eq!(p[at(2, 1)], 2.0, "grey");
    assert_eq!(p[at(3, 2)], 3.0, "marten");
    assert_eq!(p[at(0, 0)], 0.0, "empty");

    // g = energy, b = infection, a = terrain
    assert_eq!(p[at(3, 2) + 1], 7.5, "marten energy");
    assert_eq!(p[at(2, 1) + 2], 1.0, "infected grey");
    assert_eq!(p[at(0, 0) + 3], -0.5, "terrain");

    // an absent env field must pack as zero rather than shifting the other channels
    st.env = None;
    let p = pack(&st);
    assert_eq!(p.len(), n * 4);
    assert_eq!(p[at(0, 0) + 3], 0.0);
    assert_eq!(p[at(3, 2)], 3.0, "species survives a missing env");
}

#[test]
fn pack_is_row_major_so_a_column_is_strided_not_contiguous() {
    // The failure this guards is transposition: with cols == rows a transposed
    // pack has the right length and the right values, and only shows up as a
    // mirrored torus. A non-square grid makes it a hard error instead.
    let (cols, rows) = (5u32, 2u32);
    let mut st = EcologyState::new(cols, rows);
    st.set(4, 0, CellState::Red); // last cell of the first row
    let p = pack(&st);
    assert_eq!(p[4 * 4], 1.0, "index 4 is (4,0) in row-major order");
    assert_eq!(p[(cols as usize) * 4], 0.0, "index 5 is (0,1), a different cell");
}
