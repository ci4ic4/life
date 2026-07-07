//! Torus mesh, UV paint, orbit camera.
pub mod camera;
pub mod mesh;
pub use camera::OrbitCamera;

use wgpu::util::DeviceExt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode { Torus, Flat }

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
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, sim_view: &wgpu::TextureView, size: (u32, u32)) -> Renderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("draw"),
            source: wgpu::ShaderSource::Wgsl(include_str!("draw.wgsl").into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let vp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vp"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = make_bind(device, &bgl, &vp_buf, sim_view);
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
        let torus = mk(&mesh::torus(2.0, 0.9, 96, 48));
        let quad = mk(&mesh::quad());
        let depth = make_depth(device, size);
        Renderer { pipeline, bgl, vp_buf, torus, quad, bind, depth }
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        self.depth = make_depth(device, size);
    }

    pub fn set_sim_view(&mut self, device: &wgpu::Device, sim_view: &wgpu::TextureView) {
        self.bind = make_bind(device, &self.bgl, &self.vp_buf, sim_view);
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

fn make_bind(device: &wgpu::Device, bgl: &wgpu::BindGroupLayout, vp_buf: &wgpu::Buffer, sim_view: &wgpu::TextureView) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("draw bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: vp_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(sim_view) },
        ],
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
