//! Minimal wgpu init (no window). Verifies lumelite-renderer and wgpu work.

fn main() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("No adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("No device");
        let _renderer = lumelite_renderer::Renderer::new(device, queue).expect("Renderer::new");
        println!("Lumelite minimal_wgpu: OK");
    });
}
