//! Vulkan swapchain and surface support (feature "window").

use crate::{
    ResourceId, Semaphore, Swapchain, SwapchainFrame, Texture, TextureDimension, TextureFormat,
};
use ash::vk;
use ash::khr::swapchain::Device as SwapchainDevice;
use std::sync::Arc;

use super::texture::texture_format_to_vk;
use super::VulkanSemaphore;

/// Swapchain image wrapper: implements Texture for use as color attachment. Does not own the VkImage (swapchain does).
pub struct VulkanSwapchainImage {
    pub(crate) device: Arc<ash::Device>,
    pub(crate) image: vk::Image,
    pub(crate) view: vk::ImageView,
    pub(crate) format: TextureFormat,
    pub(crate) extent: (u32, u32),
    pub(crate) id: ResourceId,
}

impl VulkanSwapchainImage {
    pub fn view(&self) -> vk::ImageView {
        self.view
    }
}

impl Drop for VulkanSwapchainImage {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image_view(self.view, None);
            // Do not destroy image - owned by swapchain
        }
    }
}

impl std::fmt::Debug for VulkanSwapchainImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanSwapchainImage")
            .field("id", &self.id)
            .field("extent", &self.extent)
            .finish()
    }
}

impl Texture for VulkanSwapchainImage {
    fn id(&self) -> ResourceId {
        self.id
    }
    fn format(&self) -> TextureFormat {
        self.format
    }
    fn size(&self) -> (u32, u32, u32) {
        (self.extent.0, self.extent.1, 1)
    }
    fn dimension(&self) -> TextureDimension {
        TextureDimension::D2
    }
    fn mip_level_count(&self) -> u32 {
        1
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct VulkanSwapchain {
    device: Arc<ash::Device>,
    swapchain_loader: SwapchainDevice,
    swapchain: vk::SwapchainKHR,
    images: Vec<VulkanSwapchainImage>,
    queue: vk::Queue,
    extent: (u32, u32),
}

impl VulkanSwapchain {
    pub fn new(
        device: Arc<ash::Device>,
        swapchain_loader: SwapchainDevice,
        swapchain: vk::SwapchainKHR,
        queue: vk::Queue,
        extent: (u32, u32),
        format: TextureFormat,
        next_id: &std::sync::atomic::AtomicU64,
    ) -> Result<Self, String> {
        let vk_images = unsafe {
            swapchain_loader
                .get_swapchain_images(swapchain)
                .map_err(|e| format!("get_swapchain_images: {:?}", e))?
        };
        let vk_format = texture_format_to_vk(format);
        let mut images = Vec::with_capacity(vk_images.len());
        for image in vk_images {
            let view_create_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk_format)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            let view = unsafe {
                device
                    .create_image_view(&view_create_info, None)
                    .map_err(|e| format!("create_image_view: {:?}", e))?
            };
            let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            images.push(VulkanSwapchainImage {
                device: Arc::clone(&device),
                image,
                view,
                format,
                extent,
                id,
            });
        }
        Ok(Self {
            device,
            swapchain_loader,
            swapchain,
            images,
            queue,
            extent,
        })
    }
}

impl Drop for VulkanSwapchain {
    fn drop(&mut self) {
        self.images.clear(); // destroy image views
        unsafe {
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
        }
    }
}

impl std::fmt::Debug for VulkanSwapchain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanSwapchain")
            .field("extent", &self.extent)
            .field("image_count", &self.images.len())
            .finish()
    }
}

impl Swapchain for VulkanSwapchain {
    fn acquire_next_image(&mut self, wait_semaphore: Option<&dyn Semaphore>) -> Result<SwapchainFrame<'_>, String> {
        let (semaphore, _) = wait_semaphore
            .map(|s| {
                let vk_s = s.as_any().downcast_ref::<VulkanSemaphore>().map(|vs| vs.semaphore);
                (vk_s, ())
            })
            .unwrap_or((None, ()));
        let sem = semaphore.unwrap_or(vk::Semaphore::null());
        let (index, _suboptimal) = unsafe {
            self.swapchain_loader
                .acquire_next_image(self.swapchain, u64::MAX, sem, vk::Fence::null())
                .map_err(|e| format!("acquire_next_image: {:?}", e))?
        };
        let texture = &self.images[index as usize];
        Ok(SwapchainFrame {
            image_index: index,
            texture,
        })
    }

    fn present(&self, image_index: u32, wait_semaphore: Option<&dyn Semaphore>) -> Result<(), String> {
        let semaphore = wait_semaphore.and_then(|s| {
            s.as_any().downcast_ref::<VulkanSemaphore>().map(|vs| vs.semaphore)
        });
        let wait_semas: Vec<vk::Semaphore> = semaphore.into_iter().collect();
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&wait_semas)
            .swapchains(std::slice::from_ref(&self.swapchain))
            .image_indices(&image_indices);
        unsafe {
            self.swapchain_loader
                .queue_present(self.queue, &present_info)
                .map_err(|e| format!("queue_present: {:?}", e))?;
        }
        Ok(())
    }

    fn extent(&self) -> (u32, u32) {
        self.extent
    }
}
