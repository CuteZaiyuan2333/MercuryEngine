//! Vulkan Texture: full implementation with VkImage, memory, and ImageView.

use crate::{ResourceId, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsage};
use ash::vk;
use std::sync::Arc;

/// Create a Vulkan texture from descriptor.
pub fn create_texture(
    device: Arc<ash::Device>,
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    descriptor: &TextureDescriptor,
    next_id: impl FnOnce() -> ResourceId,
) -> Result<VulkanTexture, String> {
    let (width, height, depth_or_layers) = descriptor.size;
    let extent = vk::Extent3D {
        width: width.max(1),
        height: height.max(1),
        depth: depth_or_layers.max(1),
    };

    let vk_format = texture_format_to_vk(descriptor.format);
    let usage_flags = texture_usage_to_vk(descriptor.usage, descriptor.format);
    let image_type = texture_dimension_to_image_type(descriptor.dimension);

    let mut array_layers = 1u32;
    let mut flags = vk::ImageCreateFlags::empty();
    match descriptor.dimension {
        TextureDimension::D2 => {
            array_layers = 1;
        }
        TextureDimension::D2Array => {
            array_layers = depth_or_layers.max(1);
        }
        TextureDimension::D3 => {
            // depth is depth
        }
        TextureDimension::Cube => {
            array_layers = 6;
            flags = vk::ImageCreateFlags::CUBE_COMPATIBLE;
        }
    }

    let mip_levels = descriptor.mip_level_count.max(1);

    let create_info = vk::ImageCreateInfo::default()
        .image_type(image_type)
        .format(vk_format)
        .extent(extent)
        .mip_levels(mip_levels)
        .array_layers(array_layers)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage_flags)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .flags(flags);

    let image = unsafe {
        device
            .create_image(&create_info, None)
            .map_err(|e| e.to_string())?
    };

    let requirements = unsafe { device.get_image_memory_requirements(image) };
    let memory_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    let memory_type_index = (0..memory_props.memory_type_count)
        .find(|i| {
            let suitable = (requirements.memory_type_bits & (1 << i)) != 0;
            let mem_type = &memory_props.memory_types[*i as usize];
            let device_local = mem_type
                .property_flags
                .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);
            suitable && device_local
        })
        .ok_or("No suitable device-local memory for texture")? as u32;

    let allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index);

    let memory = unsafe {
        device
            .allocate_memory(&allocate_info, None)
            .map_err(|e| e.to_string())?
    };

    unsafe {
        device
            .bind_image_memory(image, memory, 0)
            .map_err(|e| e.to_string())?;
    }

    let view_type = texture_dimension_to_view_type(descriptor.dimension, descriptor.size);
    let aspect_mask = if format_is_depth(descriptor.format) {
        vk::ImageAspectFlags::DEPTH
    } else {
        vk::ImageAspectFlags::COLOR
    };

    let view_create_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(view_type)
        .format(vk_format)
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(aspect_mask)
                .base_mip_level(0)
                .level_count(mip_levels)
                .base_array_layer(0)
                .layer_count(array_layers),
        );

    let view = unsafe {
        device
            .create_image_view(&view_create_info, None)
            .map_err(|e| e.to_string())?
    };

    Ok(VulkanTexture {
        device,
        image,
        memory,
        view,
        format: descriptor.format,
        size: descriptor.size,
        dimension: descriptor.dimension,
        mip_level_count: mip_levels,
        id: next_id(),
        image_type,
    })
}

/// Fully implemented Vulkan texture with image, memory, and view.
pub struct VulkanTexture {
    pub(crate) device: Arc<ash::Device>,
    pub(crate) image: vk::Image,
    pub(crate) memory: vk::DeviceMemory,
    pub(crate) view: vk::ImageView,
    pub(crate) format: TextureFormat,
    pub(crate) size: (u32, u32, u32),
    pub(crate) dimension: TextureDimension,
    pub(crate) mip_level_count: u32,
    pub(crate) id: ResourceId,
    #[allow(dead_code)]
    pub(crate) image_type: vk::ImageType,
}

impl VulkanTexture {
    pub fn image(&self) -> vk::Image {
        self.image
    }

    pub fn view(&self) -> vk::ImageView {
        self.view
    }

    pub fn current_layout(&self) -> vk::ImageLayout {
        // Layout is tracked per-use; for simplicity we expose UNDEFINED as initial.
        // Caller should transition via barrier before use.
        vk::ImageLayout::UNDEFINED
    }
}

impl Drop for VulkanTexture {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

impl std::fmt::Debug for VulkanTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanTexture")
            .field("id", &self.id)
            .field("size", &self.size)
            .field("format", &self.format)
            .field("dimension", &self.dimension)
            .finish()
    }
}

impl Texture for VulkanTexture {
    fn id(&self) -> ResourceId {
        self.id
    }
    fn format(&self) -> TextureFormat {
        self.format
    }
    fn size(&self) -> (u32, u32, u32) {
        self.size
    }
    fn dimension(&self) -> TextureDimension {
        self.dimension
    }
    fn mip_level_count(&self) -> u32 {
        self.mip_level_count
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub fn texture_format_to_vk(format: TextureFormat) -> vk::Format {
    match format {
        TextureFormat::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
        TextureFormat::Bgra8Unorm => vk::Format::B8G8R8A8_UNORM,
        TextureFormat::R32Float => vk::Format::R32_SFLOAT,
        TextureFormat::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
        TextureFormat::D32Float => vk::Format::D32_SFLOAT,
        TextureFormat::R16Float => vk::Format::R16_SFLOAT,
        TextureFormat::Rgba32Float => vk::Format::R32G32B32A32_SFLOAT,
    }
}

pub fn texture_usage_to_vk(usage: TextureUsage, format: TextureFormat) -> vk::ImageUsageFlags {
    let mut flags = vk::ImageUsageFlags::empty();
    if usage.contains(TextureUsage::COPY_SRC) {
        flags |= vk::ImageUsageFlags::TRANSFER_SRC;
    }
    if usage.contains(TextureUsage::COPY_DST) {
        flags |= vk::ImageUsageFlags::TRANSFER_DST;
    }
    if usage.contains(TextureUsage::TEXTURE_BINDING) {
        flags |= vk::ImageUsageFlags::SAMPLED;
    }
    if usage.contains(TextureUsage::STORAGE_BINDING) {
        flags |= vk::ImageUsageFlags::STORAGE;
    }
    if usage.contains(TextureUsage::RENDER_ATTACHMENT) {
        if format_is_depth(format) {
            flags |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
        } else {
            flags |= vk::ImageUsageFlags::COLOR_ATTACHMENT;
        }
    }
    flags
}

fn format_is_depth(format: TextureFormat) -> bool {
    matches!(format, TextureFormat::D32Float)
}

pub fn texture_dimension_to_image_type(dim: TextureDimension) -> vk::ImageType {
    match dim {
        TextureDimension::D2 | TextureDimension::D2Array | TextureDimension::Cube => {
            vk::ImageType::TYPE_2D
        }
        TextureDimension::D3 => vk::ImageType::TYPE_3D,
    }
}

pub fn texture_dimension_to_view_type(dim: TextureDimension, _size: (u32, u32, u32)) -> vk::ImageViewType {
    match dim {
        TextureDimension::D2 => vk::ImageViewType::TYPE_2D,
        TextureDimension::D2Array => vk::ImageViewType::TYPE_2D_ARRAY,
        TextureDimension::D3 => vk::ImageViewType::TYPE_3D,
        TextureDimension::Cube => vk::ImageViewType::CUBE,
    }
}
