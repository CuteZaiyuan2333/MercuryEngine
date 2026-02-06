//! Vulkan Descriptor Set Layout, Pool, and Set.

use crate::{
    Buffer, DescriptorPool, DescriptorPoolDescriptor, DescriptorSet, DescriptorSetLayout,
    DescriptorSetLayoutBinding, DescriptorType, Sampler, ShaderStages, Texture,
};
use ash::vk;

/// Returns the VkImageView for a texture, supporting both VulkanTexture and VulkanSwapchainImage
/// (when feature "window" is enabled), so that swapchain images can be bound as sampled textures
/// e.g. for post-process or temporal accumulation.
fn texture_view_for_descriptor(texture: &dyn Texture) -> Result<vk::ImageView, String> {
    if let Some(t) = texture.as_any().downcast_ref::<super::texture::VulkanTexture>() {
        return Ok(t.view);
    }
    #[cfg(feature = "window")]
    if let Some(s) = texture.as_any().downcast_ref::<super::swapchain::VulkanSwapchainImage>() {
        return Ok(s.view());
    }
    #[cfg(not(feature = "window"))]
    return Err("Texture must be VulkanTexture; enable 'window' feature to bind swapchain images".to_string());
    #[cfg(feature = "window")]
    Err("Texture must be VulkanTexture or VulkanSwapchainImage".to_string())
}

pub struct VulkanDescriptorSetLayout {
    pub device: ash::Device,
    pub layout: vk::DescriptorSetLayout,
    /// Bindings used to create this layout; used by descriptor sets to know descriptor type per binding.
    pub bindings: Vec<DescriptorSetLayoutBinding>,
}

impl VulkanDescriptorSetLayout {
    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.layout
    }

    pub fn bindings(&self) -> &[DescriptorSetLayoutBinding] {
        &self.bindings
    }
}

impl Drop for VulkanDescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}

impl std::fmt::Debug for VulkanDescriptorSetLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDescriptorSetLayout").finish()
    }
}

impl DescriptorSetLayout for VulkanDescriptorSetLayout {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub fn descriptor_type_to_vk(t: DescriptorType) -> vk::DescriptorType {
    match t {
        DescriptorType::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
        DescriptorType::StorageBuffer => vk::DescriptorType::STORAGE_BUFFER,
        DescriptorType::StorageImage => vk::DescriptorType::STORAGE_IMAGE,
        DescriptorType::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
        DescriptorType::CombinedImageSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
    }
}

pub fn create_descriptor_set_layout(
    device: &ash::Device,
    bindings: &[DescriptorSetLayoutBinding],
) -> Result<VulkanDescriptorSetLayout, String> {
    let vk_bindings: Vec<vk::DescriptorSetLayoutBinding> = bindings
        .iter()
        .map(|b| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(b.binding)
                .descriptor_type(descriptor_type_to_vk(b.descriptor_type))
                .descriptor_count(b.count)
                .stage_flags(shader_stages_to_vk(b.stages))
        })
        .collect();
    let create_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&vk_bindings);
    let layout = unsafe {
        device
            .create_descriptor_set_layout(&create_info, None)
            .map_err(|e| format!("{:?}", e))?
    };
    Ok(VulkanDescriptorSetLayout {
        device: device.clone(),
        layout,
        bindings: bindings.to_vec(),
    })
}

const DEFAULT_POOL_MULTIPLIER: u32 = 4;

pub fn create_descriptor_pool(device: &ash::Device, max_sets: u32) -> Result<VulkanDescriptorPool, String> {
    create_descriptor_pool_from_descriptor(device, &DescriptorPoolDescriptor {
        max_sets,
        pool_sizes: Vec::new(),
    })
}

pub fn create_descriptor_pool_from_descriptor(
    device: &ash::Device,
    desc: &DescriptorPoolDescriptor,
) -> Result<VulkanDescriptorPool, String> {
    let default_per_type = desc.max_sets * DEFAULT_POOL_MULTIPLIER;
    let types_and_defaults: [(DescriptorType, u32); 5] = [
        (DescriptorType::UniformBuffer, default_per_type),
        (DescriptorType::StorageBuffer, default_per_type),
        (DescriptorType::StorageImage, default_per_type),
        (DescriptorType::SampledImage, default_per_type),
        (DescriptorType::CombinedImageSampler, default_per_type),
    ];
    let pool_sizes: Vec<vk::DescriptorPoolSize> = if desc.pool_sizes.is_empty() {
        types_and_defaults
            .iter()
            .map(|(ty, count)| {
                vk::DescriptorPoolSize::default()
                    .ty(descriptor_type_to_vk(*ty))
                    .descriptor_count(*count)
            })
            .collect()
    } else {
        types_and_defaults
            .iter()
            .map(|(ty, default_count)| {
                let count = desc
                    .pool_sizes
                    .iter()
                    .find(|(t, _)| t == ty)
                    .map(|(_, c)| *c)
                    .unwrap_or(*default_count);
                vk::DescriptorPoolSize::default()
                    .ty(descriptor_type_to_vk(*ty))
                    .descriptor_count(count)
            })
            .collect()
    };
    let create_info = vk::DescriptorPoolCreateInfo::default()
        .max_sets(desc.max_sets)
        .pool_sizes(&pool_sizes);
    let pool = unsafe {
        device
            .create_descriptor_pool(&create_info, None)
            .map_err(|e| format!("{:?}", e))?
    };
    Ok(VulkanDescriptorPool {
        device: device.clone(),
        pool,
        max_sets: desc.max_sets,
    })
}

pub fn shader_stages_to_vk(s: ShaderStages) -> vk::ShaderStageFlags {
    let mut flags = vk::ShaderStageFlags::empty();
    if s.contains(ShaderStages::VERTEX) {
        flags |= vk::ShaderStageFlags::VERTEX;
    }
    if s.contains(ShaderStages::FRAGMENT) {
        flags |= vk::ShaderStageFlags::FRAGMENT;
    }
    if s.contains(ShaderStages::COMPUTE) {
        flags |= vk::ShaderStageFlags::COMPUTE;
    }
    flags
}

pub struct VulkanDescriptorPool {
    pub device: ash::Device,
    pub pool: vk::DescriptorPool,
    pub max_sets: u32,
}

impl VulkanDescriptorPool {
    pub fn pool(&self) -> vk::DescriptorPool {
        self.pool
    }
}

impl Drop for VulkanDescriptorPool {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.pool, None);
        }
    }
}

impl std::fmt::Debug for VulkanDescriptorPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDescriptorPool").finish()
    }
}

impl DescriptorPool for VulkanDescriptorPool {
    fn allocate_set(&self, layout: &dyn DescriptorSetLayout) -> Result<Box<dyn DescriptorSet>, String> {
        let vk_layout = layout
            .as_any()
            .downcast_ref::<VulkanDescriptorSetLayout>()
            .ok_or("Layout must be VulkanDescriptorSetLayout")?;
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(std::slice::from_ref(&vk_layout.layout));
        let sets = unsafe {
            self.device
                .allocate_descriptor_sets(&alloc_info)
                .map_err(|e| format!("{:?}", e))?
        };
        Ok(Box::new(VulkanDescriptorSet {
            device: self.device.clone(),
            set: sets[0],
            bindings: vk_layout.bindings().to_vec(),
        }))
    }
}

pub struct VulkanDescriptorSet {
    pub device: ash::Device,
    pub set: vk::DescriptorSet,
    /// Copy of layout bindings so write_buffer/write_texture use correct descriptor type.
    bindings: Vec<DescriptorSetLayoutBinding>,
}

impl std::fmt::Debug for VulkanDescriptorSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDescriptorSet").finish()
    }
}

impl VulkanDescriptorSet {
    fn descriptor_type_for_binding(&self, binding: u32) -> Option<DescriptorType> {
        self.bindings
            .iter()
            .find(|b| b.binding == binding)
            .map(|b| b.descriptor_type)
    }
}

impl DescriptorSet for VulkanDescriptorSet {
    fn write_buffer(&mut self, binding: u32, buffer: &dyn Buffer, offset: u64, size: u64) -> Result<(), String> {
        self.write_buffer_at(binding, 0, buffer, offset, size)
    }

    fn write_texture(&mut self, binding: u32, texture: &dyn Texture) -> Result<(), String> {
        self.write_texture_at(binding, 0, texture)
    }

    fn write_sampled_image(&mut self, binding: u32, texture: &dyn Texture, sampler: &dyn Sampler) -> Result<(), String> {
        self.write_sampled_image_at(binding, 0, texture, sampler)
    }

    fn write_buffer_at(
        &mut self,
        binding: u32,
        array_element: u32,
        buffer: &dyn Buffer,
        offset: u64,
        size: u64,
    ) -> Result<(), String> {
        let descriptor_type = self
            .descriptor_type_for_binding(binding)
            .ok_or("write_buffer_at: binding not found in layout")?;
        let vk_ty = descriptor_type_to_vk(descriptor_type);
        let vk_buf = buffer
            .as_any()
            .downcast_ref::<super::buffer::VulkanBuffer>()
            .ok_or("Buffer must be VulkanBuffer")?;
        let buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(vk_buf.buffer)
            .offset(offset)
            .range(if size > 0 { size } else { buffer.size() - offset });
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(binding)
            .dst_array_element(array_element)
            .descriptor_type(vk_ty)
            .buffer_info(std::slice::from_ref(&buffer_info));
        unsafe {
            self.device.update_descriptor_sets(&[write], &[]);
        }
        Ok(())
    }

    fn write_texture_at(&mut self, binding: u32, array_element: u32, texture: &dyn Texture) -> Result<(), String> {
        let descriptor_type = self
            .descriptor_type_for_binding(binding)
            .ok_or("write_texture_at: binding not found in layout")?;
        let vk_ty = descriptor_type_to_vk(descriptor_type);
        let image_view = texture_view_for_descriptor(texture)?;
        let image_info = vk::DescriptorImageInfo::default()
            .image_view(image_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(binding)
            .dst_array_element(array_element)
            .descriptor_type(vk_ty)
            .image_info(std::slice::from_ref(&image_info));
        unsafe {
            self.device.update_descriptor_sets(&[write], &[]);
        }
        Ok(())
    }

    fn write_sampled_image_at(
        &mut self,
        binding: u32,
        array_element: u32,
        texture: &dyn Texture,
        sampler: &dyn Sampler,
    ) -> Result<(), String> {
        let descriptor_type = self
            .descriptor_type_for_binding(binding)
            .ok_or("write_sampled_image_at: binding not found in layout")?;
        let vk_ty = descriptor_type_to_vk(descriptor_type);
        let image_view = texture_view_for_descriptor(texture)?;
        let vk_sampler = sampler
            .as_any()
            .downcast_ref::<super::sampler::VulkanSampler>()
            .ok_or("Sampler must be VulkanSampler")?;
        let image_info = vk::DescriptorImageInfo::default()
            .image_view(image_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .sampler(vk_sampler.sampler);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(binding)
            .dst_array_element(array_element)
            .descriptor_type(vk_ty)
            .image_info(std::slice::from_ref(&image_info));
        unsafe {
            self.device.update_descriptor_sets(&[write], &[]);
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
