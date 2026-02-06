//! Vulkan Sampler implementation.

use crate::{AddressMode, FilterMode, Sampler, SamplerDescriptor};
use ash::vk;
use std::sync::Arc;

fn filter_to_vk(f: FilterMode) -> vk::Filter {
    match f {
        FilterMode::Nearest => vk::Filter::NEAREST,
        FilterMode::Linear => vk::Filter::LINEAR,
    }
}

fn address_mode_to_vk(a: AddressMode) -> vk::SamplerAddressMode {
    match a {
        AddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
        AddressMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        AddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        AddressMode::ClampToBorder => vk::SamplerAddressMode::CLAMP_TO_BORDER,
    }
}

pub fn create_sampler(
    device: Arc<ash::Device>,
    desc: &SamplerDescriptor,
) -> Result<VulkanSampler, String> {
    let anisotropy = desc.anisotropy_clamp.map(|c| c.clamp(1.0, 16.0));
    let create_info = vk::SamplerCreateInfo::default()
        .mag_filter(filter_to_vk(desc.mag_filter))
        .min_filter(filter_to_vk(desc.min_filter))
        .address_mode_u(address_mode_to_vk(desc.address_mode_u))
        .address_mode_v(address_mode_to_vk(desc.address_mode_v))
        .address_mode_w(address_mode_to_vk(desc.address_mode_w))
        .anisotropy_enable(anisotropy.is_some())
        .max_anisotropy(anisotropy.unwrap_or(1.0))
        .unnormalized_coordinates(false);
    let sampler = unsafe {
        device
            .create_sampler(&create_info, None)
            .map_err(|e| e.to_string())?
    };
    Ok(VulkanSampler { device, sampler })
}

pub struct VulkanSampler {
    pub device: Arc<ash::Device>,
    pub sampler: vk::Sampler,
}

impl VulkanSampler {
    pub fn raw(&self) -> vk::Sampler {
        self.sampler
    }
}

impl Drop for VulkanSampler {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}

impl std::fmt::Debug for VulkanSampler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanSampler").finish()
    }
}

impl Sampler for VulkanSampler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
