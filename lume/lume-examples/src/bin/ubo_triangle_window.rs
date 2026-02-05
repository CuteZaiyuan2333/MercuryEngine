//! UBO triangle in a window: opens a window and renders the green triangle using swapchain.
//! Requires lume-rhi with feature "window". Run with: cargo run --bin ubo_triangle_window --features window

use lume_rhi::{
    BufferUsage, ColorAttachment, ColorTargetState, DescriptorSetLayoutBinding, DescriptorType,
    Device, GraphicsPipelineDescriptor, LoadOp, PrimitiveTopology, RenderPassDescriptor,
    ShaderStage, ShaderStages, Swapchain, TextureFormat,
    VertexAttribute, VertexBinding, VertexInputDescriptor, VertexInputRate, VertexFormat,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

struct App {
    window: Option<Window>,
    device: Option<std::sync::Arc<dyn Device>>,
    swapchain: Option<Box<dyn Swapchain>>,
    pipeline: Option<Box<dyn lume_rhi::GraphicsPipeline>>,
    vertex_buffer: Option<Box<dyn lume_rhi::Buffer>>,
    uniform_buffer: Option<Box<dyn lume_rhi::Buffer>>,
    descriptor_set: Option<Box<dyn lume_rhi::DescriptorSet>>,
    sem_acquire: Option<Box<dyn lume_rhi::Semaphore>>,
    sem_render: Option<Box<dyn lume_rhi::Semaphore>>,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            device: None,
            swapchain: None,
            pipeline: None,
            vertex_buffer: None,
            uniform_buffer: None,
            descriptor_set: None,
            sem_acquire: None,
            sem_render: None,
        }
    }

    fn render(&mut self) {
        let device = self.device.as_ref().unwrap().as_ref();
        let swapchain = self.swapchain.as_mut().unwrap();
        let (w, h) = swapchain.extent();
        if w == 0 || h == 0 {
            return;
        }
        let sem_acquire = self.sem_acquire.as_ref().unwrap();
        let sem_render = self.sem_render.as_ref().unwrap();
        let frame = match swapchain.acquire_next_image(Some(sem_acquire.as_ref())) {
            Ok(f) => f,
            Err(_) => return,
        };
        let image_index = frame.image_index;
        let mut encoder = device.create_command_encoder();
        {
            let mut pass = encoder.begin_render_pass(RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: vec![ColorAttachment {
                    texture: frame.texture,
                    load_op: LoadOp::Clear,
                    store_op: lume_rhi::StoreOp::Store,
                    clear_value: Some(lume_rhi::ClearColor {
                        r: 0.1,
                        g: 0.1,
                        b: 0.15,
                        a: 1.0,
                    }),
                }],
                depth_stencil_attachment: None,
            });
            pass.set_pipeline(self.pipeline.as_ref().unwrap().as_ref());
            pass.bind_descriptor_set(0, self.descriptor_set.as_ref().unwrap().as_ref());
            pass.set_vertex_buffer(0, self.vertex_buffer.as_ref().unwrap().as_ref(), 0);
            pass.draw(3, 1, 0, 0);
            pass.end();
        }
        drop(frame);
        let cmd = encoder.finish();
        device
            .queue()
            .submit(
                vec![cmd],
                &[sem_acquire.as_ref()],
                &[sem_render.as_ref()],
                None,
            );
        let _ = swapchain.present(image_index, Some(sem_render.as_ref()));
        let _ = device.wait_idle();
    }
}

impl App {
    /// Create Vulkan device and swapchain after window is ready (avoids 0xC000041d on Windows).
    fn init_vulkan(&mut self) {
        if self.device.is_some() {
            return;
        }
        let window = self.window.as_ref().expect("window must exist before init_vulkan");
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let device = lume_rhi::VulkanDevice::new_with_surface(window).expect("VulkanDevice::new_with_surface");
        let swapchain = device.create_swapchain((width, height)).expect("create_swapchain");

        let vertex_buffer = device.create_buffer(&lume_rhi::BufferDescriptor {
            label: Some("vertices"),
            size: 9 * 4,
            usage: BufferUsage::Vertex,
        });
        let vertices: [f32; 9] = [0.0, 0.6, 0.0, -0.6, -0.6, 0.0, 0.6, -0.6, 0.0];
        device
            .write_buffer(vertex_buffer.as_ref(), 0, bytemuck::bytes_of(&vertices))
            .expect("write vertices");

        const UBO_SIZE: u64 = 256;
        let uniform_buffer = device.create_buffer(&lume_rhi::BufferDescriptor {
            label: Some("ubo"),
            size: UBO_SIZE,
            usage: BufferUsage::Uniform,
        });
        let color_data: [f32; 4] = [0.2, 0.8, 0.2, 1.0];
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
            }],
            depth_stencil: None,
            layout_bindings: layout_bindings.clone(),
        };

        let pipeline = device.create_graphics_pipeline(&pipeline_desc);
        let layout = device.create_descriptor_set_layout(&layout_bindings);
        let pool = device.create_descriptor_pool(1);
        let mut set = pool.allocate_set(layout.as_ref()).expect("allocate set");
        set.write_buffer(0, uniform_buffer.as_ref(), 0, UBO_SIZE);

        self.sem_acquire = Some(device.create_semaphore());
        self.sem_render = Some(device.create_semaphore());
        self.device = Some(device);
        self.swapchain = Some(swapchain);
        self.pipeline = Some(pipeline);
        self.vertex_buffer = Some(vertex_buffer);
        self.uniform_buffer = Some(uniform_buffer);
        self.descriptor_set = Some(set);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = winit::window::WindowAttributes::default()
            .with_title("Lume UBO Triangle")
            .with_inner_size(winit::dpi::LogicalSize::new(640, 480));
        let window = event_loop.create_window(attrs).expect("create window");
        self.window = Some(window);
        // Defer Vulkan/swapchain creation to first RedrawRequested so HWND is valid (avoids 0xC000041d on Windows)
        if let Some(ref w) = self.window {
            w.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                // Tear down Vulkan (wait idle, then drop swapchain/surface/device) before window closes
                // to avoid STATUS_ACCESS_VIOLATION when driver touches surface after HWND is gone.
                if let Some(ref device) = self.device {
                    let _ = device.wait_idle();
                }
                self.sem_acquire = None;
                self.sem_render = None;
                self.descriptor_set = None;
                self.uniform_buffer = None;
                self.vertex_buffer = None;
                self.pipeline = None;
                self.swapchain = None;
                self.device = None;
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.init_vulkan();
                if self.device.is_some() {
                    self.render();
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let mut app = App::new();
    let event_loop = EventLoop::new().expect("EventLoop::new");
    let _ = event_loop.run_app(&mut app);
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
