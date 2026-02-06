//! UBO triangle in a window: opens a window and renders the green triangle using swapchain.
//! Run: cargo run --bin ubo_triangle_window --features window

#[cfg(feature = "window")]
use lume_rhi::{
    BufferUsage, ColorAttachment, ColorTargetState, DescriptorSetLayoutBinding, DescriptorType,
    Device, GraphicsPipelineDescriptor, ImageLayout, LoadOp, PrimitiveTopology,
    RenderPassDescriptor, ShaderStage, ShaderStages, Swapchain,
    VertexAttribute, VertexBinding, VertexInputDescriptor, VertexInputRate, VertexFormat,
};

#[cfg(feature = "window")]
use winit::application::ApplicationHandler;
#[cfg(feature = "window")]
use winit::event::WindowEvent;
#[cfg(feature = "window")]
use winit::event_loop::{ActiveEventLoop, EventLoop};
#[cfg(feature = "window")]
use std::time::Duration;
#[cfg(feature = "window")]
use winit::window::{Window, WindowId};

#[cfg(feature = "window")]
struct App {
    window: Option<Window>,
    device: Option<std::sync::Arc<dyn Device>>,
    swapchain: Option<Box<dyn Swapchain>>,
    swapchain_image_layouts: Option<Vec<ImageLayout>>,
    pipeline: Option<Box<dyn lume_rhi::GraphicsPipeline>>,
    vertex_buffer: Option<Box<dyn lume_rhi::Buffer>>,
    uniform_buffer: Option<Box<dyn lume_rhi::Buffer>>,
    descriptor_set: Option<Box<dyn lume_rhi::DescriptorSet>>,
    sem_acquire: Option<Box<dyn lume_rhi::Semaphore>>,
    sem_render: Option<Box<dyn lume_rhi::Semaphore>>,
    /// One fence per swapchain image for frame sync (avoids wait_idle and allows higher throughput).
    frame_fences: Option<Vec<Box<dyn lume_rhi::Fence>>>,
    /// Keep submitted command buffers alive until the next wait on that image (freeing early causes ERROR_DEVICE_LOST).
    pending_command_buffers: Option<Vec<Option<Box<dyn lume_rhi::CommandBuffer>>>>,
    /// Defer Vulkan init to RedrawRequested (avoids 0xC000041d when creating surface inside Resized on Windows).
    pending_vulkan_init: bool,
    /// Skip N redraws after init so the window/surface is ready (avoids ERROR_DEVICE_LOST on first submit).
    skip_next_render: u32,
}

#[cfg(feature = "window")]
impl App {
    fn new() -> Self {
        Self {
            window: None,
            device: None,
            swapchain: None,
            swapchain_image_layouts: None,
            pipeline: None,
            vertex_buffer: None,
            uniform_buffer: None,
            descriptor_set: None,
            sem_acquire: None,
            sem_render: None,
            frame_fences: None,
            pending_command_buffers: None,
            pending_vulkan_init: false,
            skip_next_render: 0,
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
        const FENCE_TIMEOUT_NS: u64 = 10_000_000_000; // 10 s
        let image_index = frame.image_index;
        let fences = self.frame_fences.as_ref().unwrap();
        let fence = &fences[image_index as usize];
        let _ = fence.wait(FENCE_TIMEOUT_NS);
        let _ = fence.reset();
        // Free the command buffer we submitted last time we used this image (GPU is done now).
        if let Some(ref mut pending) = self.pending_command_buffers {
            let _ = pending.get_mut(image_index as usize).and_then(|s| s.take());
        }
        let layouts = self.swapchain_image_layouts.as_mut().unwrap();
        let old_layout = layouts[image_index as usize];
        let mut encoder = device.create_command_encoder().expect("create_command_encoder");
        encoder.pipeline_barrier_texture(frame.texture, old_layout, ImageLayout::ColorAttachment);
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
                    initial_layout: Some(ImageLayout::ColorAttachment),
                }],
                depth_stencil_attachment: None,
            }).expect("begin_render_pass");
            pass.set_pipeline(self.pipeline.as_ref().unwrap().as_ref());
            pass.bind_descriptor_set(0, self.descriptor_set.as_ref().unwrap().as_ref());
            pass.set_vertex_buffer(0, self.vertex_buffer.as_ref().unwrap().as_ref(), 0);
            pass.draw(3, 1, 0, 0);
            pass.end();
        }
        encoder.pipeline_barrier_texture(frame.texture, ImageLayout::ColorAttachment, ImageLayout::PresentSrc);
        layouts[image_index as usize] = ImageLayout::PresentSrc;
        drop(frame);
        let cmd = encoder.finish().expect("finish");
        if let Err(e) = device
            .queue()
            .expect("queue")
            .submit(
                &[cmd.as_ref()],
                &[sem_acquire.as_ref()],
                &[sem_render.as_ref()],
                Some(fence.as_ref()),
            )
        {
            eprintln!("queue submit failed: {} (will retry next frame)", e);
            // Re-skip a few frames and retry; avoids giving up on transient DEVICE_LOST / timing races.
            self.skip_next_render = 4;
            return;
        }
        if let Err(e) = swapchain.present(image_index, Some(sem_render.as_ref())) {
            eprintln!("present failed: {}", e);
            return;
        }
        // Keep cmd alive until we wait on this image's fence again (freeing now causes DEVICE_LOST).
        if let Some(ref mut pending) = self.pending_command_buffers {
            if let Some(p) = pending.get_mut(image_index as usize) {
                *p = Some(cmd);
            }
        }
    }
}

#[cfg(feature = "window")]
impl App {
    /// Create Vulkan device and swapchain after window is ready (avoids 0xC000041d on Windows).
    /// Only runs when window has a valid size (after first Resized); avoids creating surface too early.
    fn init_vulkan(&mut self) {
        if self.device.is_some() {
            return;
        }
        let window = self.window.as_ref().expect("window must exist before init_vulkan");
        let size = window.inner_size();
        let (w, h) = (size.width, size.height);
        if w == 0 || h == 0 {
            return;
        }
        let width = size.width.max(1);
        let height = size.height.max(1);
        let device = lume_rhi::VulkanDevice::new_with_surface(window).expect("VulkanDevice::new_with_surface");
        let swapchain = device.create_swapchain((width, height), None).expect("create_swapchain");
        let swapchain_format = swapchain.format();

        let vertex_buffer = device.create_buffer(&lume_rhi::BufferDescriptor {
            label: Some("vertices"),
            size: 9 * 4,
            usage: BufferUsage::VERTEX,
            memory: lume_rhi::BufferMemoryPreference::HostVisible,
        }).expect("create_buffer vertices");
        let vertices: [f32; 9] = [0.0, 0.6, 0.0, -0.6, -0.6, 0.0, 0.6, -0.6, 0.0];
        device
            .write_buffer(vertex_buffer.as_ref(), 0, bytemuck::bytes_of(&vertices))
            .expect("write vertices");

        const UBO_SIZE: u64 = 256;
        let uniform_buffer = device.create_buffer(&lume_rhi::BufferDescriptor {
            label: Some("ubo"),
            size: UBO_SIZE,
            usage: BufferUsage::UNIFORM,
            memory: lume_rhi::BufferMemoryPreference::HostVisible,
        }).expect("create_buffer ubo");
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
                format: swapchain_format,
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
        set.write_buffer(0, uniform_buffer.as_ref(), 0, UBO_SIZE).expect("write_buffer");

        self.sem_acquire = Some(device.create_semaphore().expect("create_semaphore"));
        self.sem_render = Some(device.create_semaphore().expect("create_semaphore"));
        let n = swapchain.image_count() as usize;
        // Create fences already signaled so the first frame wait passes immediately (no 10s block).
        self.frame_fences = Some(
            (0..n)
                .map(|_| device.create_fence(true).expect("create_fence"))
                .collect(),
        );
        self.pending_command_buffers = Some((0..n).map(|_| None).collect());
        let _ = device.wait_idle();
        // Give the window manager time to present the window so the first submit is less racy (reduces random DEVICE_LOST).
        std::thread::sleep(Duration::from_millis(80));
        self.device = Some(device);
        self.swapchain = Some(swapchain);
        self.swapchain_image_layouts = Some(vec![ImageLayout::Undefined; n]);
        self.pipeline = Some(pipeline);
        self.vertex_buffer = Some(vertex_buffer);
        self.uniform_buffer = Some(uniform_buffer);
        self.descriptor_set = Some(set);
        // Skip several redraws so the window/surface is fully ready (reduces random ERROR_DEVICE_LOST on first submit).
        self.skip_next_render = 8;
    }
}

#[cfg(feature = "window")]
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
                self.frame_fences = None;
                self.pending_command_buffers = None;
                self.descriptor_set = None;
                self.uniform_buffer = None;
                self.vertex_buffer = None;
                self.pipeline = None;
                self.swapchain = None;
                self.swapchain_image_layouts = None;
                self.device = None;
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                let (w, h) = (physical_size.width.max(1), physical_size.height.max(1));
                if w == 0 || h == 0 {
                    return;
                }
                if let Some(ref device) = self.device {
                    let _ = device.wait_idle();
                    let old = self.swapchain.as_deref();
                    if let Ok(new_swapchain) = device.create_swapchain((w, h), old) {
                        let n = new_swapchain.image_count() as usize;
                        self.frame_fences = Some(
                            (0..n)
                                .map(|_| device.create_fence(true).expect("create_fence"))
                                .collect(),
                        );
                        self.pending_command_buffers = Some((0..n).map(|_| None).collect());
                        self.swapchain = Some(new_swapchain);
                        self.swapchain_image_layouts = Some(vec![ImageLayout::Undefined; n]);
                    }
                } else {
                    // Defer init to RedrawRequested to avoid 0xC000041d (create surface outside Resized callback).
                    self.pending_vulkan_init = true;
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if self.pending_vulkan_init {
                    self.pending_vulkan_init = false;
                    self.init_vulkan();
                }
                if self.device.is_some() {
                    if self.skip_next_render > 0 {
                        self.skip_next_render -= 1;
                    } else {
                        self.render();
                    }
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

#[cfg(feature = "window")]
fn main() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("panic: {}", info);
        if let Some(loc) = info.location() {
            eprintln!("  at {}:{}:{}", loc.file(), loc.line(), loc.column());
        }
        eprintln!("{:?}", std::backtrace::Backtrace::capture());
    }));
    let mut app = App::new();
    let event_loop = EventLoop::new().expect("EventLoop::new");
    let _ = event_loop.run_app(&mut app);
}

#[cfg(not(feature = "window"))]
fn main() {
    eprintln!("Build and run with: cargo run --bin ubo_triangle_window --features window");
}

#[cfg(feature = "window")]
fn vertex_spirv() -> Vec<u8> {
    let wgsl = r#"
        @vertex
        fn main(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
            return vec4<f32>(pos, 1.0);
        }
    "#;
    compile_wgsl_to_spirv(wgsl, naga::ShaderStage::Vertex)
}

#[cfg(feature = "window")]
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

#[cfg(feature = "window")]
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
