use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;
use crate::panel::{self, PanelEdits, PanelView, SimMode, Tool, Tunables, PRESETS};
use life_core::{
    curve, generate_terrain, parse_bs, parse_evolve_rule, run_trial, CurveParams, CycleDetector,
    EvolveParams, EvolveState, Grid, Kernel, Thresholds, TrialOpts, Verdict, BS,
};
use life_core::ecology::EcologyParams;
use life_gpu::{gpu_self_test, EcologySim, EvolveSim, GpuContext, Sim, SimRule};
use life_render::{pick_cell, OrbitCamera, RenderSource, Renderer, ViewMode};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

enum SimKind {
    Binary(Sim),
    Evolve(EvolveSim),
    /// Stepped on the CPU; see `life_gpu::ecology_sim` for why it cannot be otherwise.
    Ecology(EcologySim),
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
    /// Everything the control panel edits. Defaults live in `Tunables`.
    t: Tunables,
    step_accum: f32, // fractional gens owed, carried between frames
    last_frame: Option<Instant>,
    rule: BS,
    detector: CycleDetector,
    period: Option<u64>,
    seed_counter: u32,
    // stability search (background thread)
    search_rx: Option<mpsc::Receiver<String>>,
    search_result: String,
    // evolve mode
    env: Vec<f32>,
    color_mode: u32, // 0 clan, 1 τ, 2 θ, 3 env
    evolve_stats: String,
    gens_since_stats: u32,
    tau_history: VecDeque<(f32, f32)>, // (warm τ̄, cool τ̄), newest at back
    ecology_counts: String,
    next_seed: SeedKind,
    // input
    dragging: bool,
    painting: bool,
    last_cursor: (f64, f64),
}

/// How the next rebuild seeds the grid; consumed (reset to Soup) after use.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SeedKind {
    #[default]
    Soup,
    Hatch, // compact single-clan patch (LtL creatures die in mixed soup)
    Empty, // blank canvas for pen/glider drawing
}

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
            t: Tunables::default(),
            step_accum: 0.0,
            last_frame: None,
            rule: parse_bs("B3/S23").unwrap(),
            detector: CycleDetector::new(64),
            period: None,
            seed_counter: 1,
            search_rx: None,
            search_result: String::new(),
            env: Vec::new(),
            color_mode: 0,
            evolve_stats: String::new(),
            gens_since_stats: 0,
            tau_history: VecDeque::new(),
            ecology_counts: String::new(),
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
        CurveParams { sigma: self.t.stochastic.sigma as f64, floor: self.t.stochastic.floor as f64, ceil: self.t.stochastic.ceil as f64 }
    }

    fn build_rule(&mut self) -> SimRule {
        match self.t.sim_mode {
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
        let mut g = Grid::new(self.t.grid_w, self.t.grid_h);
        if self.next_seed == SeedKind::Empty {
            return g;
        }
        self.seed_counter = self.seed_counter.wrapping_add(1);
        let mut s = 0x5EED_u64 ^ ((self.seed_counter as u64) << 17);
        for c in g.cells.iter_mut() {
            *c = (xorshift(&mut s) < self.t.density as f64) as u8;
        }
        g
    }

    /// The rule's parameters, rebuilt from the panel each time they are needed.
    /// `gen` is owned by the sim, which advances it, so it is left at its default
    /// here and preserved by `EcologySim::set_params`.
    fn ecology_params(&self) -> EcologyParams {
        let e = &self.t.ecology;
        EcologyParams {
            beta_red: e.beta_red,
            beta_grey: e.beta_grey,
            sigma: e.sigma,
            delta: e.delta,
            g: e.g,
            e0: e.e0,
            e_breed: e.e_breed,
            breed_cost: e.breed_cost,
            e_cap: e.e_cap,
            mu: e.mu,
            ..EcologyParams::default()
        }
    }

    fn kernel(&self) -> Kernel {
        if self.t.evolve.kernel_weighted { Kernel::weighted5x5() } else { Kernel::moore() }
    }

    fn evolve_params(&self) -> Option<EvolveParams> {
        let rule = parse_evolve_rule(&self.t.evolve.rule_text, self.t.evolve.kernel_weighted)?;
        Some(EvolveParams { rule, env_weight: self.t.evolve.env_weight, mut_sigma: self.t.evolve.mut_sigma })
    }

    fn seed_evolve(&mut self) -> EvolveState {
        let (w, h) = (self.t.grid_w, self.t.grid_h);
        let mut st = EvolveState::new(w, h);
        self.seed_counter = self.seed_counter.wrapping_add(1);
        let mut s = 0x50D4_u64 ^ ((self.seed_counter as u64) << 21);
        match self.next_seed {
            SeedKind::Empty => {}
            SeedKind::Hatch => {
                // compact single-clan patch at centre: LtL creatures grow from
                // a lone clan (the rival would kill them in a mixed soup)
                let (cx, cy) = (w / 2, h / 2);
                let r = 16u32.min(w.min(h) / 2 - 2);
                for y in (cy - r)..(cy + r) {
                    for x in (cx - r)..(cx + r) {
                        if xorshift(&mut s) < 0.45 {
                            let i = st.idx(x, y);
                            st.grid[i] = self.t.evolve.pen_clan;
                            st.tau[i] = self.t.evolve.seed_tau;
                            st.theta[i] = 0.5;
                        }
                    }
                }
            }
            SeedKind::Soup => {
                // two-clan soup at the chosen density, genes at seed values
                for i in 0..(w * h) as usize {
                    if xorshift(&mut s) < self.t.density as f64 {
                        st.grid[i] = if xorshift(&mut s) < 0.5 { 1 } else { 2 };
                        st.tau[i] = self.t.evolve.seed_tau;
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
        match self.t.sim_mode {
            SimMode::Evolve => {
                let Some(params) = self.evolve_params() else { return }; // bad rule text: keep old sim
                if self.env.len() != (self.t.grid_w * self.t.grid_h) as usize {
                    self.env = vec![0.0; (self.t.grid_w * self.t.grid_h) as usize];
                }
                let init = self.seed_evolve();
                let kernel = self.kernel();
                self.seed_counter = self.seed_counter.wrapping_add(1);
                let gpu = self.gpu.as_ref().unwrap();
                let esim = EvolveSim::new(gpu, &init, &self.env, kernel, params, self.t.topo, self.seed_counter.wrapping_mul(0x9E3779B9));
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
            SimMode::Ecology => {
                // no seed_counter bump: the panel's seed is the whole point, so a
                // rebuild with the same seed must reproduce the same run exactly
                let gpu = self.gpu.as_ref().unwrap();
                let esim = EcologySim::new(
                    gpu,
                    self.t.grid_w,
                    self.t.grid_h,
                    self.t.density as f64,
                    self.t.ecology.marten_frac,
                    self.ecology_params(),
                    self.t.topo,
                    self.t.ecology.seed,
                );
                let renderer = Renderer::new(
                    &gpu.device, gpu.config.format,
                    RenderSource::Ecology(esim.front_view()),
                    gpu_size,
                );
                renderer.set_color_mode(&gpu.queue, self.color_mode.min(1));
                self.sim = Some(SimKind::Ecology(esim));
                self.renderer = Some(renderer);
                self.ecology_counts.clear();
            }
            _ => {
                let rule = self.build_rule();
                let grid = self.seed_grid();
                let gpu = self.gpu.as_ref().unwrap();
                let sim = Sim::new(gpu, &grid, rule, self.t.topo);
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

    /// Steps to run this frame: manual Step = exactly 1; running = however many
    /// the real-time accumulator owes at `target_gps`, capped so a stall can't
    /// spiral into a catch-up burst; paused = 0.
    fn steps_this_frame(&mut self) -> u32 {
        if self.step_once {
            self.step_once = false;
            self.last_frame = None; // manual step: don't credit stalled wall-time
            return 1;
        }
        if !self.running {
            self.last_frame = None;
            self.step_accum = 0.0;
            return 0;
        }
        let now = Instant::now();
        let dt = self.last_frame.replace(now).map_or(0.0, |t| (now - t).as_secs_f32());
        self.step_accum += dt * self.t.target_gps;
        let n = self.step_accum.floor().min(16.0);
        self.step_accum -= n;
        n as u32
    }

    /// Advance the sim one generation and refresh the render source + readbacks.
    fn advance_once(&mut self) {
        {
            let gpu = self.gpu.as_ref().unwrap();
            match self.sim.as_mut().unwrap() {
                SimKind::Binary(sim) => {
                    sim.step(gpu);
                    self.renderer.as_mut().unwrap().set_source(&gpu.device, RenderSource::Binary(sim.front_view()));
                    // per-gen readback for cycle detection, as in the browser life-torus.
                    // ponytail: sync readback every gen; throttle if it stutters at 2048².
                    // stochastic runs never cycle — skip the readback entirely there.
                    if self.t.sim_mode == SimMode::Deterministic {
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
                SimKind::Ecology(esim) => {
                    // The step is CPU-side and uploads its own texture, so the bind
                    // group still points at the right view — no set_source needed.
                    esim.step(gpu);
                    let (red, grey, marten) = esim.counts();
                    self.ecology_counts = format!("red {red} · grey {grey} · marten {marten}");
                }
            }
        }
    }

    fn render(&mut self) {
        // advance simulation — real-time throttle to target_gps
        for _ in 0..self.steps_this_frame() {
            self.advance_once();
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

        // egui overlay: controls + period readout. The view is cloned because the
        // egui layer is mutably borrowed from `self` for the duration of the panel.
        let mut edits = PanelEdits::from(&self.t);
        let panel_view = PanelView {
            running: self.running,
            period: self.period,
            searching: self.search_rx.is_some(),
            search_result: self.search_result.clone(),
            color_mode: self.color_mode,
            evolve_stats: self.evolve_stats.clone(),
            tau_history: self.tau_history.clone(),
            ecology_counts: self.ecology_counts.clone(),
        };
        let size = (gpu.config.width, gpu.config.height);
        self.egui.as_mut().unwrap().run(
            self.window.as_ref().unwrap(),
            &gpu.device,
            &gpu.queue,
            &view,
            size,
            |ctx| panel::draw(ctx, &panel_view, &mut edits),
        );
        // Copy the edited state back wholesale. Only the tool needs a rule: the
        // terrain pen exists in evolve mode alone.
        let mode_changed = edits.t.sim_mode != self.t.sim_mode;
        let tool = if mode_changed && edits.t.tool == Tool::Terrain && edits.t.sim_mode != SimMode::Evolve {
            Tool::Orbit
        } else {
            edits.t.tool
        };
        // The env buffer below must be built at the size the *live* sim was created
        // with, not the size just typed into the panel — it is uploaded straight into
        // that sim's texture, and a resize only takes effect at the rebuild further down.
        let (live_w, live_h) = (self.t.grid_w, self.t.grid_h);
        self.t = edits.t.clone();
        self.t.tool = tool;
        if edits.toggle_run { self.running = !self.running; }
        if edits.step { self.step_once = true; }
        if edits.kernel_changed {
            // swap in the kernel family's default rule
            self.t.evolve.rule_text =
                if self.t.evolve.kernel_weighted { "B14-18/S12-24".into() } else { "B3/S23".into() };
        }
        if edits.gen_terrain || edits.flat_terrain {
            let n = (live_w * live_h) as usize;
            if edits.flat_terrain {
                self.env = vec![0.0; n];
            } else {
                self.seed_counter = self.seed_counter.wrapping_add(1);
                let mut s = 0x7E44_u64 ^ ((self.seed_counter as u64) << 13);
                let mut rng = move || xorshift(&mut s);
                self.env = generate_terrain(live_w, live_h, &mut rng);
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
        // hard cap regardless of how the value arrived: GPU textures max 2048/axis
        let max_axis = panel::max_axis(self.t.sim_mode);
        self.t.grid_w = self.t.grid_w.clamp(16, max_axis);
        self.t.grid_h = self.t.grid_h.clamp(16, max_axis);
        if edits.grid_dim_changed {
            self.env = Vec::new(); // force realloc at new size in rebuild_sim
            self.rebuild_sim();
        } else if edits.topo_changed {
            self.rebuild_sim();
        } else if let Some(i) = edits.preset {
            // preset: kernel + rule (+ single-clan hatch patch for LtL creatures)
            let (_, weighted, rule, hatch) = PRESETS[i];
            self.t.evolve.kernel_weighted = weighted;
            self.t.evolve.rule_text = rule.into();
            self.next_seed = if hatch { SeedKind::Hatch } else { SeedKind::Soup };
            self.rebuild_sim();
        } else if edits.clear {
            self.next_seed = SeedKind::Empty;
            self.running = false; // blank canvas is for drawing — pause
            self.rebuild_sim();
        } else if edits.rule_changed {
            if let Ok(bs) = parse_bs(&edits.t.rule_text) {
                self.rule = bs;
                self.rebuild_sim();
            }
        } else if edits.evolve_rule_changed || edits.kernel_changed || edits.reset || mode_changed
            || (edits.params_changed
                && !matches!(self.t.sim_mode, SimMode::Evolve | SimMode::Ecology))
        {
            self.rebuild_sim();
        } else if edits.params_changed {
            // evolve env_weight / σ_mut are live-tunable without reseeding, and so is
            // every ecology parameter: the rule reads them afresh each step, so a
            // slider moved mid-run changes the ecology rather than restarting it
            let ecology = self.ecology_params();
            match self.sim.as_mut() {
                Some(SimKind::Evolve(esim)) => {
                    if let Some(p) = evolve_params_of(&self.t.evolve.rule_text, self.t.evolve.kernel_weighted, self.t.evolve.env_weight, self.t.evolve.mut_sigma) {
                        esim.set_params(p);
                    }
                }
                Some(SimKind::Ecology(esim)) => esim.set_params(ecology),
                _ => {}
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
        let dims = (self.t.grid_w, self.t.grid_h);
        let Some((cx, cy)) = pick_cell(&self.cam, aspect, ndc, self.mode, dims) else { return };
        match self.t.tool {
            Tool::Orbit => {}
            Tool::Pen => match self.sim.as_mut() {
                Some(SimKind::Binary(sim)) => sim.write_cell(gpu, cx, cy, 1),
                Some(SimKind::Evolve(esim)) => esim.write_cell(gpu, cx, cy, self.t.evolve.pen_clan, self.t.evolve.seed_tau, 0.5),
                // ecology has three species to choose from and no picker yet, so the
                // pen drops a red squirrel; the species selector is the next slice
                Some(SimKind::Ecology(esim)) => esim.write_cell(gpu, cx, cy, 1),
                None => {}
            },
            Tool::Glider => {
                // standard glider, cells at the seed genes (browser stampGliderAt)
                const GLIDER: [(u32, u32); 5] = [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)];
                for (dc, dr) in GLIDER {
                    let (x, y) = ((cx + dc) % dims.0, (cy + dr) % dims.1);
                    match self.sim.as_mut() {
                        Some(SimKind::Binary(sim)) => sim.write_cell(gpu, x, y, 1),
                        Some(SimKind::Evolve(esim)) => esim.write_cell(gpu, x, y, self.t.evolve.pen_clan, self.t.evolve.seed_tau, 0.5),
                        // a glider is a Life pattern; it means nothing to a contact process
                        Some(SimKind::Ecology(_)) => {}
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
                        let Some((x, y)) = life_core::resolve(cx as i32 + dc, cy as i32 + dr, dims.0, dims.1, self.t.topo) else { continue };
                        let i = (y * dims.0 + x) as usize;
                        let bump = self.t.evolve.terrain_sign * 0.35 * (-((dc * dc + dr * dr) as f32) / two_sig2).exp();
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
            topo: self.t.topo,
            gens: 200,
            thresholds: Thresholds::DEFAULT,
            birth: self.rule.birth,
            survive: self.rule.survive,
            b_parm: self.curve_params(),
            s_parm: self.curve_params(),
            density: self.t.density as f64,
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
                if self.t.tool == Tool::Orbit {
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
                if self.painting && matches!(self.t.tool, Tool::Pen | Tool::Terrain) {
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
