//! Torus mesh, UV paint, orbit camera.
pub mod camera;
pub mod mesh;
pub mod pick;
pub use camera::OrbitCamera;
pub use pick::pick_cell;

use wgpu::util::DeviceExt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode { Torus, Flat }

/// What the surface paints: a binary sim's uint texture, an evolve sim's float
/// state + env pair, or an ecology sim's single packed float texture (all with
/// runtime-switchable colour modes bar the binary one).
///
/// Note this is an enum over *texture views*, not over simulation types — which
/// is why a CPU-stepped simulation can feed it without the renderer changing.
pub enum RenderSource<'a> {
    Binary(&'a wgpu::TextureView),
    Evolve { state: &'a wgpu::TextureView, env: &'a wgpu::TextureView },
    Ecology(&'a wgpu::TextureView),
}

pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    vp_buf: wgpu::Buffer,
    torus: (wgpu::Buffer, wgpu::Buffer, u32),
    quad: (wgpu::Buffer, wgpu::Buffer, u32),
    bind: wgpu::BindGroup,
    depth: wgpu::TextureView,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, source: RenderSource, size: (u32, u32)) -> Renderer {
        let float_tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let (shader_src, entries): (&str, Vec<wgpu::BindGroupLayoutEntry>) = match source {
            RenderSource::Binary(_) => (
                include_str!("draw.wgsl"),
                vec![
                    uniform_entry,
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                        count: None,
                    },
                ],
            ),
            RenderSource::Evolve { .. } => (
                include_str!("draw_evolve.wgsl"),
                vec![uniform_entry, float_tex(1), float_tex(2)],
            ),
            RenderSource::Ecology(_) => (
                include_str!("draw_ecology.wgsl"),
                vec![uniform_entry, float_tex(1)],
            ),
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("draw"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw bgl"),
            entries: &entries,
        });
        // 64B view_proj + 16B (colour mode + pad); binary shader reads only the mat4
        let vp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vp"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = make_bind(device, &bgl, &vp_buf, &source);
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("draw pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<mesh::Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("draw pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[vbl], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let mk = |data: &(Vec<mesh::Vertex>, Vec<u32>)| {
            let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None, contents: bytemuck::cast_slice(&data.0), usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None, contents: bytemuck::cast_slice(&data.1), usage: wgpu::BufferUsages::INDEX,
            });
            (vb, ib, data.1.len() as u32)
        };
        let torus = mk(&mesh::torus(mesh::MAJOR, mesh::MINOR, 96, 48));
        let quad = mk(&mesh::quad());
        let depth = make_depth(device, size);
        Renderer { pipeline, bgl, vp_buf, torus, quad, bind, depth }
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        self.depth = make_depth(device, size);
    }

    pub fn set_source(&mut self, device: &wgpu::Device, source: RenderSource) {
        self.bind = make_bind(device, &self.bgl, &self.vp_buf, &source);
    }

    /// Colour mode for the evolve shader: 0 clan, 1 τ, 2 θ, 3 env. No-op for binary.
    pub fn set_color_mode(&self, queue: &wgpu::Queue, mode: u32) {
        queue.write_buffer(&self.vp_buf, 64, bytemuck::bytes_of(&[mode, 0u32, 0, 0]));
    }

    pub fn draw(&self, device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::TextureView, cam: &OrbitCamera, mode: ViewMode, aspect: f32) {
        let vp = cam.view_proj(aspect).to_cols_array();
        queue.write_buffer(&self.vp_buf, 0, bytemuck::cast_slice(&vp));
        let (vb, ib, n) = match mode { ViewMode::Torus => &self.torus, ViewMode::Flat => &self.quad };
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.06, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*n, 0, 0..1);
        }
        queue.submit(Some(enc.finish()));
    }
}

fn make_bind(device: &wgpu::Device, bgl: &wgpu::BindGroupLayout, vp_buf: &wgpu::Buffer, source: &RenderSource) -> wgpu::BindGroup {
    let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: vp_buf.as_entire_binding() }];
    match source {
        RenderSource::Binary(view) => {
            entries.push(wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) });
        }
        RenderSource::Evolve { state, env } => {
            entries.push(wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(state) });
            entries.push(wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(env) });
        }
        RenderSource::Ecology(state) => {
            entries.push(wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(state) });
        }
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("draw bg"),
        layout: bgl,
        entries: &entries,
    })
}

fn make_depth(device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    }).create_view(&Default::default())
}
