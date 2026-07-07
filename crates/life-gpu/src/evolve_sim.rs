use crate::context::GpuContext;
use crate::sim::wrap_code;
use life_core::{EvolveParams, EvolveRule, EvolveState, Kernel, Topology};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsEvolve {
    w: u32,
    h: u32,
    wx: u32,
    wy: u32,
    env_weight: f32,
    mut_sigma: f32,
    generation: u32,
    seed: u32,
    rule_mode: u32,
    birth_mask: u32,
    survive_mask: u32,
    kernel_len: u32,
    b_lo: f32,
    b_hi: f32,
    s_lo: f32,
    s_hi: f32,
    kernel: [[f32; 4]; 25],
}

fn build_params(w: u32, h: u32, topo: Topology, k: &Kernel, p: &EvolveParams, generation: u32, seed: u32) -> ParamsEvolve {
    let mut kernel = [[0.0f32; 4]; 25];
    for (i, &(dc, dr, kw)) in k.cells.iter().enumerate() {
        kernel[i] = [dc as f32, dr as f32, kw, 0.0];
    }
    let (rule_mode, birth_mask, survive_mask, b_lo, b_hi, s_lo, s_hi) = match p.rule {
        EvolveRule::DigitSet { birth, survive } => (0, birth as u32, survive as u32, 0.0, 0.0, 0.0, 0.0),
        EvolveRule::Ranged { b_lo, b_hi, s_lo, s_hi } => (1, 0, 0, b_lo, b_hi, s_lo, s_hi),
    };
    ParamsEvolve {
        w, h,
        wx: wrap_code(topo.x), wy: wrap_code(topo.y),
        env_weight: p.env_weight, mut_sigma: p.mut_sigma,
        generation, seed,
        rule_mode, birth_mask, survive_mask,
        kernel_len: k.cells.len() as u32,
        b_lo, b_hi, s_lo, s_hi,
        kernel,
    }
}

pub struct EvolveSim {
    tex: [wgpu::Texture; 2],
    view: [wgpu::TextureView; 2],
    env_tex: wgpu::Texture,
    env_view: wgpu::TextureView,
    front: usize,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    params_buf: wgpu::Buffer,
    kernel: Kernel,
    params: EvolveParams,
    topo: Topology,
    seed: u32,
    generation: u32,
    w: u32,
    h: u32,
}

fn pack_state(st: &EvolveState) -> Vec<f32> {
    let n = (st.w * st.h) as usize;
    let mut out = vec![0.0f32; n * 4];
    for i in 0..n {
        out[i * 4] = st.grid[i] as f32;
        out[i * 4 + 1] = st.tau[i];
        out[i * 4 + 2] = st.theta[i];
    }
    out
}

impl EvolveSim {
    pub fn new(
        ctx: &GpuContext,
        init: &EvolveState,
        env: &[f32],
        kernel: Kernel,
        params: EvolveParams,
        topo: Topology,
        seed: u32,
    ) -> EvolveSim {
        let (w, h) = (init.w, init.h);
        let make_tex = |label, format| ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let tex = [
            make_tex("evolve ping", wgpu::TextureFormat::Rgba32Float),
            make_tex("evolve pong", wgpu::TextureFormat::Rgba32Float),
        ];
        let env_tex = make_tex("env", wgpu::TextureFormat::R32Float);
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex[0], mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&pack_state(init)),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 16), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &env_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(env),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let view = [tex[0].create_view(&Default::default()), tex[1].create_view(&Default::default())];
        let env_view = env_tex.create_view(&Default::default());

        let shader = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("step evolve"),
            source: wgpu::ShaderSource::Wgsl(include_str!("step_evolve.wgsl").into()),
        });
        let float_tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("evolve bgl"),
            entries: &[
                float_tex(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                float_tex(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let pl = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("evolve pl"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = ctx.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("evolve pipeline"),
            layout: Some(&pl),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let params_buf = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("evolve params"),
            contents: bytemuck::bytes_of(&build_params(w, h, topo, &kernel, &params, 0, seed)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        EvolveSim {
            tex, view, env_tex, env_view, front: 0,
            pipeline, layout, params_buf,
            kernel, params, topo, seed, generation: 0, w, h,
        }
    }

    /// Live-tunable params (env_weight, mut_sigma); rule/kernel changes rebuild the sim.
    pub fn set_params(&mut self, params: EvolveParams) {
        self.params = params;
    }

    pub fn upload_env(&self, ctx: &GpuContext, env: &[f32]) {
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.env_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(env),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.w * 4), rows_per_image: Some(self.h) },
            wgpu::Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 },
        );
    }

    pub fn step(&mut self, ctx: &GpuContext) {
        self.generation += 1;
        let params = build_params(self.w, self.h, self.topo, &self.kernel, &self.params, self.generation, self.seed);
        ctx.queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        let (src, dst) = (self.front, 1 - self.front);
        let bind = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("evolve bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.view[src]) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.view[dst]) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&self.env_view) },
                wgpu::BindGroupEntry { binding: 3, resource: self.params_buf.as_entire_binding() },
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

    pub fn read_back(&self, ctx: &GpuContext) -> EvolveState {
        let bpr = self.w * 16; // rgba32float
        let padded = ((bpr + 255) / 256) * 256;
        let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("evolve readback"),
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
        let mut st = EvolveState::new(self.w, self.h);
        for y in 0..self.h {
            let row = &data[(y * padded) as usize..];
            let row_f: &[f32] = bytemuck::cast_slice(&row[..bpr as usize]);
            for x in 0..self.w {
                let i = (y * self.w + x) as usize;
                st.grid[i] = row_f[(x * 4) as usize] as u8;
                st.tau[i] = row_f[(x * 4 + 1) as usize];
                st.theta[i] = row_f[(x * 4 + 2) as usize];
            }
        }
        drop(data);
        buf.unmap();
        st
    }

    pub fn front_view(&self) -> &wgpu::TextureView { &self.view[self.front] }
    pub fn env_view(&self) -> &wgpu::TextureView { &self.env_view }
}

/// Evolve parity check at mut_sigma = 0: GPU step == CPU step, exact.
pub fn evolve_self_test(ctx: &GpuContext) -> Result<(), String> {
    use life_core::{step_evolve, Topology, Wrap};
    let (w, h) = (32u32, 32u32);
    let topo = Topology { x: Wrap::Straight, y: Wrap::Straight };
    let mut s = 0xE01D_BEEF_u64;
    let mut rng = move || {
        s ^= s << 13; s ^= s >> 7; s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut init = EvolveState::new(w, h);
    let mut env = vec![0.0f32; (w * h) as usize];
    for i in 0..(w * h) as usize {
        let r = rng();
        init.grid[i] = if r < 0.25 { 1 } else if r < 0.5 { 2 } else { 0 };
        init.tau[i] = if init.grid[i] != 0 { rng() as f32 } else { 0.0 };
        init.theta[i] = if init.grid[i] != 0 { rng() as f32 } else { 0.0 };
        env[i] = (rng() as f32) * 2.0 - 1.0;
    }
    let cases: [(&str, Kernel, EvolveRule); 2] = [
        ("moore/digitset", Kernel::moore(),
         EvolveRule::DigitSet { birth: 1 << 3, survive: (1 << 2) | (1 << 3) }),
        ("w5x5/ranged", Kernel::weighted5x5(),
         EvolveRule::Ranged { b_lo: 14.0, b_hi: 18.0, s_lo: 12.0, s_hi: 24.0 }),
    ];
    for (label, kernel, rule) in cases {
        let params = EvolveParams { rule, env_weight: 4.0, mut_sigma: 0.0 };
        let cpu = step_evolve(&init, &env, &kernel, &params, topo, &mut || 0.5);
        let mut sim = EvolveSim::new(ctx, &init, &env, kernel, params, topo, 7);
        sim.step(ctx);
        let gpu = sim.read_back(ctx);
        // grid decisions must match exactly; genes tolerate shader FMA
        // contraction (last-ulp drift). Browser asserts the same shape:
        // grid exact, tau/theta < 1e-4 — we use 1e-5.
        const GENE_EPS: f32 = 1e-5;
        let bad: Vec<usize> = (0..(w * h) as usize).filter(|&i| {
            gpu.grid[i] != cpu.grid[i]
                || (gpu.tau[i] - cpu.tau[i]).abs() > GENE_EPS
                || (gpu.theta[i] - cpu.theta[i]).abs() > GENE_EPS
        }).collect();
        if !bad.is_empty() {
            let detail: Vec<String> = bad.iter().take(3).map(|&i| {
                format!(
                    "i={i} ({},{}) cpu=({},{:.6},{:.6}) gpu=({},{:.6},{:.6})",
                    i as u32 % w, i as u32 / w,
                    cpu.grid[i], cpu.tau[i], cpu.theta[i],
                    gpu.grid[i], gpu.tau[i], gpu.theta[i],
                )
            }).collect();
            return Err(format!(
                "evolve_self_test[{label}]: {} cells differ from CPU reference; {}",
                bad.len(), detail.join("; ")
            ));
        }
    }
    Ok(())
}
