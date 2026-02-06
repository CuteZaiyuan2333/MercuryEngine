//! Vulkan backend for Lume RHI.
//! Implements Device, Buffer, Texture, ComputePipeline, GraphicsPipeline, CommandEncoder, Fence, Semaphore.

mod buffer;
mod descriptor;
mod memory;
mod pipeline;
mod queue;
mod render_pass;
mod sampler;
mod texture;

#[cfg(feature = "window")]
mod swapchain;

use crate::{
    Buffer, BufferDescriptor, BufferMemoryPreference, BufferUsage, CommandBuffer, CommandEncoder, ComputePass,
    ComputePipelineDescriptor, DescriptorPoolDescriptor, DescriptorSetLayoutBinding, DescriptorPool,
    DescriptorSetLayout, Device, Fence, GraphicsPipelineDescriptor, ImageLayout, LoadOp, Queue,
    RenderPassDescriptor, ResourceId, Sampler, SamplerDescriptor, Semaphore, StoreOp, Texture,
    TextureDescriptor, TextureFormat,
};
use ash::vk;
use ash::vk::Handle;
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Arc, Mutex};

/// Returns validation layer names to enable if validation is requested (feature or LUME_VALIDATION=1).
#[cfg(feature = "validation")]
fn validation_layer_names(entry: &ash::Entry) -> Vec<CString> {
    let disable = std::env::var("LUME_VALIDATION").is_ok_and(|v| v == "0" || v.eq_ignore_ascii_case("false"));
    let enable = !disable;
    if !enable {
        return vec![];
    }
    let layers = match unsafe { entry.enumerate_instance_layer_properties() } {
        Ok(l) => l,
        Err(_) => return vec![],
    };
    const KHRONOS: &str = "VK_LAYER_KHRONOS_validation";
    const LUNARG: &str = "VK_LAYER_LUNARG_standard_validation";
    for prop in &layers {
        let name = unsafe { std::ffi::CStr::from_ptr(prop.layer_name.as_ptr()).to_string_lossy() };
        if name == KHRONOS {
            return vec![CString::new(KHRONOS).unwrap()];
        }
        if name == LUNARG {
            return vec![CString::new(LUNARG).unwrap()];
        }
    }
    vec![]
}

#[cfg(not(feature = "validation"))]
fn validation_layer_names(_entry: &ash::Entry) -> Vec<CString> {
    if std::env::var("LUME_VALIDATION").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true")) {
        eprintln!("LUME_VALIDATION=1 set but lume-rhi built without 'validation' feature; validation layers not available");
    }
    vec![]
}

pub use buffer::VulkanBuffer;
pub use descriptor::{VulkanDescriptorPool, VulkanDescriptorSet, VulkanDescriptorSetLayout};
pub use pipeline::{VulkanComputePipeline, VulkanGraphicsPipeline};
pub use render_pass::{ColorAttachmentInfo, DepthAttachmentInfo};
pub use sampler::VulkanSampler;
pub use texture::{create_texture as create_vulkan_texture, VulkanTexture};

#[cfg(feature = "window")]
pub use swapchain::{VulkanSwapchain, VulkanSwapchainImage};

/// Returns the VkImageView for a texture (VulkanTexture or VulkanSwapchainImage). Used when building render pass attachments.
fn texture_to_image_view(texture: &dyn crate::Texture) -> Result<vk::ImageView, String> {
    if let Some(t) = texture.as_any().downcast_ref::<VulkanTexture>() {
        return Ok(t.view);
    }
    #[cfg(feature = "window")]
    if let Some(s) = texture.as_any().downcast_ref::<VulkanSwapchainImage>() {
        return Ok(s.view());
    }
    #[cfg(feature = "window")]
    return Err("color attachment texture must be VulkanTexture or VulkanSwapchainImage".to_string());
    #[cfg(not(feature = "window"))]
    Err("texture must be VulkanTexture (enable 'window' for swapchain images)".to_string())
}

/// Key for caching VkRenderPass by attachment configuration.
#[derive(Hash, Eq, PartialEq, Clone)]
struct RenderPassCacheKey {
    color: Vec<(TextureFormat, LoadOp, StoreOp, Option<ImageLayout>)>,
    depth: Option<(TextureFormat, LoadOp, StoreOp)>,
}

/// Key for caching VkFramebuffer by render pass and attachment image views.
#[derive(Hash, Eq, PartialEq, Clone)]
struct FramebufferCacheKey {
    render_pass: u64,
    width: u32,
    height: u32,
    attachment_views: Vec<u64>,
}

pub struct VulkanDevice {
    #[allow(dead_code)]
    entry: ash::Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: Arc<ash::Device>,
    queue: vk::Queue,
    #[allow(dead_code)]
    queue_family_index: u32,
    command_pool: vk::CommandPool,
    /// Dedicated transfer-only queue and pool when available (for async uploads / VG streaming).
    transfer_queue: Option<vk::Queue>,
    transfer_command_pool: Option<vk::CommandPool>,
    next_id: std::sync::atomic::AtomicU64,
    #[cfg(feature = "window")]
    surface_state: Option<SurfaceState>,
    /// Cached VkRenderPass by attachment config to avoid per-frame create/destroy.
    render_pass_cache: Arc<Mutex<HashMap<RenderPassCacheKey, vk::RenderPass>>>,
    /// Cached VkFramebuffer by (render_pass, extent, image_views) to avoid per-frame create/destroy.
    framebuffer_cache: Arc<Mutex<HashMap<FramebufferCacheKey, vk::Framebuffer>>>,
}

#[cfg(feature = "window")]
struct SurfaceState {
    surface: vk::SurfaceKHR,
    surface_loader: ash::khr::surface::Instance,
    swapchain_loader: ash::khr::swapchain::Device,
}

fn image_layout_to_vk(l: ImageLayout) -> vk::ImageLayout {
    match l {
        ImageLayout::Undefined => vk::ImageLayout::UNDEFINED,
        ImageLayout::TransferDst => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        ImageLayout::TransferSrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        ImageLayout::ShaderReadOnly => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        ImageLayout::ColorAttachment => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        ImageLayout::DepthStencilAttachment => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        ImageLayout::General => vk::ImageLayout::GENERAL,
        ImageLayout::PresentSrc => vk::ImageLayout::PRESENT_SRC_KHR,
    }
}

/// Returns (src_stage, src_access, dst_stage, dst_access) for an image layout transition.
/// When is_depth is true, uses DEPTH_* access flags for attachment layouts.
fn image_barrier_stages_access(
    old_layout: ImageLayout,
    new_layout: ImageLayout,
    is_depth: bool,
) -> (
    vk::PipelineStageFlags,
    vk::AccessFlags,
    vk::PipelineStageFlags,
    vk::AccessFlags,
) {
    let color_write = if is_depth {
        vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
    } else {
        vk::AccessFlags::COLOR_ATTACHMENT_WRITE
    };
    let shader_stages = vk::PipelineStageFlags::VERTEX_SHADER
        | vk::PipelineStageFlags::FRAGMENT_SHADER
        | vk::PipelineStageFlags::COMPUTE_SHADER;
    let result = match (old_layout, new_layout) {
        (ImageLayout::Undefined, ImageLayout::ColorAttachment)
        | (ImageLayout::PresentSrc, ImageLayout::ColorAttachment)
        | (ImageLayout::Undefined, ImageLayout::DepthStencilAttachment)
        | (ImageLayout::PresentSrc, ImageLayout::DepthStencilAttachment) => (
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::AccessFlags::empty(),
            if is_depth {
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
            } else {
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            },
            if is_depth {
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
            } else {
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            },
        ),
        (ImageLayout::ColorAttachment, ImageLayout::PresentSrc)
        | (ImageLayout::DepthStencilAttachment, ImageLayout::PresentSrc) => (
            if is_depth {
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
            } else {
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            },
            color_write,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::AccessFlags::MEMORY_READ,
        ),
        (ImageLayout::Undefined, ImageLayout::TransferDst) => (
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::AccessFlags::empty(),
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_WRITE,
        ),
        (ImageLayout::TransferDst, ImageLayout::ShaderReadOnly) => (
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_WRITE,
            shader_stages,
            vk::AccessFlags::SHADER_READ,
        ),
        (ImageLayout::TransferDst, ImageLayout::TransferSrc) => (
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_READ,
        ),
        (ImageLayout::TransferSrc, ImageLayout::ShaderReadOnly) => (
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_READ,
            shader_stages,
            vk::AccessFlags::SHADER_READ,
        ),
        (ImageLayout::TransferSrc, ImageLayout::TransferDst) => (
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_READ,
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_WRITE,
        ),
        (ImageLayout::ShaderReadOnly, ImageLayout::ColorAttachment)
        | (ImageLayout::ShaderReadOnly, ImageLayout::DepthStencilAttachment) => (
            shader_stages,
            vk::AccessFlags::SHADER_READ,
            if is_depth {
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
            } else {
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            },
            color_write,
        ),
        (ImageLayout::ColorAttachment, ImageLayout::ShaderReadOnly)
        | (ImageLayout::DepthStencilAttachment, ImageLayout::ShaderReadOnly) => (
            if is_depth {
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
            } else {
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            },
            color_write,
            shader_stages,
            vk::AccessFlags::SHADER_READ,
        ),
        (ImageLayout::General, ImageLayout::ShaderReadOnly) => (
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            shader_stages,
            vk::AccessFlags::SHADER_READ,
        ),
        (ImageLayout::General, ImageLayout::ColorAttachment)
        | (ImageLayout::General, ImageLayout::DepthStencilAttachment) => (
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            if is_depth {
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
            } else {
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            },
            color_write,
        ),
        (ImageLayout::ShaderReadOnly, ImageLayout::General) => (
            shader_stages,
            vk::AccessFlags::SHADER_READ,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
        ),
        (ImageLayout::ColorAttachment, ImageLayout::General)
        | (ImageLayout::DepthStencilAttachment, ImageLayout::General) => (
            if is_depth {
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
            } else {
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            },
            color_write,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
        ),
        (ImageLayout::Undefined, ImageLayout::General) => (
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::AccessFlags::empty(),
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
        ),
        _ => (
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
        ),
    };
    result
}

impl VulkanDevice {
    /// Create a Vulkan device using the first available physical device and queue family.
    pub fn new() -> Result<Arc<Self>, String> {
        let entry = unsafe { ash::Entry::load().map_err(|e| e.to_string())? };
        let app_name = CString::new("Lume").unwrap();
        let engine_name = CString::new("Lume").unwrap();
        let app_info = vk::ApplicationInfo::default()
            .api_version(vk::API_VERSION_1_2)
            .application_name(&app_name)
            .engine_name(&engine_name);
        let layer_names: Vec<CString> = validation_layer_names(&entry);
        let layer_ptrs: Vec<*const i8> = layer_names.iter().map(|c| c.as_ptr()).collect();
        let instance_create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(if layer_ptrs.is_empty() { &[] } else { &layer_ptrs });
        let instance = unsafe {
            entry.create_instance(&instance_create_info, None).map_err(|e| e.to_string())?
        };
        let physical_devices = unsafe {
            instance.enumerate_physical_devices().map_err(|e| e.to_string())?
        };
        let physical_device = physical_devices.into_iter().next()
            .ok_or("No Vulkan physical device found")?;
        let queue_family_properties = unsafe {
            instance.get_physical_device_queue_family_properties(physical_device)
        };
        let queue_family_index = queue_family_properties
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::COMPUTE) || p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .ok_or("No suitable queue family")? as u32;
        // Dedicated transfer-only family: TRANSFER but not GRAPHICS and not COMPUTE (optional; many GPUs use unified queues).
        let transfer_family_index = queue_family_properties.iter().position(|p| {
            p.queue_flags.contains(vk::QueueFlags::TRANSFER)
                && !p.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                && !p.queue_flags.contains(vk::QueueFlags::COMPUTE)
        });
        let queue_priorities = [1.0f32];
        let mut queue_create_infos = vec![vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priorities)];
        if let Some(tf) = transfer_family_index {
            if tf != queue_family_index as usize {
                queue_create_infos.push(
                    vk::DeviceQueueCreateInfo::default()
                        .queue_family_index(tf as u32)
                        .queue_priorities(&queue_priorities),
                );
            }
        }
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos);
        let device_raw = unsafe {
            instance.create_device(physical_device, &device_create_info, None).map_err(|e| e.to_string())?
        };
        let queue = unsafe { device_raw.get_device_queue(queue_family_index, 0) };
        let (transfer_queue, transfer_command_pool) = match transfer_family_index {
            Some(tf) if tf != queue_family_index as usize => {
                let tq = unsafe { device_raw.get_device_queue(tf as u32, 0) };
                let tpool_info = vk::CommandPoolCreateInfo::default()
                    .queue_family_index(tf as u32)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
                let tpool = unsafe {
                    device_raw.create_command_pool(&tpool_info, None).map_err(|e| e.to_string())?
                };
                (Some(tq), Some(tpool))
            }
            _ => (None, None),
        };
        let command_pool_create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe {
            device_raw.create_command_pool(&command_pool_create_info, None).map_err(|e| e.to_string())?
        };
        let device = Arc::new(device_raw);
        Ok(Arc::new(Self {
            entry,
            instance,
            physical_device,
            device,
            queue,
            queue_family_index,
            command_pool,
            transfer_queue,
            transfer_command_pool,
            next_id: std::sync::atomic::AtomicU64::new(1),
            #[cfg(feature = "window")]
            surface_state: None,
            render_pass_cache: Arc::new(Mutex::new(HashMap::new())),
            framebuffer_cache: Arc::new(Mutex::new(HashMap::new())),
        }))
    }

    fn next_id(&self) -> ResourceId {
        self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(feature = "window")]
    /// Create a Vulkan device with a window surface for swapchain/presentation.
    pub fn new_with_surface(
        window: &dyn raw_window_handle::HasWindowHandle,
    ) -> Result<Arc<Self>, String> {
        use ash::khr::surface::Instance as SurfaceInstance;
        use ash::khr::swapchain::Device as SwapchainDevice;
        use std::ffi::CStr;
        let handle = window.window_handle().map_err(|e| format!("window_handle: {:?}", e))?;
        let raw = handle.as_raw();
        let (hwnd, hinstance) = match raw {
            raw_window_handle::RawWindowHandle::Win32(win) => {
                let hwnd = win.hwnd.get() as isize;
                let hinstance = win.hinstance.map(|h| h.get() as isize).unwrap_or(0);
                (hwnd, hinstance)
            }
            _ => return Err("Only Win32 window is supported".to_string()),
        };
        let entry = unsafe { ash::Entry::load().map_err(|e| e.to_string())? };
        let app_name = CString::new("Lume").unwrap();
        let engine_name = CString::new("Lume").unwrap();
        let app_info = vk::ApplicationInfo::default()
            .api_version(vk::API_VERSION_1_2)
            .application_name(&app_name)
            .engine_name(&engine_name);
        let ext_names = unsafe {
            [
                CStr::from_bytes_with_nul_unchecked(b"VK_KHR_surface\0").as_ptr(),
                ash::khr::win32_surface::NAME.as_ptr(),
            ]
        };
        let layer_names: Vec<CString> = validation_layer_names(&entry);
        let layer_ptrs: Vec<*const i8> = layer_names.iter().map(|c| c.as_ptr()).collect();
        let instance_create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&ext_names)
            .enabled_layer_names(if layer_ptrs.is_empty() { &[] } else { &layer_ptrs });
        let instance = unsafe {
            entry.create_instance(&instance_create_info, None).map_err(|e| e.to_string())?
        };
        let surface_loader = SurfaceInstance::new(&entry, &instance);
        let win32_create_info = vk::Win32SurfaceCreateInfoKHR::default()
            .hinstance(hinstance)
            .hwnd(hwnd);
        let surface = unsafe {
            let win32 = ash::khr::win32_surface::Instance::new(&entry, &instance);
            win32.create_win32_surface(&win32_create_info, None).map_err(|e| format!("create_win32_surface: {:?}", e))?
        };
        let physical_devices = unsafe {
            instance.enumerate_physical_devices().map_err(|e| e.to_string())?
        };
        let queue_family_properties = unsafe {
            instance.get_physical_device_queue_family_properties(physical_devices[0])
        };
        let queue_family_index = queue_family_properties
            .iter()
            .enumerate()
            .find(|(i, p)| {
                let supports_graphics = p.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                let supports_present = unsafe {
                    surface_loader.get_physical_device_surface_support(
                        physical_devices[0],
                        *i as u32,
                        surface,
                    ).unwrap_or(false)
                };
                supports_graphics && supports_present
            })
            .map(|(i, _)| i as u32)
            .ok_or("No queue family with graphics and present support")?;
        let transfer_family_index = queue_family_properties.iter().position(|p| {
            p.queue_flags.contains(vk::QueueFlags::TRANSFER)
                && !p.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                && !p.queue_flags.contains(vk::QueueFlags::COMPUTE)
        });
        let queue_priorities = [1.0f32];
        let mut queue_create_infos = vec![vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priorities)];
        if let Some(tf) = transfer_family_index {
            if tf != queue_family_index as usize {
                queue_create_infos.push(
                    vk::DeviceQueueCreateInfo::default()
                        .queue_family_index(tf as u32)
                        .queue_priorities(&queue_priorities),
                );
            }
        }
        let swapchain_ext = ash::khr::swapchain::NAME.as_ptr();
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(std::slice::from_ref(&swapchain_ext));
        let device_raw = unsafe {
            instance.create_device(physical_devices[0], &device_create_info, None).map_err(|e| e.to_string())?
        };
        let queue = unsafe { device_raw.get_device_queue(queue_family_index, 0) };
        let (transfer_queue, transfer_command_pool) = match transfer_family_index {
            Some(tf) if tf != queue_family_index as usize => {
                let tq = unsafe { device_raw.get_device_queue(tf as u32, 0) };
                let tpool_info = vk::CommandPoolCreateInfo::default()
                    .queue_family_index(tf as u32)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
                let tpool = unsafe {
                    device_raw.create_command_pool(&tpool_info, None).map_err(|e| e.to_string())?
                };
                (Some(tq), Some(tpool))
            }
            _ => (None, None),
        };
        let swapchain_loader = SwapchainDevice::new(&instance, &device_raw);
        let command_pool_create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe {
            device_raw.create_command_pool(&command_pool_create_info, None).map_err(|e| e.to_string())?
        };
        let device = Arc::new(device_raw);
        Ok(Arc::new(Self {
            entry,
            instance,
            physical_device: physical_devices[0],
            device,
            queue,
            queue_family_index,
            command_pool,
            transfer_queue,
            transfer_command_pool,
            next_id: std::sync::atomic::AtomicU64::new(1),
            surface_state: Some(SurfaceState {
                surface,
                surface_loader,
                swapchain_loader,
            }),
            render_pass_cache: Arc::new(Mutex::new(HashMap::new())),
            framebuffer_cache: Arc::new(Mutex::new(HashMap::new())),
        }))
    }

    /// Allocates a command buffer from the given pool, records a buffer-to-buffer copy, and returns the command buffer.
    fn allocate_and_record_copy(
        device: Arc<ash::Device>,
        pool: vk::CommandPool,
        src: &dyn crate::Buffer,
        src_offset: u64,
        dst: &dyn crate::Buffer,
        dst_offset: u64,
        size: u64,
    ) -> Result<VulkanCommandBuffer, String> {
        let src_buf = src
            .as_any()
            .downcast_ref::<buffer::VulkanBuffer>()
            .ok_or("src must be VulkanBuffer")?;
        let dst_buf = dst
            .as_any()
            .downcast_ref::<buffer::VulkanBuffer>()
            .ok_or("dst must be VulkanBuffer")?;
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let buffers = unsafe {
            device.allocate_command_buffers(&alloc_info).map_err(|e| e.to_string())?
        };
        let cmd = buffers[0];
        unsafe {
            device
                .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .map_err(|e| e.to_string())?;
            let region = vk::BufferCopy::default()
                .src_offset(src_offset)
                .dst_offset(dst_offset)
                .size(size);
            device.cmd_copy_buffer(cmd, src_buf.buffer, dst_buf.buffer, &[region]);
            device.end_command_buffer(cmd).map_err(|e| e.to_string())?;
        }
        Ok(VulkanCommandBuffer {
            device,
            command_pool: pool,
            buffer: cmd,
        })
    }

    fn buffer_usage_to_vk(usage: BufferUsage) -> vk::BufferUsageFlags {
        let mut flags = vk::BufferUsageFlags::empty();
        if usage.contains(BufferUsage::VERTEX) {
            flags |= vk::BufferUsageFlags::VERTEX_BUFFER;
        }
        if usage.contains(BufferUsage::INDEX) {
            flags |= vk::BufferUsageFlags::INDEX_BUFFER;
        }
        if usage.contains(BufferUsage::UNIFORM) {
            flags |= vk::BufferUsageFlags::UNIFORM_BUFFER;
        }
        if usage.contains(BufferUsage::STORAGE) {
            flags |= vk::BufferUsageFlags::STORAGE_BUFFER;
        }
        if usage.contains(BufferUsage::COPY_SRC) {
            flags |= vk::BufferUsageFlags::TRANSFER_SRC;
        }
        if usage.contains(BufferUsage::COPY_DST) {
            flags |= vk::BufferUsageFlags::TRANSFER_DST;
        }
        if usage.contains(BufferUsage::INDIRECT) {
            flags |= vk::BufferUsageFlags::INDIRECT_BUFFER;
        }
        flags
    }
}

impl Drop for VulkanDevice {
    fn drop(&mut self) {
        // Destroy cached framebuffers and render passes before device.
        if let Ok(mut cache) = self.framebuffer_cache.lock() {
            for (_, fb) in cache.drain() {
                unsafe {
                    self.device.destroy_framebuffer(fb, None);
                }
            }
        }
        if let Ok(mut cache) = self.render_pass_cache.lock() {
            for (_, rp) in cache.drain() {
                unsafe {
                    self.device.destroy_render_pass(rp, None);
                }
            }
        }
        if let Some(pool) = self.transfer_command_pool.take() {
            unsafe {
                self.device.destroy_command_pool(pool, None);
            }
        }
        #[cfg(feature = "window")]
        if let Some(ref s) = self.surface_state {
            unsafe {
                s.surface_loader.destroy_surface(s.surface, None);
            }
        }
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

impl std::fmt::Debug for VulkanDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDevice").finish_non_exhaustive()
    }
}

impl Device for VulkanDevice {
    fn create_buffer(&self, desc: &BufferDescriptor) -> Result<Box<dyn Buffer>, String> {
        let size = desc.size.max(1);
        let create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(Self::buffer_usage_to_vk(desc.usage))
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe {
            self.device
                .create_buffer(&create_info, None)
                .map_err(|e| e.to_string())?
        };
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let props = unsafe {
            self.instance.get_physical_device_memory_properties(self.physical_device)
        };
        let memory_type_index = match desc.memory {
            BufferMemoryPreference::HostVisible => (0..props.memory_type_count)
                .find(|i| {
                    let suitable = (requirements.memory_type_bits & (1 << i)) != 0;
                    let flags = &props.memory_types[*i as usize].property_flags;
                    suitable && flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT)
                })
                .unwrap_or(0) as u32,
            BufferMemoryPreference::DeviceLocal => (0..props.memory_type_count)
                .find(|i| {
                    let suitable = (requirements.memory_type_bits & (1 << i)) != 0;
                    let flags = &props.memory_types[*i as usize].property_flags;
                    suitable && flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
                })
                .unwrap_or(0) as u32,
        };
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe {
            self.device
                .allocate_memory(&allocate_info, None)
                .map_err(|e| e.to_string())?
        };
        unsafe {
            self.device
                .bind_buffer_memory(buffer, memory, 0)
                .map_err(|e| e.to_string())?;
        }
        let id = self.next_id();
        let host_visible = matches!(desc.memory, BufferMemoryPreference::HostVisible);
        Ok(Box::new(buffer::VulkanBuffer {
            device: Arc::clone(&self.device),
            buffer,
            memory,
            size,
            id,
            host_visible,
        }))
    }

    fn create_texture(&self, desc: &TextureDescriptor) -> Result<Box<dyn Texture>, String> {
        let tex = texture::create_texture(
            self.device.clone(),
            &self.instance,
            self.physical_device,
            desc,
            || self.next_id(),
        )?;
        Ok(Box::new(tex))
    }

    fn create_sampler(&self, desc: &SamplerDescriptor) -> Result<Box<dyn Sampler>, String> {
        let s = sampler::create_sampler(self.device.clone(), desc)?;
        Ok(Box::new(s))
    }

    fn create_compute_pipeline(
        &self,
        desc: &ComputePipelineDescriptor,
    ) -> Result<Box<dyn crate::ComputePipeline>, String> {
        let pipe = pipeline::VulkanComputePipeline::create(&self.device, desc)?;
        Ok(Box::new(pipe))
    }

    fn create_graphics_pipeline(
        &self,
        desc: &GraphicsPipelineDescriptor,
    ) -> Result<Box<dyn crate::GraphicsPipeline>, String> {
        let pipe = pipeline::VulkanGraphicsPipeline::create(&self.device, desc)?;
        Ok(Box::new(pipe))
    }

    fn create_descriptor_set_layout(
        &self,
        bindings: &[DescriptorSetLayoutBinding],
    ) -> Result<Box<dyn DescriptorSetLayout>, String> {
        let layout = descriptor::create_descriptor_set_layout(&self.device, bindings)?;
        Ok(Box::new(layout))
    }

    fn create_descriptor_pool(&self, max_sets: u32) -> Result<Box<dyn DescriptorPool>, String> {
        let pool = descriptor::create_descriptor_pool(&self.device, max_sets)?;
        Ok(Box::new(pool))
    }

    fn create_descriptor_pool_with_descriptor(
        &self,
        desc: &DescriptorPoolDescriptor,
    ) -> Result<Box<dyn DescriptorPool>, String> {
        let pool = descriptor::create_descriptor_pool_from_descriptor(&self.device, desc)?;
        Ok(Box::new(pool))
    }

    fn create_command_encoder(&self) -> Result<Box<dyn CommandEncoder>, String> {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let buffers = unsafe {
            self.device
                .allocate_command_buffers(&allocate_info)
                .map_err(|e| e.to_string())?
        };
        let cmd = buffers[0];
        unsafe {
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(cmd, &begin_info)
                .map_err(|e| e.to_string())?;
        }
        Ok(Box::new(VulkanCommandEncoder {
            device: Arc::clone(&self.device),
            command_pool: self.command_pool,
            buffer: cmd,
            finished: false,
            render_pass_cache: Arc::clone(&self.render_pass_cache),
            framebuffer_cache: Arc::clone(&self.framebuffer_cache),
        }))
    }

    fn write_buffer(&self, buffer: &dyn crate::Buffer, offset: u64, data: &[u8]) -> Result<(), String> {
        if !buffer.host_visible() {
            return Err("write_buffer requires a host-visible buffer; use upload_to_buffer for device-local buffers".to_string());
        }
        let vk_buf = buffer
            .as_any()
            .downcast_ref::<buffer::VulkanBuffer>()
            .ok_or("Buffer is not a Vulkan buffer")?;
        unsafe {
            let ptr = self
                .device
                .map_memory(
                    vk_buf.memory,
                    0,
                    vk::WHOLE_SIZE,
                    vk::MemoryMapFlags::empty(),
                )
                .map_err(|e| e.to_string())?;
            let dst = ptr.cast::<u8>().add(offset as usize);
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
            self.device.unmap_memory(vk_buf.memory);
        }
        Ok(())
    }

    fn upload_to_buffer(&self, buffer: &dyn crate::Buffer, offset: u64, data: &[u8]) -> Result<(), String> {
        if data.is_empty() {
            return Ok(());
        }
        if buffer.host_visible() {
            return self.write_buffer(buffer, offset, data);
        }
        let size = data.len() as u64;
        if offset + size > buffer.size() {
            return Err("upload_to_buffer: offset + data.len() exceeds buffer size".to_string());
        }
        let staging = self.create_buffer(&BufferDescriptor {
            label: Some("upload_staging"),
            size,
            usage: BufferUsage::COPY_SRC,
            memory: BufferMemoryPreference::HostVisible,
        })?;
        self.write_buffer(staging.as_ref(), 0, data)?;
        let mut encoder = self.create_command_encoder()?;
        encoder.copy_buffer_to_buffer(staging.as_ref(), 0, buffer, offset, size);
        let cmd = encoder.finish()?;
        self.submit(vec![cmd])?;
        self.wait_idle()?;
        Ok(())
    }

    fn submit(&self, command_buffers: Vec<Box<dyn CommandBuffer>>) -> Result<(), String> {
        let vk_buffers: Vec<vk::CommandBuffer> = command_buffers
            .iter()
            .filter_map(|b| b.as_any().downcast_ref::<VulkanCommandBuffer>().map(|vb| vb.buffer))
            .collect();
        if vk_buffers.is_empty() {
            return Ok(());
        }
        let submit_info = vk::SubmitInfo::default().command_buffers(&vk_buffers);
        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info], vk::Fence::null())
                .map_err(|e| format!("queue submit: {:?}", e))?;
        }
        Ok(())
    }

    fn queue(&self) -> Result<Box<dyn crate::Queue>, String> {
        Ok(Box::new(queue::VulkanQueue::new(
            self.device.clone(),
            self.queue,
        )))
    }

    fn transfer_queue(&self) -> Option<Box<dyn crate::Queue>> {
        self.transfer_queue.map(|q| {
            Box::new(queue::VulkanQueue::new(self.device.clone(), q)) as Box<dyn crate::Queue>
        })
    }

    fn upload_to_buffer_async(
        &self,
        buffer: &dyn crate::Buffer,
        offset: u64,
        data: &[u8],
        signal_fence: Option<&dyn Fence>,
    ) -> Result<(), String> {
        if data.is_empty() {
            return Ok(());
        }
        if buffer.host_visible() {
            return self.write_buffer(buffer, offset, data);
        }
        let size = data.len() as u64;
        if offset + size > buffer.size() {
            return Err("upload_to_buffer_async: offset + data.len() exceeds buffer size".to_string());
        }
        let staging = self.create_buffer(&BufferDescriptor {
            label: Some("upload_staging_async"),
            size,
            usage: BufferUsage::COPY_SRC,
            memory: BufferMemoryPreference::HostVisible,
        })?;
        self.write_buffer(staging.as_ref(), 0, data)?;
        let (submit_queue, pool) = match (self.transfer_queue, self.transfer_command_pool.as_ref()) {
            (Some(tq), Some(tpool)) => (tq, *tpool),
            _ => (self.queue, self.command_pool),
        };
        let cmd = Self::allocate_and_record_copy(
            Arc::clone(&self.device),
            pool,
            staging.as_ref(),
            0,
            buffer,
            offset,
            size,
        )?;
        let temp_fence: Option<VulkanFence> = if signal_fence.is_none() {
            let create_info = vk::FenceCreateInfo::default();
            let raw = unsafe { self.device.create_fence(&create_info, None).map_err(|e| e.to_string())? };
            Some(VulkanFence {
                device: Arc::clone(&self.device),
                fence: raw,
            })
        } else {
            None
        };
        let fence_for_submit: Option<&dyn Fence> = signal_fence.or_else(|| temp_fence.as_ref().map(|t| t as &dyn Fence));
        let queue_obj = queue::VulkanQueue::new(Arc::clone(&self.device), submit_queue);
        queue_obj.submit(&[&cmd], &[], &[], fence_for_submit)?;
        const TIMEOUT_NS: u64 = 10_000_000_000; // 10 s
        if let Some(ref f) = temp_fence {
            f.wait(TIMEOUT_NS)?;
        } else if let Some(f) = signal_fence {
            f.wait(TIMEOUT_NS)?;
        }
        Ok(())
    }

    fn submit_buffer_copy(
        &self,
        src: &dyn crate::Buffer,
        src_offset: u64,
        dst: &dyn crate::Buffer,
        dst_offset: u64,
        size: u64,
        signal_fence: Option<&dyn Fence>,
    ) -> Result<(), String> {
        if size == 0 {
            return Ok(());
        }
        let (submit_queue, pool) = match (self.transfer_queue, self.transfer_command_pool.as_ref()) {
            (Some(tq), Some(tpool)) => (tq, *tpool),
            _ => (self.queue, self.command_pool),
        };
        let cmd = Self::allocate_and_record_copy(
            Arc::clone(&self.device),
            pool,
            src,
            src_offset,
            dst,
            dst_offset,
            size,
        )?;
        let queue_obj = queue::VulkanQueue::new(Arc::clone(&self.device), submit_queue);
        queue_obj.submit(&[&cmd], &[], &[], signal_fence)?;
        Ok(())
    }

    fn wait_idle(&self) -> Result<(), String> {
        unsafe {
            self.device.queue_wait_idle(self.queue).map_err(|e| e.to_string())?;
            self.device.device_wait_idle().map_err(|e| e.to_string())
        }
    }

    fn create_fence(&self, signaled: bool) -> Result<Box<dyn Fence>, String> {
        let create_info = vk::FenceCreateInfo::default()
            .flags(if signaled { vk::FenceCreateFlags::SIGNALED } else { vk::FenceCreateFlags::empty() });
        let fence = unsafe {
            self.device
                .create_fence(&create_info, None)
                .map_err(|e| e.to_string())?
        };
        Ok(Box::new(VulkanFence {
            device: Arc::clone(&self.device),
            fence,
        }))
    }

    fn create_semaphore(&self) -> Result<Box<dyn Semaphore>, String> {
        let create_info = vk::SemaphoreCreateInfo::default();
        let semaphore = unsafe {
            self.device
                .create_semaphore(&create_info, None)
                .map_err(|e| e.to_string())?
        };
        Ok(Box::new(VulkanSemaphore {
            device: Arc::clone(&self.device),
            semaphore,
        }))
    }

    #[cfg(feature = "window")]
    fn create_swapchain(
        &self,
        extent: (u32, u32),
        old_swapchain: Option<&dyn crate::Swapchain>,
    ) -> Result<Box<dyn crate::Swapchain>, String> {
        let state = self
            .surface_state
            .as_ref()
            .ok_or("Device was created without a surface")?;
        let old_vk = old_swapchain.and_then(|s| {
            s.as_any()
                .downcast_ref::<swapchain::VulkanSwapchain>()
                .map(|vs| vs.swapchain)
        });
        let caps = unsafe {
            state
                .surface_loader
                .get_physical_device_surface_capabilities(self.physical_device, state.surface)
                .map_err(|e| format!("get_physical_device_surface_capabilities: {:?}", e))?
        };
        let (width, height) = extent;
        let extent_vk = vk::Extent2D {
            width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
            height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
        };
        let image_count = (caps.min_image_count + 1).min(caps.max_image_count).max(caps.min_image_count);
        let formats = unsafe {
            state
                .surface_loader
                .get_physical_device_surface_formats(self.physical_device, state.surface)
                .map_err(|e| format!("get_physical_device_surface_formats: {:?}", e))?
        };
        let format = formats
            .first()
            .copied()
            .unwrap_or(vk::SurfaceFormatKHR::default());
        let present_modes = unsafe {
            state
                .surface_loader
                .get_physical_device_surface_present_modes(self.physical_device, state.surface)
                .map_err(|e| format!("get_physical_device_surface_present_modes: {:?}", e))?
        };
        let present_mode = present_modes
            .iter()
            .copied()
            .find(|m| *m == vk::PresentModeKHR::MAILBOX)
            .or_else(|| present_modes.iter().copied().find(|m| *m == vk::PresentModeKHR::IMMEDIATE))
            .unwrap_or(vk::PresentModeKHR::FIFO);
        let mut create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(state.surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent_vk)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true);
        if let Some(old) = old_vk {
            create_info.old_swapchain = old;
        }
        let swapchain = unsafe {
            state
                .swapchain_loader
                .create_swapchain(&create_info, None)
                .map_err(|e| format!("create_swapchain: {:?}", e))?
        };
        let rhi_format = if format.format == vk::Format::B8G8R8A8_UNORM {
            crate::TextureFormat::Bgra8Unorm
        } else {
            crate::TextureFormat::Rgba8Unorm
        };
        let vulkan_swapchain = swapchain::VulkanSwapchain::new(
            Arc::clone(&self.device),
            state.swapchain_loader.clone(),
            swapchain,
            self.queue,
            (extent_vk.width, extent_vk.height),
            rhi_format,
            &self.next_id,
        )?;
        Ok(Box::new(vulkan_swapchain))
    }
}

struct VulkanCommandEncoder {
    device: Arc<ash::Device>,
    command_pool: vk::CommandPool,
    buffer: vk::CommandBuffer,
    finished: bool,
    render_pass_cache: Arc<Mutex<HashMap<RenderPassCacheKey, vk::RenderPass>>>,
    framebuffer_cache: Arc<Mutex<HashMap<FramebufferCacheKey, vk::Framebuffer>>>,
}

impl Drop for VulkanCommandEncoder {
    fn drop(&mut self) {
        if !self.finished {
            let _ = unsafe { self.device.end_command_buffer(self.buffer) };
        }
    }
}

impl std::fmt::Debug for VulkanCommandEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanCommandEncoder").finish()
    }
}

impl CommandEncoder for VulkanCommandEncoder {
    fn begin_compute_pass(&mut self) -> Box<dyn ComputePass> {
        Box::new(VulkanComputePass {
            device: self.device.clone(),
            buffer: self.buffer,
            pipeline_bound: None,
            pipeline_layout: None,
        })
    }

    fn begin_render_pass<'a>(&mut self, desc: RenderPassDescriptor<'a>) -> Result<Box<dyn crate::RenderPass>, String> {
        let color_infos: Vec<render_pass::ColorAttachmentInfo> = desc
            .color_attachments
            .iter()
            .map(|a| render_pass::ColorAttachmentInfo {
                format: a.texture.format(),
                load_op: a.load_op,
                store_op: a.store_op,
                initial_layout: a.initial_layout,
            })
            .collect();

        let depth_info = desc.depth_stencil_attachment.as_ref().map(|d| {
            render_pass::DepthAttachmentInfo {
                format: d.texture.format(),
                depth_load_op: d.depth_load_op,
                depth_store_op: d.depth_store_op,
            }
        });

        let rp_key = RenderPassCacheKey {
            color: color_infos
                .iter()
                .map(|a| (a.format, a.load_op, a.store_op, a.initial_layout))
                .collect(),
            depth: depth_info.as_ref().map(|d| (d.format, d.depth_load_op, d.depth_store_op)),
        };
        let vk_render_pass = {
            let mut cache = self.render_pass_cache.lock().map_err(|e| format!("render_pass_cache lock: {}", e))?;
            if let Some(&cached) = cache.get(&rp_key) {
                cached
            } else {
                let rp = render_pass::create_vk_render_pass(&self.device, &color_infos, depth_info.as_ref())
                    .map_err(|e| format!("create render pass: {}", e))?;
                cache.insert(rp_key.clone(), rp);
                cache.get(&rp_key).copied().unwrap()
            }
        };

        let mut image_views = Vec::new();
        for att in &desc.color_attachments {
            image_views.push(texture_to_image_view(att.texture)?);
        }
        if let Some(ref d) = desc.depth_stencil_attachment {
            image_views.push(texture_to_image_view(d.texture)?);
        }

        let (width, height, _) = desc
            .color_attachments
            .first()
            .map(|a| a.texture.size())
            .unwrap_or((1, 1, 1));

        let extent = vk::Extent2D {
            width,
            height,
        };

        let fb_key = FramebufferCacheKey {
            render_pass: vk_render_pass.as_raw(),
            width: extent.width,
            height: extent.height,
            attachment_views: image_views.iter().map(|v| v.as_raw()).collect(),
        };
        let framebuffer = {
            let mut cache = self.framebuffer_cache.lock().map_err(|e| format!("framebuffer_cache lock: {}", e))?;
            if let Some(&cached) = cache.get(&fb_key) {
                cached
            } else {
                let create_info = vk::FramebufferCreateInfo::default()
                    .render_pass(vk_render_pass)
                    .attachments(&image_views)
                    .width(extent.width)
                    .height(extent.height)
                    .layers(1);
                let fb = unsafe {
                    self.device
                        .create_framebuffer(&create_info, None)
                        .map_err(|e| format!("create framebuffer: {:?}", e))?
                };
                cache.insert(fb_key, fb);
                fb
            }
        };

        let mut clear_values: Vec<vk::ClearValue> = Vec::new();
        for att in &desc.color_attachments {
            let (r, g, b, a) = att.clear_value.as_ref().map_or(
                (0.0f32, 0.0, 0.0, 1.0),
                |cv| (cv.r, cv.g, cv.b, cv.a),
            );
            clear_values.push(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [r, g, b, a],
                },
            });
        }
        if let Some(ref d) = desc.depth_stencil_attachment {
            clear_values.push(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: d.clear_depth,
                    stencil: 0,
                },
            });
        }

        let render_pass_begin = vk::RenderPassBeginInfo::default()
            .render_pass(vk_render_pass)
            .framebuffer(framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            })
            .clear_values(&clear_values);

        unsafe {
            self.device.cmd_begin_render_pass(
                self.buffer,
                &render_pass_begin,
                vk::SubpassContents::INLINE,
            );
        }

        let recorder = render_pass::VulkanRenderPassRecorder::new(
            Arc::clone(&self.device),
            self.buffer,
            vk_render_pass,
            framebuffer,
            extent,
        );

        Ok(Box::new(recorder))
    }

    fn copy_buffer_to_buffer(
        &mut self,
        src: &dyn Buffer,
        src_offset: u64,
        dst: &dyn Buffer,
        dst_offset: u64,
        size: u64,
    ) {
        let src_buf = src.as_any().downcast_ref::<buffer::VulkanBuffer>().expect("src must be VulkanBuffer");
        let dst_buf = dst.as_any().downcast_ref::<buffer::VulkanBuffer>().expect("dst must be VulkanBuffer");
        let region = vk::BufferCopy::default()
            .src_offset(src_offset)
            .dst_offset(dst_offset)
            .size(size);
        unsafe {
            self.device.cmd_copy_buffer(
                self.buffer,
                src_buf.buffer,
                dst_buf.buffer,
                &[region],
            );
        }
    }

    fn pipeline_barrier_texture(
        &mut self,
        texture: &dyn Texture,
        old_layout: ImageLayout,
        new_layout: ImageLayout,
    ) {
        #[cfg(feature = "window")]
        let image = if let Some(t) = texture.as_any().downcast_ref::<VulkanTexture>() {
            t.image
        } else if let Some(s) = texture.as_any().downcast_ref::<VulkanSwapchainImage>() {
            s.image
        } else {
            panic!("texture must be VulkanTexture or VulkanSwapchainImage");
        };
        #[cfg(not(feature = "window"))]
        let image = texture.as_any().downcast_ref::<VulkanTexture>().expect("texture must be VulkanTexture").image;
        let (old_l, new_l) = (
            image_layout_to_vk(old_layout),
            image_layout_to_vk(new_layout),
        );
        let is_depth = matches!(texture.format(), TextureFormat::D32Float);
        let aspect_mask = if is_depth {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        };
        let (src_stage, src_access, dst_stage, dst_access) = image_barrier_stages_access(
            old_layout,
            new_layout,
            is_depth,
        );
        let barrier = vk::ImageMemoryBarrier::default()
            .old_layout(old_l)
            .new_layout(new_l)
            .image(image)
            .src_access_mask(src_access)
            .dst_access_mask(dst_access)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(aspect_mask)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            );
        unsafe {
            self.device.cmd_pipeline_barrier(
                self.buffer,
                src_stage,
                dst_stage,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
        }
    }

    fn pipeline_barrier_buffer(
        &mut self,
        buffer: &dyn crate::Buffer,
        offset: u64,
        size: u64,
    ) {
        let vk_buf = buffer
            .as_any()
            .downcast_ref::<buffer::VulkanBuffer>()
            .expect("Buffer must be VulkanBuffer");
        let size = if size == 0 {
            buffer.size().saturating_sub(offset)
        } else {
            size
        };
        if size == 0 {
            return;
        }
        let barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(vk_buf.buffer)
            .offset(offset)
            .size(size);
        unsafe {
            self.device.cmd_pipeline_barrier(
                self.buffer,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[barrier],
                &[],
            );
        }
    }

    fn copy_buffer_to_texture(
        &mut self,
        src: &dyn Buffer,
        src_offset: u64,
        dst: &dyn Texture,
        dst_mip: u32,
        dst_origin: (u32, u32, u32),
        size: (u32, u32, u32),
    ) {
        let src_buf = src.as_any().downcast_ref::<buffer::VulkanBuffer>().expect("src must be VulkanBuffer");
        let dst_tex = dst.as_any().downcast_ref::<VulkanTexture>().expect("dst must be VulkanTexture");
        let (width, height, depth) = size;
        let image_subresource = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(dst_mip)
            .base_array_layer(0)
            .layer_count(1);
        let image_offset = vk::Offset3D {
            x: dst_origin.0 as i32,
            y: dst_origin.1 as i32,
            z: dst_origin.2 as i32,
        };
        let image_extent = vk::Extent3D {
            width,
            height,
            depth,
        };
        let region = vk::BufferImageCopy::default()
            .buffer_offset(src_offset)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(image_subresource)
            .image_offset(image_offset)
            .image_extent(image_extent);
        unsafe {
            self.device.cmd_copy_buffer_to_image(
                self.buffer,
                src_buf.buffer,
                dst_tex.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );
        }
    }

    fn finish(mut self: Box<Self>) -> Result<Box<dyn CommandBuffer>, String> {
        unsafe {
            self.device
                .end_command_buffer(self.buffer)
                .map_err(|e| format!("end command buffer: {:?}", e))?;
        }
        self.finished = true;
        Ok(Box::new(VulkanCommandBuffer {
            device: Arc::clone(&self.device),
            command_pool: self.command_pool,
            buffer: self.buffer,
        }))
    }
}

struct VulkanComputePass {
    device: Arc<ash::Device>,
    buffer: vk::CommandBuffer,
    pipeline_bound: Option<vk::Pipeline>,
    pipeline_layout: Option<vk::PipelineLayout>,
}

impl std::fmt::Debug for VulkanComputePass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanComputePass").finish()
    }
}

impl ComputePass for VulkanComputePass {
    fn set_pipeline(&mut self, pipeline: &dyn crate::ComputePipeline) {
        if let Some(vk_pipe) = pipeline.as_any().downcast_ref::<pipeline::VulkanComputePipeline>() {
            unsafe {
                self.device.cmd_bind_pipeline(
                    self.buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    vk_pipe.pipeline,
                );
            }
            self.pipeline_bound = Some(vk_pipe.pipeline);
            self.pipeline_layout = Some(vk_pipe.layout);
        }
    }

    fn bind_descriptor_set(&mut self, set_index: u32, set: &dyn crate::DescriptorSet) {
        if let Some(vk_set) = set.as_any().downcast_ref::<descriptor::VulkanDescriptorSet>() {
            if let Some(layout) = self.pipeline_layout {
                unsafe {
                    self.device.cmd_bind_descriptor_sets(
                        self.buffer,
                        vk::PipelineBindPoint::COMPUTE,
                        layout,
                        set_index,
                        &[vk_set.set],
                        &[],
                    );
                }
            }
        }
    }

    fn dispatch(&mut self, x: u32, y: u32, z: u32) {
        unsafe {
            self.device.cmd_dispatch(self.buffer, x, y, z);
        }
    }

    fn dispatch_indirect(&mut self, buffer: &dyn crate::Buffer, offset: u64) {
        let vk_buf = buffer
            .as_any()
            .downcast_ref::<buffer::VulkanBuffer>()
            .expect("Buffer must be VulkanBuffer");
        unsafe {
            self.device.cmd_dispatch_indirect(self.buffer, vk_buf.buffer, offset);
        }
    }
}

pub struct VulkanCommandBuffer {
    device: Arc<ash::Device>,
    command_pool: vk::CommandPool,
    buffer: vk::CommandBuffer,
}

impl Drop for VulkanCommandBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.free_command_buffers(self.command_pool, &[self.buffer]);
        }
    }
}

impl std::fmt::Debug for VulkanCommandBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanCommandBuffer").finish()
    }
}

impl CommandBuffer for VulkanCommandBuffer {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) struct VulkanFence {
    device: Arc<ash::Device>,
    fence: vk::Fence,
}

impl Drop for VulkanFence {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_fence(self.fence, None);
        }
    }
}

impl std::fmt::Debug for VulkanFence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanFence").finish()
    }
}

impl Fence for VulkanFence {
    fn wait(&self, timeout_ns: u64) -> Result<(), String> {
        unsafe {
            self.device.wait_for_fences(&[self.fence], true, timeout_ns).map_err(|e| e.to_string())
        }
    }

    fn reset(&self) -> Result<(), String> {
        unsafe {
            self.device.reset_fences(&[self.fence]).map_err(|e| e.to_string())
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) struct VulkanSemaphore {
    device: Arc<ash::Device>,
    semaphore: vk::Semaphore,
}

impl Drop for VulkanSemaphore {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_semaphore(self.semaphore, None);
        }
    }
}

impl std::fmt::Debug for VulkanSemaphore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanSemaphore").finish()
    }
}

impl Semaphore for VulkanSemaphore {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
