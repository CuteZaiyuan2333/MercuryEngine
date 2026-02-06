//! Minimal runnable example: Lume RHI only. No direct Vulkan/Metal API calls.
//! Creates a device (backend-agnostic), buffer, fence, submits an empty command buffer, and exits.

fn main() {
    let device = lume_rhi::create_device(lume_rhi::DeviceCreateParams::default())
        .expect("create_device");
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
    println!("Lume RHI OK");
}
