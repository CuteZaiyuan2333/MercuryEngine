//! Vulkan memory management: explicit heaps and device-local allocations.
//! Provides foundation for VG cluster streaming and GI SDF textures.

use ash::vk;
use std::sync::Arc;

/// Memory heap for sub-allocations. Manages a large device allocation.
pub struct VulkanMemoryHeap {
    pub device: Arc<ash::Device>,
    pub memory: vk::DeviceMemory,
    pub size: u64,
    pub memory_type_index: u32,
}

impl VulkanMemoryHeap {
    /// Create a device-local memory heap of the given size.
    pub fn new(
        device: Arc<ash::Device>,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        size: u64,
        device_local: bool,
    ) -> Result<Self, String> {
        let props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let memory_type_index = (0..props.memory_type_count)
            .find(|i| {
                let mt = &props.memory_types[*i as usize];
                let suitable = (1u64 << i) != 0; // Any memory type for now
                let has_device_local = !device_local
                    || mt.property_flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);
                suitable && has_device_local
            })
            .ok_or("No suitable memory type")? as u32;

        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(size)
            .memory_type_index(memory_type_index);

        let memory = unsafe {
            device
                .allocate_memory(&allocate_info, None)
                .map_err(|e| format!("{:?}", e))?
        };

        Ok(Self {
            device,
            memory,
            size,
            memory_type_index,
        })
    }
}

impl std::fmt::Debug for VulkanMemoryHeap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanMemoryHeap")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl Drop for VulkanMemoryHeap {
    fn drop(&mut self) {
        unsafe {
            self.device.free_memory(self.memory, None);
        }
    }
}
