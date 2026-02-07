//! GBuffer pass: fill 4 RTs + depth (Flax layout). Single PBR pipeline, stride 32, four texture bindings.

use std::sync::Arc;
use wgpu::CommandEncoder;

const GBUFFER_SHADER: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/gbuffer.wgsl"));

/// Four PBR texture views (base_color, normal, metallic_roughness, ao). Required per mesh; use default when no material.
#[derive(Clone)]
pub struct PbrTextureViews {
    pub base_color: Arc<wgpu::TextureView>,
    pub normal: Arc<wgpu::TextureView>,
    pub metallic_roughness: Arc<wgpu::TextureView>,
    pub ao: Arc<wgpu::TextureView>,
}

#[derive(Clone)]
pub struct MeshDraw {
    pub vertex_buf: Arc<wgpu::Buffer>,
    pub index_buf: Arc<wgpu::Buffer>,
    pub index_count: u32,
    /// World transform (column-major 4x4). Use identity for model-space geometry.
    pub transform: [f32; 16],
    /// PBR textures for this mesh (always set; use default when host has no material).
    pub pbr_textures: PbrTextureViews,
}

pub struct GBufferPass {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout_0: wgpu::BindGroupLayout,
    bind_group_layout_1: wgpu::BindGroupLayout,
    view_proj_buf: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

impl GBufferPass {
    pub fn new(
        device: &wgpu::Device,
        format_gbuffer: wgpu::TextureFormat,
        format_depth: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gbuffer_shader"),
            source: wgpu::ShaderSource::Wgsl(GBUFFER_SHADER.into()),
        });

        let bind_group_layout_0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gbuffer_bind_group_layout_0"),
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

        let bind_group_layout_1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gbuffer_bind_group_layout_1"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gbuffer_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout_0, &bind_group_layout_1],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gbuffer_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 12,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 24,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[
                    Some(format_gbuffer.into()),
                    Some(format_gbuffer.into()),
                    Some(format_gbuffer.into()),
                    Some(format_gbuffer.into()),
                ],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: format_depth,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let view_proj_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gbuffer_view_proj"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gbuffer_sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Ok(Self {
            pipeline,
            bind_group_layout_0,
            bind_group_layout_1,
            view_proj_buf,
            sampler,
        })
    }

    pub fn encode(
        &self,
        encoder: &mut CommandEncoder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &crate::resources::FrameResources,
        meshes: &[MeshDraw],
        view_proj: &[f32; 16],
    ) -> Result<(), String> {
        queue.write_buffer(&self.view_proj_buf, 0, bytemuck::cast_slice(view_proj));
        let gbuffer0 = frame.gbuffer0_view();
        let gbuffer1 = frame.gbuffer1_view();
        let gbuffer2 = frame.gbuffer2_view();
        let gbuffer3 = frame.gbuffer3_view();
        let depth_view = frame.depth_view();
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gbuffer_pass"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &gbuffer0,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &gbuffer1,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &gbuffer2,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &gbuffer3,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rp.set_pipeline(&self.pipeline);
        let w = frame.width() as f32;
        let h = frame.height() as f32;
        rp.set_viewport(0.0, 0.0, w, h, 0.0, 1.0);
        for mesh in meshes {
            let model_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gbuffer_model"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&model_buf, 0, bytemuck::cast_slice(&mesh.transform));
            let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gbuffer_bind_group_0"),
                layout: &self.bind_group_layout_0,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.view_proj_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: model_buf.as_entire_binding(),
                    },
                ],
            });
            let bg1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gbuffer_bind_group_1"),
                layout: &self.bind_group_layout_1,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&mesh.pbr_textures.base_color),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&mesh.pbr_textures.normal),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            &mesh.pbr_textures.metallic_roughness,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&mesh.pbr_textures.ao),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            rp.set_bind_group(0, &bg0, &[]);
            rp.set_bind_group(1, &bg1, &[]);
            rp.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
            rp.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            rp.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
        drop(rp);
        Ok(())
    }
}
