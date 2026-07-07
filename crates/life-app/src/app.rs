use std::sync::Arc;
use life_core::{parse_bs, CycleDetector, Grid, Topology, Wrap, BS};
use life_gpu::{gpu_self_test, GpuContext, Sim};
use life_render::{OrbitCamera, Renderer, ViewMode};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

pub struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    sim: Option<Sim>,
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
    // input
    dragging: bool,
    last_cursor: (f64, f64),
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
            rule: parse_bs("B3/S23").unwrap(),
            rule_text: "B3/S23".into(),
            topo: Topology { x: Wrap::Straight, y: Wrap::Straight },
            grid_dim: 128,
            detector: CycleDetector::new(64),
            period: None,
            dragging: false,
            last_cursor: (0.0, 0.0),
        }
    }
}

impl App {
    fn seed_grid(dim: u32) -> Grid {
        // centre a blinker so the first frame visibly animates
        let mut g = Grid::new(dim, dim);
        let c = dim / 2;
        g.set(c, c - 1, 1);
        g.set(c, c, 1);
        g.set(c, c + 1, 1);
        g
    }

    fn rebuild_sim(&mut self) {
        let gpu = self.gpu.as_ref().unwrap();
        let grid = Self::seed_grid(self.grid_dim);
        let sim = Sim::new(gpu, &grid, self.rule, self.topo);
        if let Some(r) = self.renderer.as_mut() {
            r.set_sim_view(&gpu.device, sim.front_view());
        }
        self.sim = Some(sim);
        self.detector = CycleDetector::new(64);
        self.period = None;
    }

    fn render(&mut self) {
        // advance simulation
        if self.running || self.step_once {
            let gpu = self.gpu.as_ref().unwrap();
            let sim = self.sim.as_mut().unwrap();
            sim.step(gpu);
            self.renderer.as_mut().unwrap().set_sim_view(&gpu.device, sim.front_view());
            // per-gen readback for cycle detection, as in the browser life-torus.
            // ponytail: sync readback every gen; throttle if it stutters at 2048².
            let g = sim.read_back(gpu);
            if let Some(p) = self.detector.observe(&g) {
                self.period = Some(p);
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

        // egui overlay: controls + period readout
        let mut edits = PanelEdits { rule_text: self.rule_text.clone(), ..Default::default() };
        let running = self.running;
        let period = self.period;
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
                        ui.label("Rule");
                        if ui.text_edit_singleline(&mut edits.rule_text).lost_focus() {
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
                    });
                    ui.label(match period {
                        Some(1) => "still life".to_string(),
                        Some(p) => format!("cycle: period {p}"),
                        None => "—".to_string(),
                    });
                    ui.label("Space run · S step · V view");
                });
            },
        );
        self.rule_text = edits.rule_text.clone();
        if edits.toggle_run { self.running = !self.running; }
        if edits.step { self.step_once = true; }
        if edits.reset { self.rebuild_sim(); }
        if edits.rule_changed {
            if let Ok(bs) = parse_bs(&edits.rule_text) {
                self.rule = bs;
                self.rebuild_sim();
            }
        }
        frame.present();
    }
}

#[derive(Default)]
struct PanelEdits {
    rule_text: String,
    rule_changed: bool,
    toggle_run: bool,
    step: bool,
    reset: bool,
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
            Ok(()) => println!("gpu_self_test: OK"),
            Err(e) => panic!("GPU self-test failed: {e}"),
        }
        let grid = Self::seed_grid(self.grid_dim);
        let sim = Sim::new(&gpu, &grid, self.rule, self.topo);
        let size = (gpu.config.width, gpu.config.height);
        let renderer = Renderer::new(&gpu.device, gpu.config.format, sim.front_view(), size);
        self.egui = Some(EguiLayer::new(&gpu.device, gpu.config.format, &window));
        self.sim = Some(sim);
        self.renderer = Some(renderer);
        self.gpu = Some(gpu);
        self.window = Some(window);
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
                self.dragging = state == ElementState::Pressed;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (dx, dy) = (position.x - self.last_cursor.0, position.y - self.last_cursor.1);
                self.last_cursor = (position.x, position.y);
                if self.dragging {
                    self.cam.orbit(dx as f32, dy as f32);
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
