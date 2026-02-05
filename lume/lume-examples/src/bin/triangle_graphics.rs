//! Triangle graphics example: demonstrates Graphics Pipeline, Render Pass, and draw calls.
//! Creates an offscreen render target and records a triangle draw.
//! Note: Requires valid SPIR-V shaders for full rendering. This demonstrates the API flow.

use lume_rhi::{
    BufferUsage, ColorAttachment, ColorTargetState, Device, GraphicsPipelineDescriptor, LoadOp,
    PrimitiveTopology, RenderPassDescriptor, ShaderStage, StoreOp, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsage, VertexAttribute, VertexBinding, VertexFormat, VertexInputDescriptor,
    VertexInputRate,
};

fn main() {
    let device = lume_rhi::VulkanDevice::new().expect("VulkanDevice::new");

    let render_target = device.create_texture(&TextureDescriptor {
        label: Some("rt"),
        size: (256, 256, 1),
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsage::RENDER_ATTACHMENT | TextureUsage::COPY_SRC,
        dimension: TextureDimension::D2,
        mip_level_count: 1,
    });

    let vertex_buffer = device.create_buffer(&lume_rhi::BufferDescriptor {
        label: Some("vertices"),
        size: 9 * 4,
        usage: BufferUsage::Vertex,
    });

    let pipeline_desc = GraphicsPipelineDescriptor {
        label: Some("triangle"),
        vertex_shader: ShaderStage {
            source: minimal_vertex_spirv(),
            entry_point: "main".to_string(),
        },
        fragment_shader: Some(ShaderStage {
            source: minimal_fragment_spirv(),
            entry_point: "main".to_string(),
        }),
        vertex_input: VertexInputDescriptor {
            attributes: vec![VertexAttribute {
                location: 0,
                binding: 0,
                format: VertexFormat::Float32x3,
                offset: 0,
            }],
            bindings: vec![VertexBinding {
                binding: 0,
                stride: 12,
                input_rate: VertexInputRate::Vertex,
            }],
        },
        primitive_topology: PrimitiveTopology::TriangleList,
        rasterization: Default::default(),
        color_targets: vec![ColorTargetState {
            format: TextureFormat::Rgba8Unorm,
            blend: None,
        }],
        depth_stencil: None,
        layout_bindings: vec![],
    };

    let pipeline = device.create_graphics_pipeline(&pipeline_desc);
    let mut encoder = device.create_command_encoder();

    let mut pass = encoder.begin_render_pass(RenderPassDescriptor {
        label: Some("triangle_pass"),
        color_attachments: vec![ColorAttachment {
            texture: render_target.as_ref(),
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: Some(lume_rhi::ClearColor {
                r: 0.1,
                g: 0.2,
                b: 0.4,
                a: 1.0,
            }),
        }],
        depth_stencil_attachment: None,
    });

    pass.set_pipeline(pipeline.as_ref());
    pass.set_vertex_buffer(0, vertex_buffer.as_ref(), 0);
    pass.draw(3, 1, 0, 0);
    pass.end();

    let cmd = encoder.finish();
    device.submit(vec![cmd]);
    device.wait_idle().expect("wait_idle");

    println!("Triangle graphics API flow OK");
}

fn minimal_vertex_spirv() -> Vec<u8> {
    let wgsl = r#"
        @vertex
        fn main(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
            return vec4<f32>(pos, 1.0);
        }
    "#;
    compile_wgsl_to_spirv(wgsl, naga::ShaderStage::Vertex)
}

fn minimal_fragment_spirv() -> Vec<u8> {
    let wgsl = r#"
        @fragment
        fn main() -> @location(0) vec4<f32> {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
    "#;
    compile_wgsl_to_spirv(wgsl, naga::ShaderStage::Fragment)
}

fn compile_wgsl_to_spirv(source: &str, stage: naga::ShaderStage) -> Vec<u8> {
    let module = naga::front::wgsl::parse_str(source).expect("parse wgsl");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::default(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .expect("validate");
    let options = naga::back::spv::Options::default();
    let pipeline_options = naga::back::spv::PipelineOptions {
        shader_stage: stage,
        entry_point: "main".to_string(),
    };
    let spv = naga::back::spv::write_vec(&module, &info, &options, Some(&pipeline_options))
        .expect("compile to spirv");
    spv.iter().flat_map(|w| w.to_le_bytes()).collect()
}
