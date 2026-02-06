//! Lume RHI: Backend-agnostic Rendering Hardware Interface.
//! This crate defines the traits and types required to abstract over Vulkan and Metal.

use std::any::Any;
use std::fmt::Debug;

/// Unique identifier for a GPU resource.
pub type ResourceId = u64;

bitflags::bitflags! {
    /// Buffer usage flags; combine for buffers used in multiple ways (e.g. Vertex | Index | Indirect).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BufferUsage: u32 {
        const VERTEX = 1 << 0;
        const INDEX = 1 << 1;
        const UNIFORM = 1 << 2;
        const STORAGE = 1 << 3;
        const COPY_SRC = 1 << 4;
        const COPY_DST = 1 << 5;
        const INDIRECT = 1 << 6;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    fn create_buffer(&self, desc: &BufferDescriptor) -> Result<Box<dyn Buffer>, String>;
    fn create_texture(&self, desc: &TextureDescriptor) -> Result<Box<dyn Texture>, String>;
    fn create_sampler(&self, desc: &SamplerDescriptor) -> Result<Box<dyn Sampler>, String>;
    fn create_compute_pipeline(
        &self,
        desc: &ComputePipelineDescriptor,
    ) -> Result<Box<dyn ComputePipeline>, String>;
    /// Create a graphics pipeline. The pipeline is created against a compatible render pass configuration.
    /// **Compatibility:** When recording with [`CommandEncoder::begin_render_pass`], the runtime
    /// `RenderPassDescriptor` must use the same color/depth formats and load/store ops as the
    /// `color_targets` and `depth_stencil` in this descriptor, so that the Vulkan render pass
    /// (or backend equivalent) is compatible with the pipeline.
    fn create_graphics_pipeline(
        &self,
        desc: &GraphicsPipelineDescriptor,
    ) -> Result<Box<dyn GraphicsPipeline>, String>;
    fn create_descriptor_set_layout(
        &self,
        bindings: &[DescriptorSetLayoutBinding],
    ) -> Result<Box<dyn DescriptorSetLayout>, String>;
    fn create_descriptor_pool(&self, max_sets: u32) -> Result<Box<dyn DescriptorPool>, String>;

    /// Create a descriptor pool with configurable per-type capacities (e.g. for bindless).
    /// When `desc.pool_sizes` is empty, uses the same default as `create_descriptor_pool` (max_sets * 4 per type).
    fn create_descriptor_pool_with_descriptor(
        &self,
        desc: &DescriptorPoolDescriptor,
    ) -> Result<Box<dyn DescriptorPool>, String>;

    /// Create a command encoder for recording GPU commands.
    fn create_command_encoder(&self) -> Result<Box<dyn CommandEncoder>, String>;

    /// Submit command buffers to the default queue. Does not block; use wait_idle or Fence to synchronize.
    /// For frame loops with a swapchain, prefer [`queue()`](Self::queue) and then [`Queue::submit`]
    /// with wait/signal semaphores (and optionally a fence) so that acquire and present are correctly
    /// synchronized; using only this method can lead to missing synchronization with present.
    fn submit(&self, command_buffers: Vec<Box<dyn CommandBuffer>>) -> Result<(), String>;

    /// Get the main queue (graphics+compute) for submissions.
    /// Use this for swapchain frame loops: call `submit(cmd_bufs, &[acquire_semaphore], &[render_semaphore], None)`
    /// then present with the render semaphore so the GPU waits for rendering before presenting.
    fn queue(&self) -> Result<Box<dyn Queue>, String>;

    /// Write data into a buffer (CPU to GPU). Buffer must be host-visible (Buffer::host_visible() == true).
    fn write_buffer(&self, buffer: &dyn Buffer, offset: u64, data: &[u8]) -> Result<(), String>;

    /// Upload data into any buffer (HostVisible or DeviceLocal).
    /// For HostVisible buffers, uses write_buffer. For DeviceLocal, uses staging buffer + copy.
    /// DeviceLocal buffers must have BufferUsage::COPY_DST. Blocks until upload completes.
    fn upload_to_buffer(&self, buffer: &dyn Buffer, offset: u64, data: &[u8]) -> Result<(), String>;

    /// Optional dedicated transfer queue for async copies (e.g. VG streaming).
    /// When present, use with [`upload_to_buffer_async`](Self::upload_to_buffer_async) to avoid blocking the main queue.
    fn transfer_queue(&self) -> Option<Box<dyn Queue>> {
        None
    }

    /// Upload into a device-local buffer using staging + copy. Prefer transfer queue when [`transfer_queue`](Self::transfer_queue) returns Some.
    /// Blocks until the copy completes (so staging can be freed); use transfer queue so the main queue is not blocked.
    /// If `signal_fence` is provided, it is signaled when the copy completes; the implementation still waits so staging can be freed.
    /// For fire-and-forget streaming (e.g. VG), use [`submit_buffer_copy`](Self::submit_buffer_copy) with a caller-owned staging buffer and wait the fence later.
    fn upload_to_buffer_async(
        &self,
        buffer: &dyn Buffer,
        offset: u64,
        data: &[u8],
        _signal_fence: Option<&dyn Fence>,
    ) -> Result<(), String> {
        self.upload_to_buffer(buffer, offset, data)
    }

    /// Submits a buffer-to-buffer copy without waiting. For use with a caller-owned staging buffer: write into staging, call this, then wait `signal_fence` (if provided) before reusing or dropping the staging buffer.
    /// Does not block; optional `signal_fence` is signaled when the copy completes.
    /// Returns `Err` if the backend does not support non-blocking submit (default implementation).
    fn submit_buffer_copy(
        &self,
        _src: &dyn Buffer,
        _src_offset: u64,
        _dst: &dyn Buffer,
        _dst_offset: u64,
        _size: u64,
        _signal_fence: Option<&dyn Fence>,
    ) -> Result<(), String> {
        Err("submit_buffer_copy not implemented".to_string())
    }

    /// Wait for the device to become idle (all submitted work finished).
    fn wait_idle(&self) -> Result<(), String>;

    /// Create a fence for CPU-GPU synchronization.
    fn create_fence(&self, signaled: bool) -> Result<Box<dyn Fence>, String>;
    /// Create a semaphore for GPU-GPU synchronization.
    fn create_semaphore(&self) -> Result<Box<dyn Semaphore>, String>;

    /// Create a swapchain for presentation (only supported when device was created with a window/surface).
    /// Returns Err for headless devices.
    /// When resizing, pass the current swapchain as `old_swapchain` so the driver can reuse resources (Vulkan oldSwapchain).
    fn create_swapchain(
        &self,
        extent: (u32, u32),
        old_swapchain: Option<&dyn Swapchain>,
    ) -> Result<Box<dyn Swapchain>, String> {
        let _ = (extent, old_swapchain);
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
/// The caller must keep command_buffers alive until the signal_fence has been waited on
/// (otherwise the GPU may still be executing and freeing the buffers causes DEVICE_LOST).
pub trait Queue: Send + Sync + Debug {
    fn submit(
        &self,
        command_buffers: &[&dyn CommandBuffer],
        wait_semaphores: &[&dyn Semaphore],
        signal_semaphores: &[&dyn Semaphore],
        signal_fence: Option<&dyn Fence>,
    ) -> Result<(), String>;
}

/// When true, buffer is mappable (host-visible) and write_buffer can be used. When false, device-local only (e.g. for VG/GI streaming).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BufferMemoryPreference {
    #[default]
    HostVisible,
    DeviceLocal,
}

#[derive(Debug, Clone)]
pub struct BufferDescriptor {
    pub label: Option<&'static str>,
    pub size: u64,
    pub usage: BufferUsage,
    /// HostVisible: mappable, write_buffer works. DeviceLocal: faster GPU access, write via copy from staging.
    pub memory: BufferMemoryPreference,
}

impl Default for BufferDescriptor {
    fn default() -> Self {
        Self {
            label: None,
            size: 0,
            usage: BufferUsage::VERTEX,
            memory: BufferMemoryPreference::HostVisible,
        }
    }
}

pub trait Buffer: Send + Sync + Debug {
    fn id(&self) -> ResourceId;
    fn size(&self) -> u64;
    /// If true, Device::write_buffer can be used. If false, buffer is device-local; upload via staging copy.
    fn host_visible(&self) -> bool {
        true
    }
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

/// Filter mode for sampler min/mag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterMode {
    #[default]
    Nearest,
    Linear,
}

/// Address mode for sampler U/V/W.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddressMode {
    #[default]
    Repeat,
    MirroredRepeat,
    ClampToEdge,
    ClampToBorder,
}

#[derive(Debug, Clone)]
pub struct SamplerDescriptor {
    pub label: Option<&'static str>,
    pub min_filter: FilterMode,
    pub mag_filter: FilterMode,
    pub address_mode_u: AddressMode,
    pub address_mode_v: AddressMode,
    pub address_mode_w: AddressMode,
    pub anisotropy_clamp: Option<f32>,
}

impl Default for SamplerDescriptor {
    fn default() -> Self {
        Self {
            label: None,
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            anisotropy_clamp: None,
        }
    }
}

/// Sampler for texture sampling (filter, address mode). Used with CombinedImageSampler in descriptor sets.
pub trait Sampler: Send + Sync + Debug {
    fn as_any(&self) -> &dyn Any;
}

pub trait ComputePipeline: Send + Sync + Debug {
    fn as_any(&self) -> &dyn Any;
}

#[derive(Debug, Clone, Default)]
pub struct ComputePipelineDescriptor {
    pub label: Option<&'static str>,
    /// SPIR-V binary (little-endian, 4-byte aligned).
    pub shader_source: Vec<u8>,
    pub entry_point: String,
    pub layout_bindings: Vec<DescriptorSetLayoutBinding>,
}

/// Graphics pipeline for rasterization (vertex + fragment).
pub trait GraphicsPipeline: Send + Sync + Debug {
    fn as_any(&self) -> &dyn Any;
}

/// Descriptor for creating a graphics pipeline.
/// The pipeline's `color_targets` and `depth_stencil` formats (and load/store) must match the
/// attachments used at runtime in [`RenderPassDescriptor`] when calling `begin_render_pass`,
/// so that the backend can use a compatible render pass.
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

/// Color attachment state for a graphics pipeline.
/// When `load_op`/`store_op` are None, the backend uses Clear/Store (default for main pass).
/// Set them explicitly (e.g. Load/Store) for passes that read from the same attachment (e.g. post-process).
#[derive(Debug, Clone)]
pub struct ColorTargetState {
    pub format: TextureFormat,
    pub blend: Option<BlendState>,
    /// If None, backend uses Clear. Set to Load for passes that preserve attachment contents.
    pub load_op: Option<LoadOp>,
    /// If None, backend uses Store. Set to DontCare when attachment is not read later.
    pub store_op: Option<StoreOp>,
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

/// Depth/stencil attachment state for a graphics pipeline.
/// When `depth_load_op`/`depth_store_op` are None, the backend uses Load/Store.
#[derive(Debug, Clone)]
pub struct DepthStencilState {
    pub format: TextureFormat,
    pub depth_write_enabled: bool,
    pub depth_compare: CompareOp,
    /// If None, backend uses Load. Set to Clear for first use in a frame.
    pub depth_load_op: Option<LoadOp>,
    /// If None, backend uses Store.
    pub depth_store_op: Option<StoreOp>,
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
/// Use the same color/depth formats and load/store ops as the [`GraphicsPipelineDescriptor`]
/// used to create the pipeline that will be bound in this pass, so that the backend render pass
/// is compatible with the pipeline.
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
    /// Layout the image is in when the render pass begins. None = Undefined (render pass will transition).
    pub initial_layout: Option<ImageLayout>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadOp {
    Load,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoreOp {
    Store,
    DontCare,
}

pub trait CommandEncoder: Debug {
    fn begin_compute_pass(&mut self) -> Box<dyn ComputePass>;
    fn begin_render_pass<'a>(&mut self, desc: RenderPassDescriptor<'a>) -> Result<Box<dyn RenderPass>, String>;
    fn copy_buffer_to_buffer(
        &mut self,
        src: &dyn Buffer,
        src_offset: u64,
        dst: &dyn Buffer,
        dst_offset: u64,
        size: u64,
    );
    /// Copy buffer data into a texture region. The caller must ensure the destination texture is in
    /// [`ImageLayout::TransferDst`] before this call (e.g. via [`Self::pipeline_barrier_texture`]);
    /// after the copy, transition to [`ImageLayout::ShaderReadOnly`] if the texture will be sampled.
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
    /// Insert a pipeline barrier for buffer memory (e.g. compute write -> graphics/compute read).
    /// Uses shader write -> shader read with compute stage to fragment/vertex/compute.
    fn pipeline_barrier_buffer(
        &mut self,
        buffer: &dyn Buffer,
        offset: u64,
        size: u64,
    );
    fn finish(self: Box<Self>) -> Result<Box<dyn CommandBuffer>, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageLayout {
    Undefined,
    TransferDst,
    TransferSrc,
    ShaderReadOnly,
    ColorAttachment,
    DepthStencilAttachment,
    General,
    /// For swapchain images before present. Use after render pass, then present.
    PresentSrc,
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
    /// Draw indexed indirect. For VG, use draw_count > 1 and stride = sizeof(DrawIndexedIndirectCommand).
    fn draw_indexed_indirect(&mut self, buffer: &dyn Buffer, offset: u64, draw_count: u32, stride: u32);
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
    /// Dispatch compute using indirect buffer (offset in bytes to VkDispatchIndirectCommand: x, y, z).
    fn dispatch_indirect(&mut self, buffer: &dyn Buffer, offset: u64);
}

/// Descriptor binding type for layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorType {
    UniformBuffer,
    StorageBuffer,
    StorageImage,
    SampledImage,
    /// Image + sampler in one binding; use write_sampled_image to bind both.
    CombinedImageSampler,
}

/// Descriptor set layout binding.
#[derive(Debug, Clone)]
pub struct DescriptorSetLayoutBinding {
    pub binding: u32,
    pub descriptor_type: DescriptorType,
    pub count: u32,
    pub stages: ShaderStages,
}

/// Descriptor for creating a descriptor pool with configurable per-type capacities.
/// When `pool_sizes` is empty, backends use a default (e.g. max_sets * 4 per type).
#[derive(Debug, Clone, Default)]
pub struct DescriptorPoolDescriptor {
    pub max_sets: u32,
    /// Per-type descriptor counts (e.g. for bindless: `(DescriptorType::CombinedImageSampler, 256)`).
    /// Types not listed get a backend default (e.g. max_sets * 4).
    pub pool_sizes: Vec<(DescriptorType, u32)>,
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
    fn write_buffer(&mut self, binding: u32, buffer: &dyn Buffer, offset: u64, size: u64) -> Result<(), String>;
    fn write_texture(&mut self, binding: u32, texture: &dyn Texture) -> Result<(), String>;
    /// Bind texture + sampler for a CombinedImageSampler binding (or SampledImage with separate sampler).
    fn write_sampled_image(&mut self, binding: u32, texture: &dyn Texture, sampler: &dyn Sampler) -> Result<(), String>;
    /// Write buffer at a specific array element (for bindless; use 0 for single descriptor).
    fn write_buffer_at(
        &mut self,
        binding: u32,
        array_element: u32,
        buffer: &dyn Buffer,
        offset: u64,
        size: u64,
    ) -> Result<(), String>;
    /// Write texture at a specific array element (for bindless; use 0 for single descriptor).
    fn write_texture_at(&mut self, binding: u32, array_element: u32, texture: &dyn Texture) -> Result<(), String>;
    /// Write sampled image at a specific array element (for bindless; use 0 for single descriptor).
    fn write_sampled_image_at(
        &mut self,
        binding: u32,
        array_element: u32,
        texture: &dyn Texture,
        sampler: &dyn Sampler,
    ) -> Result<(), String>;
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
    fn as_any(&self) -> &dyn Any;
    /// Acquire the next image. Returns (image_index, texture to use as color attachment).
    /// Wait semaphore will be signaled when the image is available.
    fn acquire_next_image(&mut self, wait_semaphore: Option<&dyn Semaphore>) -> Result<SwapchainFrame<'_>, String>;
    /// Present the image. Wait semaphore should be signaled when rendering to that image is done.
    fn present(&self, image_index: u32, wait_semaphore: Option<&dyn Semaphore>) -> Result<(), String>;
    /// Current extent (width, height). May change on resize.
    fn extent(&self) -> (u32, u32);
    /// Number of swapchain images (for layout tracking).
    fn image_count(&self) -> u32;
    /// Color format of swapchain images. Pipeline color_targets must use this format for compatibility.
    fn format(&self) -> TextureFormat;
}

#[cfg(feature = "vulkan")]
pub mod vulkan;

#[cfg(feature = "vulkan")]
pub use vulkan::VulkanDevice;