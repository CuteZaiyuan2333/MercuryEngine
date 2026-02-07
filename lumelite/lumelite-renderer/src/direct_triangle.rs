//! Direct triangle pass: draw triangle to swapchain. Debug - bypass GBuffer/Light/Present.
//! Step 1: uses vertex buffer + view_proj (same layout as GBuffer) to verify mesh renders.

use wgpu::CommandEncoder;

use crate::gbuffer::MeshDraw;

const SHADER: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/direct_triangle.wgsl"));

pub struct DirectTrianglePass {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    view_proj_buf: wgpu::Buffer,
}

impl DirectTrianglePass {
    pub fn new(device: &wgpu::Device, output_format: wgpu::TextureFormat) -> Result<Self, String> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("direct_triangle_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("direct_triangle_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(64),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("direct_triangle_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("direct_triangle"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(output_format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let view_proj_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("direct_triangle_view_proj"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self { pipeline, bind_group_layout, view_proj_buf })
    }

    pub fn encode(
        &self,
        encoder: &mut CommandEncoder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output_view: &wgpu::TextureView,
        meshes: &[MeshDraw],
        view_proj: &[f32; 16],
    ) -> Result<(), String> {
        queue.write_buffer(&self.view_proj_buf, 0, bytemuck::cast_slice(view_proj));
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("direct_triangle"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rp.set_pipeline(&self.pipeline);
        for mesh in meshes {
            let model_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("direct_triangle_model"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&model_buf, 0, bytemuck::cast_slice(&mesh.transform));
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("direct_triangle_bg"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.view_proj_buf.as_entire_binding() },
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
