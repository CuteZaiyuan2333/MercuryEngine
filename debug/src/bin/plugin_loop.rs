//! Host loop: use render_api types and LumelitePlugin as RenderBackend (prepare + render_frame).

use std::collections::HashMap;
use render_api::{ExtractedMeshes, ExtractedView, RenderBackend};

fn main() -> Result<(), String> {
    let (device, queue) = pollster::block_on(request_device());
    let mut backend: Box<dyn RenderBackend> = Box::new(lumelite_bridge::LumelitePlugin::new(device, queue)?);

    // One frame: one triangle (model-space vertices: pos + normal, 6 f32 per vertex = 24 bytes)
    let vertex_data: Vec<u8> = bytemuck::cast_slice(&[
        0.0f32, 0.5, 0.0, 0.0, 1.0, 0.0,
        -0.5, -0.5, 0.0, 0.0, 1.0, 0.0,
        0.5, -0.5, 0.0, 0.0, 1.0, 0.0,
    ]).to_vec();
    let index_data: Vec<u8> = bytemuck::cast_slice(&[0u32, 1u32, 2u32]).to_vec();
    let identity: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let mut meshes = HashMap::new();
    meshes.insert(
        1u64,
        render_api::ExtractedMesh {
            entity_id: 1,
            vertex_data: vertex_data.clone(),
            index_data: index_data.clone(),
            transform: identity,
            visible: true,
        },
    );
    let extracted = ExtractedMeshes { meshes };
    let view = ExtractedView {
        view_proj: identity,
        viewport_size: (800, 600),
        directional_light: None,
    };

    backend.prepare(&extracted);
    backend.render_frame(&view)?;
    println!("Lumelite plugin_loop: one frame OK");
    Ok(())
}

async fn request_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("No adapter");
    adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .expect("No device")
}
