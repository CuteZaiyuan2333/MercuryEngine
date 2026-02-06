//! Triangle with UBO: exercises GraphicsPipeline descriptor set layout,
//! bind_descriptor_set in RenderPass, and DescriptorSet::write_buffer with UniformBuffer type.

use lume_rhi::{
    BufferUsage, ColorAttachment, ColorTargetState, DescriptorSetLayoutBinding, DescriptorType,
    GraphicsPipelineDescriptor, LoadOp, PrimitiveTopology, RenderPassDescriptor,
    ShaderStage, ShaderStages, StoreOp, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsage, VertexAttribute, VertexBinding, VertexInputDescriptor, VertexInputRate,
    VertexFormat,
};

fn main() {
    let device = lume_rhi::create_device(lume_rhi::DeviceCreateParams::default())
        .expect("create_device");

    let render_target = device.create_texture(&TextureDescriptor {
        label: Some("rt"),
        size: (256, 256, 1),
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsage::RENDER_ATTACHMENT | TextureUsage::COPY_SRC,
        dimension: TextureDimension::D2,
        mip_level_count: 1,
    }).expect("create_texture");

    let vertex_buffer = device.create_buffer(&lume_rhi::BufferDescriptor {
        label: Some("vertices"),
        size: 9 * 4,
        usage: BufferUsage::VERTEX,
        memory: lume_rhi::BufferMemoryPreference::HostVisible,
    }).expect("create_buffer");
    let vertices: [f32; 9] = [0.0, 0.6, 0.0, -0.6, -0.6, 0.0, 0.6, -0.6, 0.0];
    device
        .write_buffer(vertex_buffer.as_ref(), 0, bytemuck::bytes_of(&vertices))
        .expect("write vertices");

    // UBO: vec4 color. Use 256 bytes to satisfy minUniformBufferOffsetAlignment.
    const UBO_SIZE: u64 = 256;
    let uniform_buffer = device.create_buffer(&lume_rhi::BufferDescriptor {
        label: Some("ubo"),
        size: UBO_SIZE,
        usage: BufferUsage::UNIFORM,
        memory: lume_rhi::BufferMemoryPreference::HostVisible,
    }).expect("create_buffer ubo");
    let color_data: [f32; 4] = [0.2, 0.8, 0.2, 1.0]; // green
    device
        .write_buffer(uniform_buffer.as_ref(), 0, bytemuck::bytes_of(&color_data))
        .expect("write ubo");

    let layout_bindings = vec![DescriptorSetLayoutBinding {
        binding: 0,
        descriptor_type: DescriptorType::UniformBuffer,
        count: 1,
        stages: ShaderStages::FRAGMENT,
    }];

    let pipeline_desc = GraphicsPipelineDescriptor {
        label: Some("ubo_triangle"),
        vertex_shader: ShaderStage {
            source: vertex_spirv(),
            entry_point: "main".to_string(),
        },
        fragment_shader: Some(ShaderStage {
            source: fragment_spirv(),
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
            load_op: None,
            store_op: None,
        }],
        depth_stencil: None,
        layout_bindings: layout_bindings.clone(),
    };

    let pipeline = device.create_graphics_pipeline(&pipeline_desc).expect("create_graphics_pipeline");
    let layout = device.create_descriptor_set_layout(&layout_bindings).expect("create_descriptor_set_layout");
    let pool = device.create_descriptor_pool(1).expect("create_descriptor_pool");
    let mut set = pool.allocate_set(layout.as_ref()).expect("allocate set");
    // UBO range must be multiple of minUniformBufferOffsetAlignment (often 256)
    set.write_buffer(0, uniform_buffer.as_ref(), 0, UBO_SIZE).expect("write_buffer");

    let mut encoder = device.create_command_encoder().expect("create_command_encoder");
    let mut pass = encoder.begin_render_pass(RenderPassDescriptor {
        label: Some("ubo_pass"),
        color_attachments: vec![ColorAttachment {
            texture: render_target.as_ref(),
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: Some(lume_rhi::ClearColor {
                r: 0.1,
                g: 0.1,
                b: 0.15,
                a: 1.0,
            }),
            initial_layout: None,
        }],
        depth_stencil_attachment: None,
    }).expect("begin_render_pass");

    pass.set_pipeline(pipeline.as_ref());
    pass.bind_descriptor_set(0, set.as_ref());
    pass.set_vertex_buffer(0, vertex_buffer.as_ref(), 0);
    pass.draw(3, 1, 0, 0);
    pass.end();

    let cmd = encoder.finish().expect("finish");
    device.submit(vec![cmd]).expect("submit");
    device.wait_idle().expect("wait_idle");

    println!("UBO triangle OK");
}

fn vertex_spirv() -> Vec<u8> {
    let wgsl = r#"
        @vertex
        fn main(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
            return vec4<f32>(pos, 1.0);
        }
    "#;
    compile_wgsl_to_spirv(wgsl, naga::ShaderStage::Vertex)
}

fn fragment_spirv() -> Vec<u8> {
    let wgsl = r#"
        @group(0) @binding(0) var<uniform> color: vec4<f32>;
        @fragment
        fn main() -> @location(0) vec4<f32> {
            return color;
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
