use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::Arc;
use life_core::{
    curve, generate_terrain, parse_bs, parse_evolve_rule, run_trial, CurveParams, CycleDetector,
    EvolveParams, EvolveState, Grid, Kernel, Thresholds, Topology, TrialOpts, Verdict, Wrap, BS,
};
use life_gpu::{gpu_self_test, EvolveSim, GpuContext, Sim, SimRule};
use life_render::{pick_cell, OrbitCamera, RenderSource, Renderer, ViewMode};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

enum SimKind {
    Binary(Sim),
    Evolve(EvolveSim),
}

pub struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    sim: Option<SimKind>,
    renderer: Option<Renderer>,
    egui: Option<EguiLayer>,
    cam: OrbitCamera,
    mode: ViewMode,
    running: bool,
    step_once: bool,
    rule: BS,
    rule_text: String,
    topo: Topology,
    grid_dim: u32,
    detector: CycleDetector,
    period: Option<u64>,
    // stochastic mode
    sim_mode: SimMode,
    sigma: f32,
    floor: f32,
    ceil: f32,
    density: f32,
    seed_counter: u32,
    // stability search (background thread)
    search_rx: Option<mpsc::Receiver<String>>,
    search_result: String,
    // evolve mode
    env: Vec<f32>,
    env_weight: f32,
    mut_sigma: f32,
    seed_tau: f32,
    kernel_weighted: bool,
    evolve_rule_text: String,
    color_mode: u32, // 0 clan, 1 τ, 2 θ, 3 env
    evolve_stats: String,
    gens_since_stats: u32,
    tau_history: VecDeque<(f32, f32)>, // (warm τ̄, cool τ̄), newest at back
    // pointer tools
    tool: Tool,
    pen_clan: u8,      // 1 warm / 2 cool (evolve pen)
    terrain_sign: f32, // +1 fertile / -1 hostile
    next_seed: SeedKind,
    // input
    dragging: bool,
    painting: bool,
    last_cursor: (f64, f64),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SimMode { Deterministic, Stochastic, Evolve }

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tool {
    #[default]
    Orbit,
    Pen,
    Glider,
    Terrain,
}

/// How the next rebuild seeds the grid; consumed (reset to Soup) after use.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SeedKind {
    #[default]
    Soup,
    Hatch, // compact single-clan patch (LtL creatures die in mixed soup)
    Empty, // blank canvas for pen/glider drawing
}

/// Evolve presets, ported from the browser's preset dropdown.
/// (name, weighted kernel, rule, hatch)
const PRESETS: [(&str, bool, &str, bool); 8] = [
    ("Conway — B3/S23", false, "B3/S23", false),
    ("HighLife — B36/S23", false, "B36/S23", false),
    ("3-4 Life — B34/S34", false, "B34/S34", false),
    ("Coral (dense borders)", true, "B6-14/S4-8", false),
    ("Blobs", true, "B10-18/S12-16", false),
    ("Hatch — lace disc", true, "B16-18/S14-26", true),
    ("Hatch — ring", true, "B16-22/S16-28", true),
    ("Hatch — blob", true, "B16-20/S16-28", true),
];

impl Default for App {
    fn default() -> Self {
        App {
            window: None,
            gpu: None,
            sim: None,
            renderer: None,
            egui: None,
            cam: OrbitCamera::default(),
            mode: ViewMode::Torus,
            running: false,
            step_once: false,
            rule: parse_bs("B3/S23").unwrap(),
            rule_text: "B3/S23".into(),
            topo: Topology { x: Wrap::Straight, y: Wrap::Straight },
            grid_dim: 128,
            detector: CycleDetector::new(64),
            period: None,
            sim_mode: SimMode::Deterministic,
            sigma: 0.6,
            floor: 0.0,
            ceil: 1.0,
            density: 0.3,
            seed_counter: 1,
            search_rx: None,
            search_result: String::new(),
            env: Vec::new(),
            env_weight: 4.0,
            mut_sigma: 0.05,
            seed_tau: 0.5,
            kernel_weighted: false,
            evolve_rule_text: "B3/S23".into(),
            color_mode: 0,
            evolve_stats: String::new(),
            gens_since_stats: 0,
            tau_history: VecDeque::new(),
            tool: Tool::Orbit,
            pen_clan: 1,
            terrain_sign: 1.0,
            next_seed: SeedKind::Soup,
            dragging: false,
            painting: false,
            last_cursor: (0.0, 0.0),
        }
    }
}

// free fn (not a method) so it can run while `self.sim` is mutably borrowed
fn evolve_params_of(rule_text: &str, ranged: bool, env_weight: f32, mut_sigma: f32) -> Option<EvolveParams> {
    let rule = parse_evolve_rule(rule_text, ranged)?;
    Some(EvolveParams { rule, env_weight, mut_sigma })
}

// tiny xorshift for UI-side seeding — determinism doesn't matter here
fn xorshift(state: &mut u64) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 11) as f64 / (1u64 << 53) as f64
}

impl App {
    fn curve_params(&self) -> CurveParams {
        CurveParams { sigma: self.sigma as f64, floor: self.floor as f64, ceil: self.ceil as f64 }
    }

    fn build_rule(&mut self) -> SimRule {
        match self.sim_mode {
            SimMode::Stochastic => {
                let prm = self.curve_params();
                let to_f32 = |t: [f64; 9]| t.map(|v| v as f32);
                self.seed_counter = self.seed_counter.wrapping_add(1);
                SimRule::Stochastic {
                    p_birth: to_f32(curve(self.rule.birth, prm)),
                    p_survive: to_f32(curve(self.rule.survive, prm)),
                    seed: self.seed_counter.wrapping_mul(0x9E3779B9),
                }
            }
            _ => SimRule::Deterministic(self.rule),
        }
    }

    fn seed_grid(&mut self) -> Grid {
        // random soup at the chosen density (both torus and stochastic modes),
        // or a blank canvas when Clear was pressed
        let dim = self.grid_dim;
        let mut g = Grid::new(dim, dim);
        if self.next_seed == SeedKind::Empty {
            return g;
        }
        self.seed_counter = self.seed_counter.wrapping_add(1);
        let mut s = 0x5EED_u64 ^ ((self.seed_counter as u64) << 17);
        for c in g.cells.iter_mut() {
            *c = (xorshift(&mut s) < self.density as f64) as u8;
        }
        g
    }

    fn kernel(&self) -> Kernel {
        if self.kernel_weighted { Kernel::weighted5x5() } else { Kernel::moore() }
    }

    fn evolve_params(&self) -> Option<EvolveParams> {
        let rule = parse_evolve_rule(&self.evolve_rule_text, self.kernel_weighted)?;
        Some(EvolveParams { rule, env_weight: self.env_weight, mut_sigma: self.mut_sigma })
    }

    fn seed_evolve(&mut self) -> EvolveState {
        let dim = self.grid_dim;
        let mut st = EvolveState::new(dim, dim);
        self.seed_counter = self.seed_counter.wrapping_add(1);
        let mut s = 0x50D4_u64 ^ ((self.seed_counter as u64) << 21);
        match self.next_seed {
            SeedKind::Empty => {}
            SeedKind::Hatch => {
                // compact single-clan patch at centre: LtL creatures grow from
                // a lone clan (the rival would kill them in a mixed soup)
                let c = dim / 2;
                let r = 16u32.min(dim / 2 - 2);
                for y in (c - r)..(c + r) {
                    for x in (c - r)..(c + r) {
                        if xorshift(&mut s) < 0.45 {
                            let i = st.idx(x, y);
                            st.grid[i] = self.pen_clan;
                            st.tau[i] = self.seed_tau;
                            st.theta[i] = 0.5;
                        }
                    }
                }
            }
            SeedKind::Soup => {
                // two-clan soup at the chosen density, genes at seed values
                for i in 0..(dim * dim) as usize {
                    if xorshift(&mut s) < self.density as f64 {
                        st.grid[i] = if xorshift(&mut s) < 0.5 { 1 } else { 2 };
                        st.tau[i] = self.seed_tau;
                        st.theta[i] = 0.5;
                    }
                }
            }
        }
        st
    }

    /// Rebuild sim + renderer for the current mode (renderer pipeline differs
    /// per source kind, so it is recreated rather than rebound).
    fn rebuild_sim(&mut self) {
        let gpu_size = {
            let gpu = self.gpu.as_ref().unwrap();
            (gpu.config.width, gpu.config.height)
        };
        match self.sim_mode {
            SimMode::Evolve => {
                let Some(params) = self.evolve_params() else { return }; // bad rule text: keep old sim
                if self.env.len() != (self.grid_dim * self.grid_dim) as usize {
                    self.env = vec![0.0; (self.grid_dim * self.grid_dim) as usize];
                }
                let init = self.seed_evolve();
                let kernel = self.kernel();
                self.seed_counter = self.seed_counter.wrapping_add(1);
                let gpu = self.gpu.as_ref().unwrap();
                let esim = EvolveSim::new(gpu, &init, &self.env, kernel, params, self.topo, self.seed_counter.wrapping_mul(0x9E3779B9));
                let renderer = Renderer::new(
                    &gpu.device, gpu.config.format,
                    RenderSource::Evolve { state: esim.front_view(), env: esim.env_view() },
                    gpu_size,
                );
                renderer.set_color_mode(&gpu.queue, self.color_mode);
                self.sim = Some(SimKind::Evolve(esim));
                self.renderer = Some(renderer);
                self.evolve_stats.clear();
                self.gens_since_stats = 0;
                self.tau_history.clear();
            }
            _ => {
                let rule = self.build_rule();
                let grid = self.seed_grid();
                let gpu = self.gpu.as_ref().unwrap();
                let sim = Sim::new(gpu, &grid, rule, self.topo);
                let renderer = Renderer::new(
                    &gpu.device, gpu.config.format,
                    RenderSource::Binary(sim.front_view()),
                    gpu_size,
                );
                self.sim = Some(SimKind::Binary(sim));
                self.renderer = Some(renderer);
            }
        }
        self.detector = CycleDetector::new(64);
        self.period = None;
        self.next_seed = SeedKind::Soup; // one-shot; Reseed goes back to soup
    }

    fn render(&mut self) {
        // advance simulation
        if self.running || self.step_once {
            let gpu = self.gpu.as_ref().unwrap();
            match self.sim.as_mut().unwrap() {
                SimKind::Binary(sim) => {
                    sim.step(gpu);
                    self.renderer.as_mut().unwrap().set_source(&gpu.device, RenderSource::Binary(sim.front_view()));
                    // per-gen readback for cycle detection, as in the browser life-torus.
                    // ponytail: sync readback every gen; throttle if it stutters at 2048².
                    // stochastic runs never cycle — skip the readback entirely there.
                    if self.sim_mode == SimMode::Deterministic {
                        let g = sim.read_back(gpu);
                        if let Some(p) = self.detector.observe(&g) {
                            self.period = Some(p);
                        }
                    }
                }
                SimKind::Evolve(esim) => {
                    esim.step(gpu);
                    self.renderer.as_mut().unwrap().set_source(
                        &gpu.device,
                        RenderSource::Evolve { state: esim.front_view(), env: esim.env_view() },
                    );
                    // throttled gene-stats readback (every 8 gens, browser-style)
                    self.gens_since_stats += 1;
                    if self.gens_since_stats >= 8 {
                        self.gens_since_stats = 0;
                        let st = esim.read_back(gpu);
                        let (mut n0, mut n1) = (0u32, 0u32);
                        let (mut t0, mut t1, mut h0, mut h1) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
                        for i in 0..st.grid.len() {
                            match st.grid[i] {
                                1 => { n0 += 1; t0 += st.tau[i] as f64; h0 += st.theta[i] as f64; }
                                2 => { n1 += 1; t1 += st.tau[i] as f64; h1 += st.theta[i] as f64; }
                                _ => {}
                            }
                        }
                        let m = |s: f64, n: u32| if n > 0 { s / n as f64 } else { 0.0 };
                        self.evolve_stats = format!(
                            "warm {n0} τ̄{:.2} θ̄{:.2} · cool {n1} τ̄{:.2} θ̄{:.2}",
                            m(t0, n0), m(h0, n0), m(t1, n1), m(h1, n1),
                        );
                        self.tau_history.push_back((m(t0, n0) as f32, m(t1, n1) as f32));
                        if self.tau_history.len() > 120 { self.tau_history.pop_front(); }
                    }
                }
            }
            self.step_once = false;
        }
        let gpu = self.gpu.as_ref().unwrap();
        let frame = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let aspect = gpu.config.width as f32 / gpu.config.height.max(1) as f32;
        self.renderer.as_ref().unwrap().draw(&gpu.device, &gpu.queue, &view, &self.cam, self.mode, aspect);

        // poll a finished stability search without blocking the frame
        if let Some(rx) = &self.search_rx {
            if let Ok(msg) = rx.try_recv() {
                self.search_result = msg;
                self.search_rx = None;
            }
        }

        // egui overlay: controls + period readout
        let mut edits = PanelEdits {
            rule_text: self.rule_text.clone(),
            sim_mode: self.sim_mode,
            sigma: self.sigma,
            floor: self.floor,
            ceil: self.ceil,
            density: self.density,
            evolve_rule_text: self.evolve_rule_text.clone(),
            env_weight: self.env_weight,
            mut_sigma: self.mut_sigma,
            seed_tau: self.seed_tau,
            kernel_weighted: self.kernel_weighted,
            tool: self.tool,
            pen_clan: self.pen_clan,
            terrain_sign: self.terrain_sign,
            grid_dim: self.grid_dim,
            topo: self.topo,
            ..Default::default()
        };
        let evolve_stats = self.evolve_stats.clone();
        let tau_history = self.tau_history.clone();
        let color_mode = self.color_mode;
        let running = self.running;
        let period = self.period;
        let searching = self.search_rx.is_some();
        let search_result = self.search_result.clone();
        let size = (gpu.config.width, gpu.config.height);
        self.egui.as_mut().unwrap().run(
            self.window.as_ref().unwrap(),
            &gpu.device,
            &gpu.queue,
            &view,
            size,
            |ctx| {
                egui::Window::new("life").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut edits.sim_mode, SimMode::Deterministic, "Torus");
                        ui.selectable_value(&mut edits.sim_mode, SimMode::Stochastic, "Stochastic");
                        ui.selectable_value(&mut edits.sim_mode, SimMode::Evolve, "Evolve");
                    });
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut edits.tool, Tool::Orbit, "🔄 Orbit");
                        ui.selectable_value(&mut edits.tool, Tool::Pen, "✏ Pen");
                        ui.selectable_value(&mut edits.tool, Tool::Glider, "🚀 Glider");
                        if edits.sim_mode == SimMode::Evolve {
                            ui.selectable_value(&mut edits.tool, Tool::Terrain, "⛰ Terrain");
                        }
                    });
                    if edits.sim_mode == SimMode::Evolve {
                        ui.horizontal(|ui| {
                            if matches!(edits.tool, Tool::Pen | Tool::Glider) {
                                ui.selectable_value(&mut edits.pen_clan, 1, "Warm");
                                ui.selectable_value(&mut edits.pen_clan, 2, "Cool");
                            }
                            if edits.tool == Tool::Terrain {
                                ui.selectable_value(&mut edits.terrain_sign, 1.0, "Fertile +");
                                ui.selectable_value(&mut edits.terrain_sign, -1.0, "Hostile −");
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("Rule");
                        if edits.sim_mode == SimMode::Evolve {
                            if ui.text_edit_singleline(&mut edits.evolve_rule_text).lost_focus() {
                                edits.evolve_rule_changed = true;
                            }
                        } else if ui.text_edit_singleline(&mut edits.rule_text).lost_focus() {
                            edits.rule_changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button(if running { "Pause" } else { "Run" }).clicked() {
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
                    ui.horizontal(|ui| {
                        ui.label("Size");
                        egui::ComboBox::from_id_salt("grid_dim")
                            .selected_text(format!("{0}×{0}", edits.grid_dim))
                            .show_ui(ui, |ui| {
                                for &n in &[64u32, 128, 256, 512, 1024, 2048] {
                                    if ui.selectable_value(&mut edits.grid_dim, n, format!("{n}×{n}")).clicked() {
                                        edits.grid_dim_changed = true;
                                    }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        let wrap_name = |w: Wrap| match w {
                            Wrap::Straight => "wrap",
                            Wrap::None => "edge",
                            Wrap::Flip => "flip",
                        };
                        for (label, axis, id) in [
                            ("X", &mut edits.topo.x, "wrap_x"),
                            ("Y", &mut edits.topo.y, "wrap_y"),
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
                    if edits.sim_mode == SimMode::Stochastic {
                        ui.separator();
                        edits.params_changed |= ui.add(egui::Slider::new(&mut edits.sigma, 0.05..=2.0).text("σ")).drag_stopped();
                        edits.params_changed |= ui.add(egui::Slider::new(&mut edits.floor, 0.0..=0.5).text("floor")).drag_stopped();
                        edits.params_changed |= ui.add(egui::Slider::new(&mut edits.ceil, 0.5..=1.0).text("ceil")).drag_stopped();
                        ui.add(egui::Slider::new(&mut edits.density, 0.0..=1.0).text("density"));
                        if ui.button("Reseed").clicked() {
                            edits.reset = true;
                        }
                        ui.separator();
                        if searching {
                            ui.label("searching…");
                        } else if ui.button("Stability search (20 trials)").clicked() {
                            edits.start_search = true;
                        }
                        if !search_result.is_empty() {
                            ui.label(&search_result);
                        }
                    } else if edits.sim_mode == SimMode::Evolve {
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
                            let before = edits.kernel_weighted;
                            ui.selectable_value(&mut edits.kernel_weighted, false, "Moore");
                            ui.selectable_value(&mut edits.kernel_weighted, true, "5×5 weighted");
                            edits.kernel_changed = before != edits.kernel_weighted;
                        });
                        edits.params_changed |= ui.add(egui::Slider::new(&mut edits.env_weight, 0.0..=8.0).text("env weight")).drag_stopped();
                        edits.params_changed |= ui.add(egui::Slider::new(&mut edits.mut_sigma, 0.0..=0.3).text("σ mut")).drag_stopped();
                        ui.add(egui::Slider::new(&mut edits.seed_tau, 0.0..=1.0).text("seed τ"));
                        ui.add(egui::Slider::new(&mut edits.density, 0.0..=1.0).text("density"));
                        ui.horizontal(|ui| {
                            if ui.button("Reseed").clicked() { edits.reset = true; }
                            if ui.button("Terrain").clicked() { edits.gen_terrain = true; }
                            if ui.button("Flat").clicked() { edits.flat_terrain = true; }
                            let label = ["◧ Clan", "◧ τ", "◧ θ", "◧ Env"][color_mode as usize];
                            if ui.button(label).clicked() { edits.cycle_color = true; }
                        });
                        if !evolve_stats.is_empty() { ui.label(&evolve_stats); }
                        if tau_history.len() > 1 {
                            // strip-chart of mean τ per clan over time, y∈[0,1]
                            ui.label("τ̄ history");
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width().min(260.0), 48.0),
                                egui::Sense::hover(),
                            );
                            let p = ui.painter_at(rect);
                            p.rect_filled(rect, 2.0, egui::Color32::from_gray(20));
                            let n = tau_history.len();
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
                                let pts: Vec<_> = tau_history.iter().enumerate()
                                    .map(|(i, s)| xy(i, if pick == 0 { s.0 } else { s.1 }))
                                    .collect();
                                p.add(egui::Shape::line(pts, egui::Stroke::new(1.5, col)));
                            }
                        }
                    } else {
                        ui.add(egui::Slider::new(&mut edits.density, 0.0..=1.0).text("density"));
                        if ui.button("Reseed").clicked() {
                            edits.reset = true;
                        }
                        ui.label(match period {
                            Some(1) => "still life".to_string(),
                            Some(p) => format!("cycle: period {p}"),
                            None => "—".to_string(),
                        });
                    }
                    ui.label("Space run · S step · V view");
                });
            },
        );
        self.rule_text = edits.rule_text.clone();
        if edits.toggle_run { self.running = !self.running; }
        if edits.step { self.step_once = true; }
        let mode_changed = edits.sim_mode != self.sim_mode;
        self.sim_mode = edits.sim_mode;
        self.sigma = edits.sigma;
        self.floor = edits.floor;
        self.ceil = edits.ceil;
        self.density = edits.density;
        self.env_weight = edits.env_weight;
        self.mut_sigma = edits.mut_sigma;
        self.seed_tau = edits.seed_tau;
        self.pen_clan = edits.pen_clan;
        self.terrain_sign = edits.terrain_sign;
        self.tool = if mode_changed && edits.tool == Tool::Terrain && edits.sim_mode != SimMode::Evolve {
            Tool::Orbit // terrain pen only exists in evolve mode
        } else {
            edits.tool
        };
        if edits.kernel_changed {
            self.kernel_weighted = edits.kernel_weighted;
            // swap in the kernel family's default rule
            self.evolve_rule_text =
                if self.kernel_weighted { "B14-18/S12-24".into() } else { "B3/S23".into() };
        }
        self.evolve_rule_text = if edits.evolve_rule_changed { edits.evolve_rule_text.clone() } else { self.evolve_rule_text.clone() };
        if edits.gen_terrain || edits.flat_terrain {
            let n = (self.grid_dim * self.grid_dim) as usize;
            if edits.flat_terrain {
                self.env = vec![0.0; n];
            } else {
                self.seed_counter = self.seed_counter.wrapping_add(1);
                let mut s = 0x7E44_u64 ^ ((self.seed_counter as u64) << 13);
                let mut rng = move || xorshift(&mut s);
                self.env = generate_terrain(self.grid_dim, self.grid_dim, &mut rng);
            }
            if let (Some(SimKind::Evolve(esim)), Some(gpu)) = (self.sim.as_ref(), self.gpu.as_ref()) {
                esim.upload_env(gpu, &self.env);
            }
        }
        if edits.cycle_color {
            self.color_mode = (self.color_mode + 1) % 4;
            if let (Some(r), Some(gpu)) = (self.renderer.as_ref(), self.gpu.as_ref()) {
                r.set_color_mode(&gpu.queue, self.color_mode);
            }
        }
        if edits.topo_changed {
            self.topo = edits.topo;
        }
        if edits.grid_dim_changed {
            self.grid_dim = edits.grid_dim;
            self.env = Vec::new(); // force realloc at new size in rebuild_sim
            self.rebuild_sim();
        } else if edits.topo_changed {
            self.rebuild_sim();
        } else if let Some(i) = edits.preset {
            // preset: kernel + rule (+ single-clan hatch patch for LtL creatures)
            let (_, weighted, rule, hatch) = PRESETS[i];
            self.kernel_weighted = weighted;
            self.evolve_rule_text = rule.into();
            self.next_seed = if hatch { SeedKind::Hatch } else { SeedKind::Soup };
            self.rebuild_sim();
        } else if edits.clear {
            self.next_seed = SeedKind::Empty;
            self.running = false; // blank canvas is for drawing — pause
            self.rebuild_sim();
        } else if edits.rule_changed {
            if let Ok(bs) = parse_bs(&edits.rule_text) {
                self.rule = bs;
                self.rebuild_sim();
            }
        } else if edits.evolve_rule_changed || edits.kernel_changed || edits.reset || mode_changed
            || (edits.params_changed && self.sim_mode != SimMode::Evolve)
        {
            self.rebuild_sim();
        } else if edits.params_changed {
            // evolve env_weight / σ_mut are live-tunable without reseeding
            if let Some(SimKind::Evolve(esim)) = self.sim.as_mut() {
                if let Some(p) = evolve_params_of(&self.evolve_rule_text, self.kernel_weighted, self.env_weight, self.mut_sigma) {
                    esim.set_params(p);
                }
            }
        }
        if edits.start_search {
            self.start_search();
        }
        frame.present();
    }

    /// Apply the active tool at a cursor position (physical px).
    fn apply_tool(&mut self, cursor: (f64, f64)) {
        let Some(gpu) = self.gpu.as_ref() else { return };
        let (sw, sh) = (gpu.config.width as f64, gpu.config.height as f64);
        let ndc = (
            (2.0 * cursor.0 / sw - 1.0) as f32,
            (1.0 - 2.0 * cursor.1 / sh) as f32,
        );
        let aspect = (sw / sh.max(1.0)) as f32;
        let dims = (self.grid_dim, self.grid_dim);
        let Some((cx, cy)) = pick_cell(&self.cam, aspect, ndc, self.mode, dims) else { return };
        match self.tool {
            Tool::Orbit => {}
            Tool::Pen => match self.sim.as_ref() {
                Some(SimKind::Binary(sim)) => sim.write_cell(gpu, cx, cy, 1),
                Some(SimKind::Evolve(esim)) => esim.write_cell(gpu, cx, cy, self.pen_clan, self.seed_tau, 0.5),
                None => {}
            },
            Tool::Glider => {
                // standard glider, cells at the seed genes (browser stampGliderAt)
                const GLIDER: [(u32, u32); 5] = [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)];
                for (dc, dr) in GLIDER {
                    let (x, y) = ((cx + dc) % dims.0, (cy + dr) % dims.1);
                    match self.sim.as_ref() {
                        Some(SimKind::Binary(sim)) => sim.write_cell(gpu, x, y, 1),
                        Some(SimKind::Evolve(esim)) => esim.write_cell(gpu, x, y, self.pen_clan, self.seed_tau, 0.5),
                        None => {}
                    }
                }
            }
            Tool::Terrain => {
                // soft radial bump of the chosen sign, accumulate + clamp (browser terrainAt)
                let n = (dims.0 * dims.1) as usize;
                if self.env.len() != n { self.env = vec![0.0; n]; }
                const R: i32 = 5;
                let two_sig2 = 2.0 * 2.5f32 * 2.5;
                for dr in -R..=R {
                    for dc in -R..=R {
                        let Some((x, y)) = life_core::resolve(cx as i32 + dc, cy as i32 + dr, dims.0, dims.1, self.topo) else { continue };
                        let i = (y * dims.0 + x) as usize;
                        let bump = self.terrain_sign * 0.35 * (-((dc * dc + dr * dr) as f32) / two_sig2).exp();
                        self.env[i] = (self.env[i] + bump).clamp(-1.0, 1.0);
                    }
                }
                if let Some(SimKind::Evolve(esim)) = self.sim.as_ref() {
                    esim.upload_env(gpu, &self.env);
                }
            }
        }
        // hand-edited state invalidates any cycle history
        self.detector = CycleDetector::new(64);
        self.period = None;
    }

    fn start_search(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.search_rx = Some(rx);
        let opts = TrialOpts {
            w: 100, h: 100,
            topo: self.topo,
            gens: 200,
            thresholds: Thresholds::DEFAULT,
            birth: self.rule.birth,
            survive: self.rule.survive,
            b_parm: self.curve_params(),
            s_parm: self.curve_params(),
            density: self.density as f64,
        };
        let seed = self.seed_counter as u64 ^ 0xDEAD_BEEF;
        std::thread::spawn(move || {
            let mut s = seed | 1;
            let mut rng = move || xorshift(&mut s);
            let mut counts = [0usize; 4]; // dead, full, stable, chaotic
            let (mut mean_sum, mut std_sum) = (0.0, 0.0);
            const TRIALS: usize = 20;
            for _ in 0..TRIALS {
                let r = run_trial(&opts, &mut rng);
                mean_sum += r.mean;
                std_sum += r.std_dev;
                counts[match r.status {
                    Verdict::Dead => 0, Verdict::Full => 1, Verdict::Stable => 2, Verdict::Chaotic => 3,
                }] += 1;
            }
            let n = TRIALS as f64;
            let _ = tx.send(format!(
                "dead {} · full {} · stable {} · chaotic {}\nmean {:.3} σ {:.3}",
                counts[0], counts[1], counts[2], counts[3], mean_sum / n, std_sum / n,
            ));
        });
    }
}

#[derive(Default)]
struct PanelEdits {
    rule_text: String,
    rule_changed: bool,
    toggle_run: bool,
    step: bool,
    reset: bool,
    sim_mode: SimMode,
    sigma: f32,
    floor: f32,
    ceil: f32,
    density: f32,
    params_changed: bool,
    start_search: bool,
    grid_dim: u32,
    grid_dim_changed: bool,
    topo: Topology,
    topo_changed: bool,
    // tools
    tool: Tool,
    pen_clan: u8,
    terrain_sign: f32,
    // evolve
    evolve_rule_text: String,
    evolve_rule_changed: bool,
    env_weight: f32,
    mut_sigma: f32,
    seed_tau: f32,
    kernel_weighted: bool,
    kernel_changed: bool,
    gen_terrain: bool,
    flat_terrain: bool,
    cycle_color: bool,
    clear: bool,
    preset: Option<usize>,
}

impl Default for SimMode {
    fn default() -> Self { SimMode::Deterministic }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("life — torus"))
                .unwrap(),
        );
        let gpu = GpuContext::new(window.clone());
        // boot self-test: GPU step must equal CPU reference, or abort.
        match gpu_self_test(&gpu) {
            Ok(()) => println!("gpu_self_test: OK (deterministic, stochastic@limit, evolve@σ0)"),
            Err(e) => panic!("GPU self-test failed: {e}"),
        }
        self.egui = Some(EguiLayer::new(&gpu.device, gpu.config.format, &window));
        self.gpu = Some(gpu);
        self.window = Some(window);
        self.rebuild_sim(); // builds sim + renderer for the current mode
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // let egui consume input first
        if let (Some(egui_layer), Some(window)) = (self.egui.as_mut(), self.window.as_ref()) {
            if egui_layer.on_event(window, &event) {
                window.request_redraw();
                return;
            }
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                let gpu = self.gpu.as_mut().unwrap();
                gpu.resize(size.width, size.height);
                self.renderer.as_mut().unwrap().resize(&gpu.device, (size.width, size.height));
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let pressed = state == ElementState::Pressed;
                if self.tool == Tool::Orbit {
                    self.dragging = pressed;
                } else {
                    self.painting = pressed;
                    if pressed {
                        self.apply_tool(self.last_cursor);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (dx, dy) = (position.x - self.last_cursor.0, position.y - self.last_cursor.1);
                self.last_cursor = (position.x, position.y);
                if self.dragging {
                    self.cam.orbit(dx as f32, dy as f32);
                }
                // pen/terrain drag-paint; gliders stamp on click only
                if self.painting && matches!(self.tool, Tool::Pen | Tool::Terrain) {
                    self.apply_tool(self.last_cursor);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.01,
                };
                self.cam.zoom(d);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Space) => self.running = !self.running,
                    PhysicalKey::Code(KeyCode::KeyS) => self.step_once = true,
                    PhysicalKey::Code(KeyCode::KeyV) => {
                        self.mode = if self.mode == ViewMode::Torus { ViewMode::Flat } else { ViewMode::Torus };
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }
}

// --- egui integration shim over egui-winit + egui-wgpu ---
struct EguiLayer {
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl EguiLayer {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, window: &Window) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(ctx, egui::ViewportId::ROOT, window, None, None, None);
        let renderer = egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions::default());
        EguiLayer { state, renderer }
    }

    fn on_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }

    fn run(
        &mut self,
        window: &Window,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        size: (u32, u32),
        mut build: impl FnMut(&egui::Context),
    ) {
        let input = self.state.take_egui_input(window);
        let ctx = self.state.egui_ctx().clone();
        let output = ctx.run_ui(input, |ui| build(ui.ctx()));
        self.state.handle_platform_output(window, output.platform_output);
        let tris = ctx.tessellate(output.shapes, output.pixels_per_point);
        let sd = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [size.0, size.1],
            pixels_per_point: output.pixels_per_point,
        };
        for (id, delta) in &output.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
        }
        let mut enc = device.create_command_encoder(&Default::default());
        self.renderer.update_buffers(device, queue, &mut enc, &tris, &sd);
        {
            let mut pass = enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &tris, &sd);
        }
        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
        queue.submit(Some(enc.finish()));
    }
}
