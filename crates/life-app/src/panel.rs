//! The egui control panel: what it shows, and what the user changed.
//!
//! Split from `app.rs` so that adding a simulation mode touches one screenful
//! of UI rather than the middle of the frame loop. The panel owns no state. It
//! reads a [`PanelView`] snapshot, writes into a [`PanelEdits`], and `app.rs`
//! decides what any of it means — which is why a slider moving and a simulation
//! rebuilding stay separate concerns.

use std::collections::VecDeque;
use life_core::{Topology, Wrap};
use life_gpu::ecology_sim::MAX_AXIS as ECOLOGY_MAX_AXIS;

/// Largest grid axis this mode allows. The GPU modes are bounded by texture size;
/// ecology is bounded by how fast a CPU can step it.
pub fn max_axis(mode: SimMode) -> u32 {
    match mode {
        SimMode::Ecology => ECOLOGY_MAX_AXIS,
        _ => 2048,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SimMode {
    #[default]
    Deterministic,
    Stochastic,
    Evolve,
    Ecology,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Orbit,
    Pen,
    Glider,
    Terrain,
}

/// Evolve presets, ported from the browser's preset dropdown.
/// (name, weighted kernel, rule, hatch)
pub const PRESETS: [(&str, bool, &str, bool); 8] = [
    ("Conway — B3/S23", false, "B3/S23", false),
    ("HighLife — B36/S23", false, "B36/S23", false),
    ("3-4 Life — B34/S34", false, "B34/S34", false),
    ("Coral (dense borders)", true, "B6-14/S4-8", false),
    ("Blobs", true, "B10-18/S12-16", false),
    ("Hatch — lace disc", true, "B16-18/S14-26", true),
    ("Hatch — ring", true, "B16-22/S16-28", true),
    ("Hatch — blob", true, "B16-20/S16-28", true),
];

/// Stochastic-only knobs: the Gaussian bump around the deterministic B/S counts.
#[derive(Clone)]
pub struct StochasticUi {
    pub sigma: f32,
    pub floor: f32,
    pub ceil: f32,
}

/// Evolve-only knobs, including the two pen settings that exist only in that mode.
#[derive(Clone)]
pub struct EvolveUi {
    pub rule_text: String,
    pub env_weight: f32,
    pub mut_sigma: f32,
    pub seed_tau: f32,
    pub kernel_weighted: bool,
    pub pen_clan: u8,      // 1 warm / 2 cool
    pub terrain_sign: f32, // +1 fertile / -1 hostile
}

/// Ecology knobs: what seeds a board, and the rule's own parameters.
///
/// These are f64 to match `EcologyParams`, which is f64 because the reference
/// implementation is JS — see the float-width invariant in CLAUDE.md.
#[derive(Clone)]
pub struct EcologyUi {
    /// Martens seeded per empty cell. The browser introduces them with a release
    /// button instead; that is not ported yet, and with none seeded there is no
    /// predation to watch.
    pub marten_frac: f64,
    /// Fixed so a run is reproducible from the panel, as the browser's boardSeed is.
    pub seed: u32,
    // prey: contact-process colonisation and survival
    pub beta_red: f64,
    pub beta_grey: f64,
    pub sigma: f64,
    // marten: the energy ledger
    pub delta: f64,      // drain per tick
    pub g: f64,          // gain per meal
    pub e0: f64,         // energy a newborn starts with
    pub e_breed: f64,    // threshold to breed
    pub breed_cost: f64, // paid on breeding
    pub e_cap: f64,      // sated; stops hunting above this
    pub mu: f64,         // breeding spill into an empty neighbour
}

/// Everything the panel may edit, held in one place so `App` and `PanelEdits`
/// cannot drift apart — the copy-back after a frame is a single assignment
/// rather than one line per field.
#[derive(Clone)]
pub struct Tunables {
    pub sim_mode: SimMode,
    pub tool: Tool,
    pub rule_text: String,
    pub density: f32,
    pub target_gps: f32,
    pub grid_w: u32,
    pub grid_h: u32,
    pub topo: Topology,
    pub stochastic: StochasticUi,
    pub evolve: EvolveUi,
    pub ecology: EcologyUi,
}

impl Default for Tunables {
    fn default() -> Self {
        Tunables {
            sim_mode: SimMode::Deterministic,
            tool: Tool::Orbit,
            rule_text: "B3/S23".into(),
            density: 0.3,
            target_gps: 12.0,
            grid_w: 128,
            grid_h: 128,
            topo: Topology { x: Wrap::Straight, y: Wrap::Straight },
            stochastic: StochasticUi { sigma: 0.6, floor: 0.0, ceil: 1.0 },
            evolve: EvolveUi {
                rule_text: "B3/S23".into(),
                env_weight: 4.0,
                mut_sigma: 0.05,
                seed_tau: 0.5,
                kernel_weighted: false,
                pen_clan: 1,
                terrain_sign: 1.0,
            },
            // rule defaults match EcologyParams::default(), which matches the browser
            ecology: EcologyUi {
                marten_frac: 0.02,
                seed: 12345,
                beta_red: 0.10,
                beta_grey: 0.14,
                sigma: 0.92,
                delta: 1.0,
                g: 3.0,
                e0: 5.0,
                e_breed: 10.0,
                breed_cost: 5.0,
                e_cap: 15.0,
                mu: 0.5,
            },
        }
    }
}

/// Read-only snapshot the panel displays. Owned rather than borrowed because
/// the egui layer is mutably borrowed from `App` while the panel runs.
pub struct PanelView {
    pub running: bool,
    pub period: Option<u64>,
    pub searching: bool,
    pub search_result: String,
    pub color_mode: u32,
    pub evolve_stats: String,
    pub tau_history: VecDeque<(f32, f32)>,
    pub ecology_counts: String,
}

/// One frame of user intent. `t` is the edited state; the rest are one-shot
/// signals that `app.rs` consumes and clears by dropping this struct.
pub struct PanelEdits {
    pub t: Tunables,
    pub rule_changed: bool,
    pub toggle_run: bool,
    pub step: bool,
    pub reset: bool,
    pub clear: bool,
    pub params_changed: bool,
    pub start_search: bool,
    pub grid_dim_changed: bool,
    pub topo_changed: bool,
    pub evolve_rule_changed: bool,
    pub kernel_changed: bool,
    pub gen_terrain: bool,
    pub flat_terrain: bool,
    pub cycle_color: bool,
    pub preset: Option<usize>,
}

impl PanelEdits {
    /// Start from the current state: anything the user does not touch stays put.
    pub fn from(t: &Tunables) -> Self {
        PanelEdits {
            t: t.clone(),
            rule_changed: false,
            toggle_run: false,
            step: false,
            reset: false,
            clear: false,
            params_changed: false,
            start_search: false,
            grid_dim_changed: false,
            topo_changed: false,
            evolve_rule_changed: false,
            kernel_changed: false,
            gen_terrain: false,
            flat_terrain: false,
            cycle_color: false,
            preset: None,
        }
    }
}

pub fn draw(ctx: &egui::Context, view: &PanelView, edits: &mut PanelEdits) {
    egui::Window::new("life").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut edits.t.sim_mode, SimMode::Deterministic, "Torus");
            ui.selectable_value(&mut edits.t.sim_mode, SimMode::Stochastic, "Stochastic");
            ui.selectable_value(&mut edits.t.sim_mode, SimMode::Evolve, "Evolve");
            ui.selectable_value(&mut edits.t.sim_mode, SimMode::Ecology, "Ecology");
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut edits.t.tool, Tool::Orbit, "🔄 Orbit");
            ui.selectable_value(&mut edits.t.tool, Tool::Pen, "✏ Pen");
            ui.selectable_value(&mut edits.t.tool, Tool::Glider, "🚀 Glider");
            if edits.t.sim_mode == SimMode::Evolve {
                ui.selectable_value(&mut edits.t.tool, Tool::Terrain, "⛰ Terrain");
            }
        });
        if edits.t.sim_mode == SimMode::Evolve {
            ui.horizontal(|ui| {
                if matches!(edits.t.tool, Tool::Pen | Tool::Glider) {
                    ui.selectable_value(&mut edits.t.evolve.pen_clan, 1, "Warm");
                    ui.selectable_value(&mut edits.t.evolve.pen_clan, 2, "Cool");
                }
                if edits.t.tool == Tool::Terrain {
                    ui.selectable_value(&mut edits.t.evolve.terrain_sign, 1.0, "Fertile +");
                    ui.selectable_value(&mut edits.t.evolve.terrain_sign, -1.0, "Hostile −");
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("Rule");
            if edits.t.sim_mode == SimMode::Evolve {
                if ui.text_edit_singleline(&mut edits.t.evolve.rule_text).lost_focus() {
                    edits.evolve_rule_changed = true;
                }
            } else if ui.text_edit_singleline(&mut edits.t.rule_text).lost_focus() {
                edits.rule_changed = true;
            }
        });
        ui.horizontal(|ui| {
            if ui.button(if view.running { "Pause" } else { "Run" }).clicked() {
                edits.toggle_run = true;
            }
            if ui.button("Step").clicked() {
                edits.step = true;
            }
            if ui.button("Reset").clicked() {
                edits.reset = true;
            }
            if ui.button("Clear").clicked() {
                edits.clear = true;
            }
        });
        ui.add(egui::Slider::new(&mut edits.t.target_gps, 0.5..=120.0).logarithmic(true).text("gens/s"));
        ui.horizontal(|ui| {
            ui.label("Size");
            // ecology is CPU-stepped and cannot go as large, so offer the sizes it
            // can actually run rather than accepting one and silently clamping it
            let cap = max_axis(edits.t.sim_mode);
            egui::ComboBox::from_id_salt("grid_dim")
                .selected_text(format!("{}×{}", edits.t.grid_w, edits.t.grid_h))
                .show_ui(ui, |ui| {
                    for &n in [64u32, 128, 256, 512, 1024, 2048].iter().filter(|&&n| n <= cap) {
                        let sel = edits.t.grid_w == n && edits.t.grid_h == n;
                        if ui.selectable_label(sel, format!("{n}×{n}")).clicked() {
                            edits.t.grid_w = n;
                            edits.t.grid_h = n;
                            edits.grid_dim_changed = true;
                        }
                    }
                });
            for v in [&mut edits.t.grid_w, &mut edits.t.grid_h] {
                let r = ui.add(egui::DragValue::new(v).range(16..=cap).speed(8));
                if r.drag_stopped() || r.lost_focus() {
                    edits.grid_dim_changed = true;
                }
            }
        });
        ui.horizontal(|ui| {
            let wrap_name = |w: Wrap| match w {
                Wrap::Straight => "wrap",
                Wrap::None => "edge",
                Wrap::Flip => "flip",
            };
            for (label, axis, id) in [
                ("X", &mut edits.t.topo.x, "wrap_x"),
                ("Y", &mut edits.t.topo.y, "wrap_y"),
            ] {
                ui.label(label);
                egui::ComboBox::from_id_salt(id)
                    .selected_text(wrap_name(*axis))
                    .show_ui(ui, |ui| {
                        for w in [Wrap::Straight, Wrap::None, Wrap::Flip] {
                            if ui.selectable_value(axis, w, wrap_name(w)).clicked() {
                                edits.topo_changed = true;
                            }
                        }
                    });
            }
        });
        if edits.t.sim_mode == SimMode::Stochastic {
            ui.separator();
            edits.params_changed |= ui.add(egui::Slider::new(&mut edits.t.stochastic.sigma, 0.05..=2.0).text("σ")).drag_stopped();
            edits.params_changed |= ui.add(egui::Slider::new(&mut edits.t.stochastic.floor, 0.0..=0.5).text("floor")).drag_stopped();
            edits.params_changed |= ui.add(egui::Slider::new(&mut edits.t.stochastic.ceil, 0.5..=1.0).text("ceil")).drag_stopped();
            ui.add(egui::Slider::new(&mut edits.t.density, 0.0..=1.0).text("density"));
            if ui.button("Reseed").clicked() {
                edits.reset = true;
            }
            ui.separator();
            if view.searching {
                ui.label("searching…");
            } else if ui.button("Stability search (20 trials)").clicked() {
                edits.start_search = true;
            }
            if !view.search_result.is_empty() {
                ui.label(&view.search_result);
            }
        } else if edits.t.sim_mode == SimMode::Evolve {
            ui.separator();
            egui::ComboBox::from_label("preset")
                .selected_text("Presets…")
                .show_ui(ui, |ui| {
                    for (i, (name, ..)) in PRESETS.iter().enumerate() {
                        if ui.selectable_label(false, *name).clicked() {
                            edits.preset = Some(i);
                        }
                    }
                });
            ui.horizontal(|ui| {
                let before = edits.t.evolve.kernel_weighted;
                ui.selectable_value(&mut edits.t.evolve.kernel_weighted, false, "Moore");
                ui.selectable_value(&mut edits.t.evolve.kernel_weighted, true, "5×5 weighted");
                edits.kernel_changed = before != edits.t.evolve.kernel_weighted;
            });
            edits.params_changed |= ui.add(egui::Slider::new(&mut edits.t.evolve.env_weight, 0.0..=8.0).text("env weight")).drag_stopped();
            edits.params_changed |= ui.add(egui::Slider::new(&mut edits.t.evolve.mut_sigma, 0.0..=0.3).text("σ mut")).drag_stopped();
            ui.add(egui::Slider::new(&mut edits.t.evolve.seed_tau, 0.0..=1.0).text("seed τ"));
            ui.add(egui::Slider::new(&mut edits.t.density, 0.0..=1.0).text("density"));
            ui.horizontal(|ui| {
                if ui.button("Reseed").clicked() { edits.reset = true; }
                if ui.button("Terrain").clicked() { edits.gen_terrain = true; }
                if ui.button("Flat").clicked() { edits.flat_terrain = true; }
                let label = ["◧ Clan", "◧ τ", "◧ θ", "◧ Env"][view.color_mode as usize];
                if ui.button(label).clicked() { edits.cycle_color = true; }
            });
            if !view.evolve_stats.is_empty() { ui.label(&view.evolve_stats); }
            if view.tau_history.len() > 1 {
                // strip-chart of mean τ per clan over time, y∈[0,1]
                ui.label("τ̄ history");
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().min(260.0), 48.0),
                    egui::Sense::hover(),
                );
                let p = ui.painter_at(rect);
                p.rect_filled(rect, 2.0, egui::Color32::from_gray(20));
                let n = view.tau_history.len();
                let xy = |i: usize, tau: f32| {
                    egui::pos2(
                        rect.left() + rect.width() * i as f32 / (n - 1) as f32,
                        rect.bottom() - rect.height() * tau.clamp(0.0, 1.0),
                    )
                };
                for (col, pick) in [
                    (egui::Color32::from_rgb(230, 120, 80), 0usize), // warm
                    (egui::Color32::from_rgb(90, 150, 230), 1usize), // cool
                ] {
                    let pts: Vec<_> = view.tau_history.iter().enumerate()
                        .map(|(i, s)| xy(i, if pick == 0 { s.0 } else { s.1 }))
                        .collect();
                    p.add(egui::Shape::line(pts, egui::Stroke::new(1.5, col)));
                }
            }
        } else if edits.t.sim_mode == SimMode::Ecology {
            let e = &mut edits.t.ecology;
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("seed");
                if ui.add(egui::DragValue::new(&mut e.seed).speed(1.0)).changed() {
                    // a seed is only meaningful from the start of a run
                    edits.reset = true;
                }
                if ui.button("Reseed").clicked() { edits.reset = true; }
            });
            ui.add(egui::Slider::new(&mut edits.t.density, 0.0..=1.0).text("squirrel density"));
            ui.add(egui::Slider::new(&mut e.marten_frac, 0.0..=0.2).text("marten seed"));

            ui.separator();
            ui.label("prey");
            // these are live: the rule reads params afresh every step, so changing
            // one mid-run is a legitimate experiment rather than a reason to reseed
            let mut live = false;
            live |= ui.add(egui::Slider::new(&mut e.beta_red, 0.0..=1.0).text("β red")).drag_stopped();
            live |= ui.add(egui::Slider::new(&mut e.beta_grey, 0.0..=1.0).text("β grey")).drag_stopped();
            live |= ui.add(egui::Slider::new(&mut e.sigma, 0.0..=1.0).text("σ survival")).drag_stopped();

            ui.label("marten energy");
            live |= ui.add(egui::Slider::new(&mut e.delta, 0.0..=4.0).text("δ drain")).drag_stopped();
            live |= ui.add(egui::Slider::new(&mut e.g, 0.0..=10.0).text("g per meal")).drag_stopped();
            live |= ui.add(egui::Slider::new(&mut e.e0, 0.0..=20.0).text("E₀ newborn")).drag_stopped();
            live |= ui.add(egui::Slider::new(&mut e.e_breed, 0.0..=30.0).text("E breed")).drag_stopped();
            live |= ui.add(egui::Slider::new(&mut e.breed_cost, 0.0..=20.0).text("breed cost")).drag_stopped();
            live |= ui.add(egui::Slider::new(&mut e.e_cap, 1.0..=40.0).text("E cap")).drag_stopped();
            live |= ui.add(egui::Slider::new(&mut e.mu, 0.0..=1.0).text("μ spill")).drag_stopped();
            edits.params_changed |= live;

            ui.separator();
            let label = ["◧ Species", "◧ Marten energy"][view.color_mode.min(1) as usize];
            if ui.button(label).clicked() { edits.cycle_color = true; }
            if !view.ecology_counts.is_empty() { ui.label(&view.ecology_counts); }
        } else {
            ui.add(egui::Slider::new(&mut edits.t.density, 0.0..=1.0).text("density"));
            if ui.button("Reseed").clicked() {
                edits.reset = true;
            }
            ui.label(match view.period {
                Some(1) => "still life".to_string(),
                Some(p) => format!("cycle: period {p}"),
                None => "—".to_string(),
            });
        }
        ui.label("Space run · S step · V view");
    });
}
