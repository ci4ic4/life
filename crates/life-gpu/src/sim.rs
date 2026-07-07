use crate::context::GpuContext;
use life_core::{Grid, Topology, Wrap, BS};
use wgpu::util::DeviceExt;

pub(crate) fn wrap_code(w: Wrap) -> u32 {
    match w { Wrap::Straight => 0, Wrap::None => 1, Wrap::Flip => 2 }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params { w: u32, h: u32, wx: u32, wy: u32, birth: u32, survive: u32, _p0: u32, _p1: u32 }

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsStoch {
    w: u32, h: u32, wx: u32, wy: u32,
    generation: u32, seed: u32, _p0: u32, _p1: u32,
    birth: [[f32; 4]; 3],   // 9 entries used of 12 (vec4 uniform stride)
    survive: [[f32; 4]; 3],
}

fn pack9(t: &[f32; 9]) -> [[f32; 4]; 3] {
    let mut out = [[0.0f32; 4]; 3];
    for (i, &v) in t.iter().enumerate() {
        out[i / 4][i % 4] = v;
    }
    out
}

#[derive(Clone, Copy, Debug)]
pub enum SimRule {
    Deterministic(BS),
    Stochastic { p_birth: [f32; 9], p_survive: [f32; 9], seed: u32 },
}

pub struct Sim {
    tex: [wgpu::Texture; 2],
    view: [wgpu::TextureView; 2],
    front: usize,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    params_buf: wgpu::Buffer,
    rule: SimRule,
    topo: Topology,
    generation: u32,
    w: u32,
    h: u32,
}

impl Sim {
    pub fn new(ctx: &GpuContext, init: &Grid, rule: SimRule, topo: Topology) -> Sim {
        let (w, h) = (init.w, init.h);
        let make_tex = |label| ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let tex = [make_tex("ping"), make_tex("pong")];
        // upload init into tex[0] as R32Uint (one u32 per cell)
        let cells32: Vec<u32> = init.cells.iter().map(|&c| c as u32).collect();
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex[0], mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&cells32),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let view = [
            tex[0].create_view(&Default::default()),
            tex[1].create_view(&Default::default()),
        ];
        let shader = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("step"),
            source: wgpu::ShaderSource::Wgsl(match rule {
                SimRule::Deterministic(_) => include_str!("step.wgsl").into(),
                SimRule::Stochastic { .. } => include_str!("step_stoch.wgsl").into(),
            }),
        });
        let layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("step bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::R32Uint, view_dimension: wgpu::TextureViewDimension::D2 },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let pl = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("step pl"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = ctx.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("step pipeline"),
            layout: Some(&pl),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let (wx, wy) = (wrap_code(topo.x), wrap_code(topo.y));
        let params_bytes: Vec<u8> = match rule {
            SimRule::Deterministic(bs) => bytemuck::bytes_of(&Params {
                w, h, wx, wy,
                birth: bs.birth as u32, survive: bs.survive as u32,
                _p0: 0, _p1: 0,
            }).to_vec(),
            SimRule::Stochastic { p_birth, p_survive, seed } => bytemuck::bytes_of(&ParamsStoch {
                w, h, wx, wy,
                generation: 0, seed, _p0: 0, _p1: 0,
                birth: pack9(&p_birth), survive: pack9(&p_survive),
            }).to_vec(),
        };
        let params_buf = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: &params_bytes,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Sim { tex, view, front: 0, pipeline, layout, params_buf, rule, topo, generation: 0, w, h }
    }

    pub fn step(&mut self, ctx: &GpuContext) {
        // stochastic draws depend on the generation counter — refresh params each step
        if let SimRule::Stochastic { p_birth, p_survive, seed } = self.rule {
            self.generation += 1;
            let params = ParamsStoch {
                w: self.w, h: self.h,
                wx: wrap_code(self.topo.x), wy: wrap_code(self.topo.y),
                generation: self.generation, seed, _p0: 0, _p1: 0,
                birth: pack9(&p_birth), survive: pack9(&p_survive),
            };
            ctx.queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        }
        let (src, dst) = (self.front, 1 - self.front);
        let bind = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("step bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.view[src]) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.view[dst]) },
                wgpu::BindGroupEntry { binding: 2, resource: self.params_buf.as_entire_binding() },
            ],
        });
        let mut enc = ctx.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((self.w + 7) / 8, (self.h + 7) / 8, 1);
        }
        ctx.queue.submit(Some(enc.finish()));
        self.front = dst;
    }

    pub fn read_back(&self, ctx: &GpuContext) -> Grid {
        let bpr = self.w * 4; // R32Uint
        let padded = ((bpr + 255) / 256) * 256;
        let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * self.h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = ctx.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &self.tex[self.front], mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(self.h) } },
            wgpu::Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 },
        );
        ctx.queue.submit(Some(enc.finish()));
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        ctx.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = buf.slice(..).get_mapped_range();
        let mut cells = vec![0u8; (self.w * self.h) as usize];
        for y in 0..self.h {
            let row = &data[(y * padded) as usize..];
            let row32: &[u32] = bytemuck::cast_slice(&row[..(bpr) as usize]);
            for x in 0..self.w {
                cells[(y * self.w + x) as usize] = row32[x as usize] as u8;
            }
        }
        drop(data);
        buf.unmap();
        Grid { w: self.w, h: self.h, cells }
    }

    pub fn front_view(&self) -> &wgpu::TextureView { &self.view[self.front] }

    /// Poke a single cell into the live (front) texture — pen/glider tools.
    pub fn write_cell(&self, ctx: &GpuContext, x: u32, y: u32, v: u32) {
        if x >= self.w || y >= self.h { return; }
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.tex[self.front],
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::bytes_of(&v),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
    }
}

/// Boot-time parity check: one GPU step must equal the CPU reference exactly —
/// directly for the deterministic rule, and at the deterministic limit (exact
/// 0/1 probability tables) for the stochastic pipeline.
pub fn gpu_self_test(ctx: &GpuContext) -> Result<(), String> {
    use life_core::step_deterministic;
    let (w, h) = (32u32, 32u32);
    let bs = life_core::parse_bs("B3/S23").unwrap();
    let topo = Topology { x: Wrap::Straight, y: Wrap::Straight };
    // deterministic pseudo-random seed grid (xorshift, no rand dep here)
    let mut state = 0xB33Fu64;
    let mut init = Grid::new(w, h);
    for c in init.cells.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *c = (state % 100 < 35) as u8;
    }
    let cpu = step_deterministic(&init, &bs, topo);

    let check = |label: &str, rule: SimRule| -> Result<(), String> {
        let mut sim = Sim::new(ctx, &init, rule, topo);
        sim.step(ctx);
        let gpu = sim.read_back(ctx);
        if gpu.cells == cpu.cells {
            Ok(())
        } else {
            let diff = gpu.cells.iter().zip(&cpu.cells).filter(|(a, b)| a != b).count();
            Err(format!("gpu_self_test[{label}]: {diff} cells differ from CPU reference"))
        }
    };

    check("deterministic", SimRule::Deterministic(bs))?;
    // stochastic at the deterministic limit: p is exactly 1 for members, 0 otherwise,
    // and rand01 ∈ [0,1) — so `rand < p` reproduces the digit-set rule exactly.
    let limit_table = |mask: u16| -> [f32; 9] {
        std::array::from_fn(|n| if mask & (1 << n) != 0 { 1.0 } else { 0.0 })
    };
    check("stochastic@limit", SimRule::Stochastic {
        p_birth: limit_table(bs.birth),
        p_survive: limit_table(bs.survive),
        seed: 0xC0FFEE,
    })?;
    // evolve pipeline parity at mut_sigma = 0
    crate::evolve_sim::evolve_self_test(ctx)
}
