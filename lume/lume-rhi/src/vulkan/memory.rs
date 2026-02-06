//! Vulkan memory management: explicit heaps and device-local allocations.
//! Provides foundation for VG cluster streaming and GI SDF textures.

use ash::vk;
use std::sync::Arc;

/// Memory heap for sub-allocations. Manages a large device allocation.
/// Used by streaming/upload paths (VG/GI); reserved for future use.
#[allow(dead_code)]
pub struct VulkanMemoryHeap {
    pub device: Arc<ash::Device>,
    pub memory: vk::DeviceMemory,
    pub size: u64,
    pub memory_type_index: u32,
}

#[allow(dead_code)]
impl VulkanMemoryHeap {
    /// Create a memory heap for sub-allocations. `memory_type_bits` is the mask from buffer/image memory requirements;
    /// `prefer_device_local` selects a device-local type when possible.
    pub fn new(
        device: Arc<ash::Device>,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        size: u64,
        memory_type_bits: u32,
        prefer_device_local: bool,
    ) -> Result<Self, String> {
        let props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let memory_type_index = (0..props.memory_type_count)
            .find(|i| {
                let suitable = (memory_type_bits & (1 << i)) != 0;
                let mt = &props.memory_types[*i as usize];
                let has_device_local = mt.property_flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);
                suitable && (!prefer_device_local || has_device_local)
            })
            .or_else(|| {
                (0..props.memory_type_count).find(|i| (memory_type_bits & (1 << i)) != 0)
            })
            .ok_or("No suitable memory type for heap")? as u32;

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
