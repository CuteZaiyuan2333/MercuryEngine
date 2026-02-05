//! Vulkan Buffer implementation.

use crate::{Buffer, ResourceId};
use ash::vk;
use std::sync::Arc;

pub struct VulkanBuffer {
    pub device: Arc<ash::Device>,
    pub buffer: vk::Buffer,
    pub memory: vk::DeviceMemory,
    pub size: u64,
    pub id: ResourceId,
}

impl Drop for VulkanBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

impl std::fmt::Debug for VulkanBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanBuffer")
            .field("id", &self.id)
            .field("size", &self.size)
            .finish()
    }
}

impl Buffer for VulkanBuffer {
    fn id(&self) -> ResourceId {
        self.id
    }
    fn size(&self) -> u64 {
        self.size
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
