//! Lume RHI: Backend-agnostic Rendering Hardware Interface.
//! This crate defines the traits and types required to abstract over Vulkan and Metal.

use std::any::Any;
use std::fmt::Debug;

/// Unique identifier for a GPU resource.
pub type ResourceId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferUsage {
    Vertex,
    Index,
    Uniform,
    Storage,
    CopySrc,
    CopyDst,
    Indirect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    Rgba8Unorm,
    Bgra8Unorm,
    R32Float,
    Rgba16Float,
    D32Float,
    R16Float,
    Rgba32Float,
}

/// Texture dimension / type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureDimension {
    #[default]
    D2,
    D2Array,
    D3,
    Cube,
}

/// The core device trait that all backends must implement.
pub trait Device: Send + Sync + Debug {
    fn create_buffer(&self, desc: &BufferDescriptor) -> Box<dyn Buffer>;
    fn create_texture(&self, desc: &TextureDescriptor) -> Box<dyn Texture>;
    fn create_compute_pipeline(&self, desc: &ComputePipelineDescriptor) -> Box<dyn ComputePipeline>;
    fn create_graphics_pipeline(&self, desc: &GraphicsPipelineDescriptor) -> Box<dyn GraphicsPipeline>;
    fn create_descriptor_set_layout(&self, bindings: &[DescriptorSetLayoutBinding]) -> Box<dyn DescriptorSetLayout>;
    fn create_descriptor_pool(&self, max_sets: u32) -> Box<dyn DescriptorPool>;

    /// Create a command encoder for recording GPU commands.
    fn create_command_encoder(&self) -> Box<dyn CommandEncoder>;
    
    /// Submit command buffers to the default queue. Does not block; use wait_idle or Fence to synchronize.
    fn submit(&self, command_buffers: Vec<Box<dyn CommandBuffer>>);

    /// Get the main queue (graphics+compute) for submissions.
    fn queue(&self) -> Box<dyn Queue>;

    /// Write data into a buffer (CPU to GPU). Buffer must have been created with host-visible usage.
    fn write_buffer(&self, buffer: &dyn Buffer, offset: u64, data: &[u8]) -> Result<(), String>;

    /// Wait for the device to become idle (all submitted work finished).
    fn wait_idle(&self) -> Result<(), String>;

    /// Create a fence for CPU-GPU synchronization.
    fn create_fence(&self, signaled: bool) -> Box<dyn Fence>;
    /// Create a semaphore for GPU-GPU synchronization.
    fn create_semaphore(&self) -> Box<dyn Semaphore>;

    /// Create a swapchain for presentation (only supported when device was created with a window/surface).
    /// Returns Err for headless devices.
    fn create_swapchain(&self, extent: (u32, u32)) -> Result<Box<dyn Swapchain>, String> {
        let _ = extent;
        Err("Swapchain not supported (device created without surface)".to_string())
    }
}

/// Fence: CPU can wait for GPU to complete submitted work.
pub trait Fence: Send + Sync + Debug {
    fn wait(&self, timeout_ns: u64) -> Result<(), String>;
    fn reset(&self) -> Result<(), String>;
    fn as_any(&self) -> &dyn Any;
}

/// Semaphore: GPU-GPU synchronization between queues or passes.
pub trait Semaphore: Send + Sync + Debug {
    fn as_any(&self) -> &dyn Any;
}

/// Queue for submitting work. Supports non-blocking submit with semaphores and fence.
pub trait Queue: Send + Sync + Debug {
    fn submit(
        &self,
        command_buffers: Vec<Box<dyn CommandBuffer>>,
        wait_semaphores: &[&dyn Semaphore],
        signal_semaphores: &[&dyn Semaphore],
        signal_fence: Option<&dyn Fence>,
    );
}

#[derive(Debug)]
pub struct BufferDescriptor {
    pub label: Option<&'static str>,
    pub size: u64,
    pub usage: BufferUsage,
}

pub trait Buffer: Send + Sync + Debug {
    fn id(&self) -> ResourceId;
    fn size(&self) -> u64;
    fn as_any(&self) -> &dyn Any;
}

#[derive(Debug, Clone)]
pub struct TextureDescriptor {
    pub label: Option<&'static str>,
    /// (width, height, depth_or_layers). For 2D: depth=1. For 2DArray: depth=array_layers. For 3D: depth=depth.
    pub size: (u32, u32, u32),
    pub format: TextureFormat,
    pub usage: TextureUsage,
    pub dimension: TextureDimension,
    pub mip_level_count: u32,
}

impl Default for TextureDescriptor {
    fn default() -> Self {
        Self {
            label: None,
            size: (1, 1, 1),
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsage::empty(),
            dimension: TextureDimension::D2,
            mip_level_count: 1,
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct TextureUsage: u32 {
        const COPY_SRC = 1 << 0;
        const COPY_DST = 1 << 1;
        const TEXTURE_BINDING = 1 << 2;
        const STORAGE_BINDING = 1 << 3;
        const RENDER_ATTACHMENT = 1 << 4;
    }
}

pub trait Texture: Send + Sync + Debug {
    fn id(&self) -> ResourceId;
    fn format(&self) -> TextureFormat;
    fn size(&self) -> (u32, u32, u32);
    fn dimension(&self) -> TextureDimension;
    fn mip_level_count(&self) -> u32;
    fn as_any(&self) -> &dyn Any;
}

pub trait ComputePipeline: Send + Sync + Debug {
    fn as_any(&self) -> &dyn Any;
}

#[derive(Debug, Clone, Default)]
pub struct ComputePipelineDescriptor {
    pub label: Option<&'static str>,
    pub shader_source: String,
    pub entry_point: String,
    pub layout_bindings: Vec<DescriptorSetLayoutBinding>,
}

/// Graphics pipeline for rasterization (vertex + fragment).
pub trait GraphicsPipeline: Send + Sync + Debug {
    fn as_any(&self) -> &dyn Any;
}

/// Descriptor for creating a graphics pipeline.
#[derive(Debug, Clone)]
pub struct GraphicsPipelineDescriptor {
    pub label: Option<&'static str>,
    pub vertex_shader: ShaderStage,
    pub fragment_shader: Option<ShaderStage>,
    pub vertex_input: VertexInputDescriptor,
    pub primitive_topology: PrimitiveTopology,
    pub rasterization: RasterizationState,
    pub color_targets: Vec<ColorTargetState>,
    pub depth_stencil: Option<DepthStencilState>,
    /// Descriptor set layout bindings for UBO/sampled image etc. Used to create pipeline layout.
    pub layout_bindings: Vec<DescriptorSetLayoutBinding>,
}

#[derive(Debug, Clone)]
pub struct ShaderStage {
    pub source: Vec<u8>, // SPIR-V bytes
    pub entry_point: String,
}

#[derive(Debug, Clone, Default)]
pub struct VertexInputDescriptor {
    pub attributes: Vec<VertexAttribute>,
    pub bindings: Vec<VertexBinding>,
}

#[derive(Debug, Clone)]
pub struct VertexAttribute {
    pub location: u32,
    pub binding: u32,
    pub format: VertexFormat,
    pub offset: u32,
}

#[derive(Debug, Clone)]
pub struct VertexBinding {
    pub binding: u32,
    pub stride: u32,
    pub input_rate: VertexInputRate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexInputRate {
    Vertex,
    Instance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VertexFormat {
    #[default]
    Float32x3,
    Float32x2,
    Float32x4,
    Uint32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrimitiveTopology {
    #[default]
    TriangleList,
    TriangleStrip,
    LineList,
    PointList,
}

#[derive(Debug, Clone, Default)]
pub struct RasterizationState {
    pub cull_mode: CullMode,
    pub front_face: FrontFace,
    pub polygon_mode: PolygonMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CullMode {
    None,
    #[default]
    Back,
    Front,
    FrontAndBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrontFace {
    #[default]
    CounterClockwise,
    Clockwise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PolygonMode {
    #[default]
    Fill,
    Line,
    Point,
}

#[derive(Debug, Clone)]
pub struct ColorTargetState {
    pub format: TextureFormat,
    pub blend: Option<BlendState>,
}

#[derive(Debug, Clone)]
pub struct BlendState {
    pub color: BlendComponent,
    pub alpha: BlendComponent,
}

#[derive(Debug, Clone, Copy)]
pub struct BlendComponent {
    pub src_factor: BlendFactor,
    pub dst_factor: BlendFactor,
    pub operation: BlendOp,
}

#[derive(Debug, Clone, Copy)]
pub enum BlendFactor {
    One,
    Zero,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
}

#[derive(Debug, Clone, Copy)]
pub enum BlendOp {
    Add,
    Subtract,
}

#[derive(Debug, Clone)]
pub struct DepthStencilState {
    pub format: TextureFormat,
    pub depth_write_enabled: bool,
    pub depth_compare: CompareOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Never,
    Less,
    Equal,
    LessOrEqual,
    Greater,
    NotEqual,
    GreaterOrEqual,
    Always,
}

/// Render pass descriptor for begin_render_pass.
#[derive(Debug, Clone)]
pub struct RenderPassDescriptor<'a> {
    pub label: Option<&'static str>,
    pub color_attachments: Vec<ColorAttachment<'a>>,
    pub depth_stencil_attachment: Option<DepthStencilAttachment<'a>>,
}

#[derive(Debug, Clone)]
pub struct ColorAttachment<'a> {
    pub texture: &'a dyn Texture,
    pub load_op: LoadOp,
    pub store_op: StoreOp,
    pub clear_value: Option<ClearColor>,
}

#[derive(Debug, Clone, Copy)]
pub struct ClearColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Debug, Clone)]
pub struct DepthStencilAttachment<'a> {
    pub texture: &'a dyn Texture,
    pub depth_load_op: LoadOp,
    pub depth_store_op: StoreOp,
    pub stencil_load_op: LoadOp,
    pub stencil_store_op: StoreOp,
    pub clear_depth: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOp {
    Load,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreOp {
    Store,
    DontCare,
}

pub trait CommandEncoder: Debug {
    fn begin_compute_pass(&mut self) -> Box<dyn ComputePass>;
    fn begin_render_pass<'a>(&mut self, desc: RenderPassDescriptor<'a>) -> Box<dyn RenderPass>;
    fn copy_buffer_to_buffer(
        &mut self,
        src: &dyn Buffer,
        src_offset: u64,
        dst: &dyn Buffer,
        dst_offset: u64,
        size: u64,
    );
    fn copy_buffer_to_texture(
        &mut self,
        src: &dyn Buffer,
        src_offset: u64,
        dst: &dyn Texture,
        dst_mip: u32,
        dst_origin: (u32, u32, u32),
        size: (u32, u32, u32),
    );
    /// Insert a pipeline barrier for layout transitions and synchronization.
    fn pipeline_barrier_texture(
        &mut self,
        texture: &dyn Texture,
        old_layout: ImageLayout,
        new_layout: ImageLayout,
    );
    fn finish(self: Box<Self>) -> Box<dyn CommandBuffer>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageLayout {
    Undefined,
    TransferDst,
    TransferSrc,
    ShaderReadOnly,
    ColorAttachment,
    DepthStencilAttachment,
    General,
}

/// Render pass for recording draw calls.
pub trait RenderPass: Debug {
    fn set_pipeline(&mut self, pipeline: &dyn GraphicsPipeline);
    /// Bind a descriptor set for the currently bound graphics pipeline (set_index must match layout).
    fn bind_descriptor_set(&mut self, set_index: u32, set: &dyn DescriptorSet);
    fn set_vertex_buffer(&mut self, index: u32, buffer: &dyn Buffer, offset: u64);
    fn set_index_buffer(&mut self, buffer: &dyn Buffer, offset: u64, index_format: IndexFormat);
    fn draw(&mut self, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32);
    fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    );
    fn draw_indexed_indirect(&mut self, buffer: &dyn Buffer, offset: u64);
    fn end(self: Box<Self>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexFormat {
    Uint16,
    Uint32,
}

pub trait ComputePass: Debug {
    fn set_pipeline(&mut self, pipeline: &dyn ComputePipeline);
    fn bind_descriptor_set(&mut self, set_index: u32, set: &dyn DescriptorSet);
    fn dispatch(&mut self, x: u32, y: u32, z: u32);
}

/// Descriptor binding type for layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorType {
    UniformBuffer,
    StorageBuffer,
    StorageImage,
    SampledImage,
}

/// Descriptor set layout binding.
#[derive(Debug, Clone)]
pub struct DescriptorSetLayoutBinding {
    pub binding: u32,
    pub descriptor_type: DescriptorType,
    pub count: u32,
    pub stages: ShaderStages,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ShaderStages: u32 {
        const VERTEX = 1 << 0;
        const FRAGMENT = 1 << 1;
        const COMPUTE = 1 << 2;
    }
}

/// Descriptor set layout.
pub trait DescriptorSetLayout: Send + Sync + Debug {
    fn as_any(&self) -> &dyn Any;
}

/// Descriptor pool for allocating sets.
pub trait DescriptorPool: Send + Sync + Debug {
    fn allocate_set(&self, layout: &dyn DescriptorSetLayout) -> Result<Box<dyn DescriptorSet>, String>;
}

/// Descriptor set for binding resources.
pub trait DescriptorSet: Send + Sync + Debug {
    fn write_buffer(&mut self, binding: u32, buffer: &dyn Buffer, offset: u64, size: u64);
    fn write_texture(&mut self, binding: u32, texture: &dyn Texture);
    fn as_any(&self) -> &dyn Any;
}

pub trait CommandBuffer: Send + Sync + Debug {
    fn as_any(&self) -> &dyn Any;
}

/// One swapchain image available for rendering this frame.
pub struct SwapchainFrame<'a> {
    pub image_index: u32,
    pub texture: &'a dyn Texture,
}

/// Swapchain for presenting to a window. Acquire an image, render to it, then present.
pub trait Swapchain: Send + Sync + Debug {
    /// Acquire the next image. Returns (image_index, texture to use as color attachment).
    /// Wait semaphore will be signaled when the image is available.
    fn acquire_next_image(&mut self, wait_semaphore: Option<&dyn Semaphore>) -> Result<SwapchainFrame<'_>, String>;
    /// Present the image. Wait semaphore should be signaled when rendering to that image is done.
    fn present(&self, image_index: u32, wait_semaphore: Option<&dyn Semaphore>) -> Result<(), String>;
    /// Current extent (width, height). May change on resize.
    fn extent(&self) -> (u32, u32);
}

#[cfg(feature = "vulkan")]
pub mod vulkan;

#[cfg(feature = "vulkan")]
pub use vulkan::VulkanDevice;