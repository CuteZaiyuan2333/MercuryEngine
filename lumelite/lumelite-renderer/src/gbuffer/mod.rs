//! GBuffer pass: fill 4 RTs + depth (Flax layout).

use std::sync::Arc;
use wgpu::CommandEncoder;

const GBUFFER_SHADER: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/gbuffer.wgsl"));

#[derive(Clone)]
pub struct MeshDraw {
    pub vertex_buf: Arc<wgpu::Buffer>,
    pub index_buf: Arc<wgpu::Buffer>,
    pub index_count: u32,
    /// World transform (column-major 4x4). Use identity for model-space geometry.
    pub transform: [f32; 16],
}

pub struct GBufferPass {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl GBufferPass {
    pub fn new(device: &wgpu::Device, format_gbuffer: wgpu::TextureFormat, format_depth: wgpu::TextureFormat) -> Result<Self, String> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gbuffer_shader"),
            source: wgpu::ShaderSource::Wgsl(GBUFFER_SHADER.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gbuffer_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: std::num::NonZeroU64::new(64) },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: std::num::NonZeroU64::new(64) },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gbuffer_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gbuffer_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(format_gbuffer.into()), Some(format_gbuffer.into()), Some(format_gbuffer.into()), Some(format_gbuffer.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: format_depth,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        Ok(Self { pipeline, bind_group_layout })
    }

    pub fn encode(&self, encoder: &mut CommandEncoder, device: &wgpu::Device, queue: &wgpu::Queue, frame: &crate::resources::FrameResources, meshes: &[MeshDraw], view_proj: &[f32; 16]) -> Result<(), String> {
        let view_proj_bytes: &[u8] = bytemuck::cast_slice(view_proj);
        let view_proj_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gbuffer_view_proj"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&view_proj_buf, 0, view_proj_bytes);
        let gbuffer0 = frame.gbuffer0_view();
        let gbuffer1 = frame.gbuffer1_view();
        let gbuffer2 = frame.gbuffer2_view();
        let gbuffer3 = frame.gbuffer3_view();
        let depth_view = frame.depth_view();
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gbuffer_pass"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment { view: &gbuffer0, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } }),
                Some(wgpu::RenderPassColorAttachment { view: &gbuffer1, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } }),
                Some(wgpu::RenderPassColorAttachment { view: &gbuffer2, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 1.0, g: 0.0, b: 0.0, a: 0.0 }), store: wgpu::StoreOp::Store } }),
                Some(wgpu::RenderPassColorAttachment { view: &gbuffer3, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rp.set_pipeline(&self.pipeline);
        for mesh in meshes {
            let model_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gbuffer_model"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&model_buf, 0, bytemuck::cast_slice(&mesh.transform));
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gbuffer_bind_group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: view_proj_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: model_buf.as_entire_binding() },
                ],
            });
            rp.set_bind_group(0, &bind_group, &[]);
            rp.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
            rp.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            rp.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
        drop(rp);
        Ok(())
    }
}
