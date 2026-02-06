//! Minimal runnable example: Lume + Vulkan only. No Bevy, no WGPU.
//! Creates a Vulkan device, buffer, fence, submits an empty command buffer, and exits.

use lume_rhi::Device;

fn main() {
    let device = lume_rhi::VulkanDevice::new().expect("VulkanDevice::new");
    let _buffer = device.create_buffer(&lume_rhi::BufferDescriptor {
        label: Some("minimal"),
        size: 256,
        usage: lume_rhi::BufferUsage::STORAGE,
        memory: lume_rhi::BufferMemoryPreference::HostVisible,
    }).expect("create_buffer");
    let _fence = device.create_fence(false).expect("create_fence");
    let _sem = device.create_semaphore().expect("create_semaphore");
    let encoder = device.create_command_encoder().expect("create_command_encoder");
    let cmd = encoder.finish().expect("finish");
    device.submit(vec![cmd]).expect("submit");
    device.wait_idle().expect("wait_idle");
    println!("Lume + Vulkan OK");
}
