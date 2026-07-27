//! The ecology simulation, stepped on the CPU and uploaded for the renderer.
//!
//! Unlike its siblings in this crate there is no compute shader here, and there
//! deliberately cannot be one: `life_core::ecology` consumes its RNG as a single
//! sequential stream in scan order, which is what makes a seed replay exactly and
//! what a parallel port cannot preserve. See the reproducibility invariant in
//! CLAUDE.md. What this type owns is the GPU side — a texture in the layout the
//! renderer expects — so `app.rs` can treat it like `Sim` and `EvolveSim`.

use crate::context::GpuContext;
use life_core::ecology::{step_ecology, EcologyParams, EcologyState};
use life_core::Topology;

/// Grid ceiling for this mode, far below the GPU modes' 2048 per axis because the
/// step is CPU-bound. Measured cost of one `step_ecology`:
///
/// ```text
///              release      debug
///   128x128    759 gen/s    133 gen/s
///   256x256    176           32
///   512x256     97           17
///   512x512     46            8
///  1024x512     24            4
/// ```
///
/// The app's target rate runs 0.5–120 gen/s and defaults to 12. 512 per axis is the
/// largest power of two that still clears that default in a release build at its
/// worst case (512x512, 46 gen/s), and it is already 80x the browser's 80x40. A
/// debug build is roughly 5.7x slower; the step accumulator absorbs that by falling
/// behind the requested rate rather than spiralling into a catch-up burst.
pub const MAX_AXIS: u32 = 512;

pub struct EcologySim {
    state: EcologyState,
    params: EcologyParams,
    topo: Topology,
    /// mulberry32 state. One stream, advanced only by `step`, so a given seed
    /// replays the same run — the same contract the browser version holds to.
    rng_state: u32,
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    packed: Vec<f32>, // reused between frames; one upload per step
    generation: u64,
    w: u32,
    h: u32,
}

/// The generator `life-ecology.html` and the core tests both use. f64 because JS
/// has no f32: rounding here moves every `rng() < p` comparison off the reference.
fn mulberry32(state: &mut u32) -> f64 {
    *state = state.wrapping_add(0x6D2B79F5);
    let mut t = (*state ^ (*state >> 15)).wrapping_mul(1 | *state);
    t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
    ((t ^ (t >> 14)) as f64) / 4294967296.0
}

/// One RGBA32F texel per cell: species, marten energy, infection, terrain.
/// Only the first two are read by the shader today; the others are carried so
/// squirrelpox and refuges arrive without a format change.
pub fn pack(state: &EcologyState) -> Vec<f32> {
    let n = (state.cols * state.rows) as usize;
    let mut out = vec![0.0f32; n * 4];
    for i in 0..n {
        out[i * 4] = state.grid[i] as f32;
        out[i * 4 + 1] = state.energy[i];
        out[i * 4 + 2] = state.infected[i] as f32;
        out[i * 4 + 3] = state.env.as_ref().map_or(0.0, |e| e[i]);
    }
    out
}

impl EcologySim {
    /// Seed a board and upload it. The seeding draws from the same stream that
    /// will then drive the steps, exactly as the browser's `randomize()` does —
    /// which is what makes a seed reproduce a whole run and not merely a board.
    ///
    /// Squirrels are placed at `density`, split evenly red/grey, as in the browser.
    /// Martens differ: the page introduces them with a release button, which is out
    /// of scope here, so a small fraction is seeded directly. Without any there is
    /// no predation to watch and the energy ledger never moves.
    pub fn new(
        ctx: &GpuContext,
        w: u32,
        h: u32,
        density: f64,
        marten_frac: f64,
        params: EcologyParams,
        topo: Topology,
        seed: u32,
    ) -> EcologySim {
        let mut rng_state = seed;
        let mut init = EcologyState::new(w, h);
        for i in 0..(w * h) as usize {
            if mulberry32(&mut rng_state) < density {
                init.grid[i] = if mulberry32(&mut rng_state) < 0.5 { 1 } else { 2 };
            } else if mulberry32(&mut rng_state) < marten_frac {
                init.grid[i] = 3;
                init.energy[i] = params.e0 as f32;
            }
        }
        EcologySim::from_state(ctx, init, params, topo, rng_state)
    }

    /// Build from an existing board, taking the RNG stream wherever it has reached.
    pub fn from_state(ctx: &GpuContext, init: EcologyState, params: EcologyParams, topo: Topology, rng_state: u32) -> EcologySim {
        let (w, h) = (init.cols, init.rows);
        let tex = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ecology state"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            // no STORAGE_BINDING: nothing on the GPU writes this
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let mut sim = EcologySim {
            state: init,
            params,
            topo,
            rng_state,
            tex,
            view,
            packed: Vec::new(),
            generation: 0,
            w,
            h,
        };
        sim.upload(ctx);
        sim
    }

    fn upload(&mut self, ctx: &GpuContext) {
        self.packed = pack(&self.state);
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&self.packed),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.w * 16),
                rows_per_image: Some(self.h),
            },
            wgpu::Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 },
        );
    }

    pub fn step(&mut self, ctx: &GpuContext) {
        self.params.gen = self.generation;
        // the closure borrows a local copy so the stream position survives the call
        let mut s = self.rng_state;
        let next = {
            let mut rng = || mulberry32(&mut s);
            step_ecology(&self.state, self.topo, &self.params, &mut rng)
        };
        self.rng_state = s;
        self.state = next;
        self.generation += 1;
        self.upload(ctx);
    }

    /// Live population counts, for the panel. Cheap: the state is already on the CPU,
    /// which is the one thing this sim gets for free that its GPU siblings do not.
    pub fn counts(&self) -> (u32, u32, u32) {
        let (mut red, mut grey, mut marten) = (0, 0, 0);
        for &v in &self.state.grid {
            match v {
                1 => red += 1,
                2 => grey += 1,
                3 => marten += 1,
                _ => {}
            }
        }
        (red, grey, marten)
    }

    pub fn set_params(&mut self, params: EcologyParams) {
        let gen = self.params.gen;
        self.params = params;
        self.params.gen = gen;
    }

    pub fn generation(&self) -> u64 { self.generation }
    pub fn state(&self) -> &EcologyState { &self.state }
    pub fn front_view(&self) -> &wgpu::TextureView { &self.view }

    /// Paint a single cell, for the pen tool. Uploads the whole texture again;
    /// at the sizes this simulation runs at that is cheaper than a partial write.
    pub fn write_cell(&mut self, ctx: &GpuContext, x: u32, y: u32, v: u8) {
        if x >= self.w || y >= self.h { return; }
        let i = (y * self.w + x) as usize;
        self.state.grid[i] = v;
        self.state.energy[i] = if v == 3 { self.params.e0 as f32 } else { 0.0 };
        self.upload(ctx);
    }
}
