//! Vulkan backend for Lume RHI.
//! Implements Device, Buffer, Texture, ComputePipeline, GraphicsPipeline, CommandEncoder, Fence, Semaphore.

mod buffer;
mod descriptor;
mod memory;
mod pipeline;
mod queue;
mod render_pass;
mod texture;

#[cfg(feature = "window")]
mod swapchain;

use crate::{
    Buffer, BufferDescriptor, BufferUsage, CommandBuffer, CommandEncoder, ComputePass,
    ComputePipelineDescriptor, DescriptorSetLayoutBinding, DescriptorPool, DescriptorSetLayout,
    Device, Fence, GraphicsPipelineDescriptor, ImageLayout, RenderPassDescriptor, ResourceId,
    Semaphore, Texture, TextureDescriptor,
};
use ash::vk;
use std::ffi::CString;
use std::sync::Arc;

pub use buffer::VulkanBuffer;
pub use descriptor::{VulkanDescriptorPool, VulkanDescriptorSet, VulkanDescriptorSetLayout};
pub use pipeline::{VulkanComputePipeline, VulkanGraphicsPipeline};
pub use render_pass::{ColorAttachmentInfo, DepthAttachmentInfo};
pub use texture::{create_texture as create_vulkan_texture, VulkanTexture};

#[cfg(feature = "window")]
pub use swapchain::{VulkanSwapchain, VulkanSwapchainImage};

#[cfg(feature = "window")]
fn texture_to_image_view(texture: &dyn crate::Texture) -> vk::ImageView {
    if let Some(t) = texture.as_any().downcast_ref::<VulkanTexture>() {
        return t.view;
    }
    if let Some(s) = texture.as_any().downcast_ref::<VulkanSwapchainImage>() {
        return s.view();
    }
    panic!("color attachment texture must be VulkanTexture or VulkanSwapchainImage");
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
    next_id: std::sync::atomic::AtomicU64,
    #[cfg(feature = "window")]
    surface_state: Option<SurfaceState>,
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
    }
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
        let instance_create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info);
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
        let queue_priorities = [1.0f32];
        let queue_create_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priorities);
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_create_info));
        let device_raw = unsafe {
            instance.create_device(physical_device, &device_create_info, None).map_err(|e| e.to_string())?
        };
        let queue = unsafe { device_raw.get_device_queue(queue_family_index, 0) };
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
            next_id: std::sync::atomic::AtomicU64::new(1),
            #[cfg(feature = "window")]
            surface_state: None,
        }))
    }

    fn next_id(&self) -> ResourceId {
        self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(feature = "window")]
    /// Create a Vulkan device with a window surface for swapchain/presentation.
    pub fn new_with_surface(
        window: &impl raw_window_handle::HasRawWindowHandle,
    ) -> Result<Arc<Self>, String> {
        use ash::khr::surface::Instance as SurfaceInstance;
        use ash::khr::swapchain::Device as SwapchainDevice;
        use std::ffi::CStr;
        let raw = window.raw_window_handle().map_err(|e| format!("raw_window_handle: {:?}", e))?;
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
        let instance_create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&ext_names);
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
        let queue_priorities = [1.0f32];
        let queue_create_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priorities);
        let swapchain_ext = ash::khr::swapchain::NAME.as_ptr();
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_create_info))
            .enabled_extension_names(std::slice::from_ref(&swapchain_ext));
        let device_raw = unsafe {
            instance.create_device(physical_devices[0], &device_create_info, None).map_err(|e| e.to_string())?
        };
        let queue = unsafe { device_raw.get_device_queue(queue_family_index, 0) };
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
            next_id: std::sync::atomic::AtomicU64::new(1),
            surface_state: Some(SurfaceState {
                surface,
                surface_loader,
                swapchain_loader,
            }),
        }))
    }

    fn buffer_usage_to_vk(usage: BufferUsage) -> vk::BufferUsageFlags {
        let mut flags = vk::BufferUsageFlags::empty();
        if matches!(usage, BufferUsage::Vertex) {
            flags |= vk::BufferUsageFlags::VERTEX_BUFFER;
        }
        if matches!(usage, BufferUsage::Index) {
            flags |= vk::BufferUsageFlags::INDEX_BUFFER;
        }
        if matches!(usage, BufferUsage::Uniform) {
            flags |= vk::BufferUsageFlags::UNIFORM_BUFFER;
        }
        if matches!(usage, BufferUsage::Storage) {
            flags |= vk::BufferUsageFlags::STORAGE_BUFFER;
        }
        if matches!(usage, BufferUsage::CopySrc) {
            flags |= vk::BufferUsageFlags::TRANSFER_SRC;
        }
        if matches!(usage, BufferUsage::CopyDst) {
            flags |= vk::BufferUsageFlags::TRANSFER_DST;
        }
        if matches!(usage, BufferUsage::Indirect) {
            flags |= vk::BufferUsageFlags::INDIRECT_BUFFER;
        }
        flags
    }
}

impl Drop for VulkanDevice {
    fn drop(&mut self) {
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
    fn create_buffer(&self, desc: &BufferDescriptor) -> Box<dyn Buffer> {
        let size = desc.size.max(1);
        let create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(Self::buffer_usage_to_vk(desc.usage))
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe {
            self.device.create_buffer(&create_info, None).expect("create buffer")
        };
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let memory_type_index = {
            let props = unsafe {
                self.instance.get_physical_device_memory_properties(self.physical_device)
            };
            (0..props.memory_type_count)
                .find(|i| {
                    let suitable = (requirements.memory_type_bits & (1 << i)) != 0;
                    let host_visible = props.memory_types[*i as usize].property_flags
                        .contains(vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT);
                    suitable && host_visible
                })
                .unwrap_or(0) as u32
        };
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe {
            self.device.allocate_memory(&allocate_info, None).expect("allocate buffer memory")
        };
        unsafe {
            self.device.bind_buffer_memory(buffer, memory, 0).expect("bind buffer memory");
        }
        let id = self.next_id();
        Box::new(buffer::VulkanBuffer {
            device: Arc::clone(&self.device),
            buffer,
            memory,
            size,
            id,
        })
    }

    fn create_texture(&self, desc: &TextureDescriptor) -> Box<dyn Texture> {
        match texture::create_texture(
            self.device.clone(),
            &self.instance,
            self.physical_device,
            desc,
            || self.next_id(),
        ) {
            Ok(tex) => Box::new(tex),
            Err(e) => panic!("create_texture failed: {}", e),
        }
    }

    fn create_compute_pipeline(&self, desc: &ComputePipelineDescriptor) -> Box<dyn crate::ComputePipeline> {
        let pipe = pipeline::VulkanComputePipeline::create(&self.device, desc)
            .expect("create compute pipeline");
        Box::new(pipe)
    }

    fn create_graphics_pipeline(&self, desc: &GraphicsPipelineDescriptor) -> Box<dyn crate::GraphicsPipeline> {
        let pipe = pipeline::VulkanGraphicsPipeline::create(&self.device, desc)
            .expect("create graphics pipeline");
        Box::new(pipe)
    }

    fn create_descriptor_set_layout(&self, bindings: &[DescriptorSetLayoutBinding]) -> Box<dyn DescriptorSetLayout> {
        let layout = descriptor::create_descriptor_set_layout(&self.device, bindings)
            .expect("create descriptor set layout");
        Box::new(layout)
    }

    fn create_descriptor_pool(&self, max_sets: u32) -> Box<dyn DescriptorPool> {
        let pool = descriptor::create_descriptor_pool(&self.device, max_sets)
            .expect("create descriptor pool");
        Box::new(pool)
    }

    fn create_command_encoder(&self) -> Box<dyn CommandEncoder> {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let buffers = unsafe {
            self.device.allocate_command_buffers(&allocate_info).expect("allocate command buffer")
        };
        let cmd = buffers[0];
        unsafe {
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device.begin_command_buffer(cmd, &begin_info).expect("begin command buffer");
        }
        Box::new(VulkanCommandEncoder {
            device: Arc::clone(&self.device),
            command_pool: self.command_pool,
            buffer: cmd,
            finished: false,
        })
    }

    fn write_buffer(&self, buffer: &dyn crate::Buffer, offset: u64, data: &[u8]) -> Result<(), String> {
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

    fn submit(&self, command_buffers: Vec<Box<dyn CommandBuffer>>) {
        let vk_buffers: Vec<vk::CommandBuffer> = command_buffers
            .iter()
            .filter_map(|b| b.as_any().downcast_ref::<VulkanCommandBuffer>().map(|vb| vb.buffer))
            .collect();
        if vk_buffers.is_empty() {
            return;
        }
        let submit_info = vk::SubmitInfo::default().command_buffers(&vk_buffers);
        unsafe {
            self.device.queue_submit(self.queue, &[submit_info], vk::Fence::null()).expect("queue submit");
        }
    }

    fn queue(&self) -> Box<dyn crate::Queue> {
        Box::new(queue::VulkanQueue::new(
            self.device.clone(),
            self.queue,
        ))
    }

    fn wait_idle(&self) -> Result<(), String> {
        unsafe {
            self.device.queue_wait_idle(self.queue).map_err(|e| e.to_string())?;
            self.device.device_wait_idle().map_err(|e| e.to_string())
        }
    }

    fn create_fence(&self, signaled: bool) -> Box<dyn Fence> {
        let create_info = vk::FenceCreateInfo::default()
            .flags(if signaled { vk::FenceCreateFlags::SIGNALED } else { vk::FenceCreateFlags::empty() });
        let fence = unsafe {
            self.device.create_fence(&create_info, None).expect("create fence")
        };
        Box::new(VulkanFence {
            device: Arc::clone(&self.device),
            fence,
        })
    }

    fn create_semaphore(&self) -> Box<dyn Semaphore> {
        let create_info = vk::SemaphoreCreateInfo::default();
        let semaphore = unsafe {
            self.device.create_semaphore(&create_info, None).expect("create semaphore")
        };
        Box::new(VulkanSemaphore {
            device: Arc::clone(&self.device),
            semaphore,
        })
    }

    #[cfg(feature = "window")]
    fn create_swapchain(&self, extent: (u32, u32)) -> Result<Box<dyn crate::Swapchain>, String> {
        let state = self
            .surface_state
            .as_ref()
            .ok_or("Device was created without a surface")?;
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
        let create_info = vk::SwapchainCreateInfoKHR::default()
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

    fn begin_render_pass<'a>(&mut self, desc: RenderPassDescriptor<'a>) -> Box<dyn crate::RenderPass> {
        let color_infos: Vec<render_pass::ColorAttachmentInfo> = desc
            .color_attachments
            .iter()
            .map(|a| render_pass::ColorAttachmentInfo {
                format: a.texture.format(),
                load_op: a.load_op,
                store_op: a.store_op,
            })
            .collect();

        let depth_info = desc.depth_stencil_attachment.as_ref().map(|d| {
            render_pass::DepthAttachmentInfo {
                format: d.texture.format(),
                depth_load_op: d.depth_load_op,
                depth_store_op: d.depth_store_op,
            }
        });

        let vk_render_pass = render_pass::create_vk_render_pass(
            &self.device,
            &color_infos,
            depth_info.as_ref(),
        )
        .expect("create render pass");

        let mut image_views = Vec::new();
        for att in &desc.color_attachments {
            #[cfg(feature = "window")]
            let view = texture_to_image_view(att.texture);
            #[cfg(not(feature = "window"))]
            let view = att.texture.as_any().downcast_ref::<VulkanTexture>().expect("texture must be VulkanTexture").view;
            image_views.push(view);
        }
        if let Some(ref d) = desc.depth_stencil_attachment {
            #[cfg(feature = "window")]
            let view = texture_to_image_view(d.texture);
            #[cfg(not(feature = "window"))]
            let view = d.texture.as_any().downcast_ref::<VulkanTexture>().expect("texture must be VulkanTexture").view;
            image_views.push(view);
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

        let framebuffer_create_info = vk::FramebufferCreateInfo::default()
            .render_pass(vk_render_pass)
            .attachments(&image_views)
            .width(extent.width)
            .height(extent.height)
            .layers(1);

        let framebuffer = unsafe {
            self.device
                .create_framebuffer(&framebuffer_create_info, None)
                .expect("create framebuffer")
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

        Box::new(recorder)
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
        let barrier = vk::ImageMemoryBarrier::default()
            .old_layout(old_l)
            .new_layout(new_l)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            );
        unsafe {
            self.device.cmd_pipeline_barrier(
                self.buffer,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
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

    fn finish(mut self: Box<Self>) -> Box<dyn CommandBuffer> {
        unsafe {
            self.device.end_command_buffer(self.buffer).expect("end command buffer");
        }
        self.finished = true;
        Box::new(VulkanCommandBuffer {
            device: Arc::clone(&self.device),
            command_pool: self.command_pool,
            buffer: self.buffer,
        })
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
