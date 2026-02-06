//! Vulkan Queue for non-blocking submit.

use crate::{CommandBuffer, Fence, Queue, Semaphore};
use ash::vk;
use std::sync::Arc;

pub struct VulkanQueue {
    pub device: Arc<ash::Device>,
    pub queue: vk::Queue,
}

impl VulkanQueue {
    pub fn new(device: Arc<ash::Device>, queue: vk::Queue) -> Self {
        Self { device, queue }
    }
}

impl std::fmt::Debug for VulkanQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanQueue").finish()
    }
}

impl Queue for VulkanQueue {
    fn submit(
        &self,
        command_buffers: &[&dyn CommandBuffer],
        wait_semaphores: &[&dyn Semaphore],
        signal_semaphores: &[&dyn Semaphore],
        signal_fence: Option<&dyn Fence>,
    ) -> Result<(), String> {
        let vk_buffers: Vec<vk::CommandBuffer> = command_buffers
            .iter()
            .filter_map(|b| {
                b.as_any()
                    .downcast_ref::<super::VulkanCommandBuffer>()
                    .map(|vb| vb.buffer)
            })
            .collect();
        if vk_buffers.is_empty() {
            return Ok(());
        }

        let wait_semas: Vec<vk::Semaphore> = wait_semaphores
            .iter()
            .filter_map(|s| {
                s.as_any()
                    .downcast_ref::<super::VulkanSemaphore>()
                    .map(|vs| vs.semaphore)
            })
            .collect();
        let signal_semas: Vec<vk::Semaphore> = signal_semaphores
            .iter()
            .filter_map(|s| {
                s.as_any()
                    .downcast_ref::<super::VulkanSemaphore>()
                    .map(|vs| vs.semaphore)
            })
            .collect();

        let fence = signal_fence.and_then(|f| {
            f.as_any()
                .downcast_ref::<super::VulkanFence>()
                .map(|vf| vf.fence)
        }).unwrap_or(vk::Fence::null());

        // Wait at color attachment output so the swapchain image is ready before we write to it.
        let wait_stages = if wait_semas.is_empty() {
            vec![]
        } else {
            vec![vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT; wait_semas.len()]
        };

        let submit_info = vk::SubmitInfo::default()
            .command_buffers(&vk_buffers)
            .wait_semaphores(if wait_semas.is_empty() { &[] } else { &wait_semas })
            .wait_dst_stage_mask(if wait_stages.is_empty() { &[] } else { &wait_stages })
            .signal_semaphores(if signal_semas.is_empty() { &[] } else { &signal_semas });

        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info], fence)
                .map_err(|e| format!("queue submit: {:?}", e))?;
        }
        Ok(())
    }
}
