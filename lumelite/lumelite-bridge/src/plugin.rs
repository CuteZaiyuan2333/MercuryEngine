//! Lumelite plugin: implements RenderBackend for the host.

use std::sync::Arc;
use render_api::{ExtractedMeshes, ExtractedView, RenderBackend};
use lumelite_renderer::{LumeliteConfig, MeshDraw, Renderer};

/// Invert 4x4 matrix (column-major). Returns None if singular.
fn invert_view_proj(m: &[f32; 16]) -> Option<[f32; 16]> {
    let mut inv = [0.0f32; 16];
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15] + m[9] * m[7] * m[14] + m[13] * m[6] * m[11] - m[13] * m[7] * m[10];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15] - m[9] * m[3] * m[14] - m[13] * m[2] * m[11] + m[13] * m[3] * m[10];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15] + m[5] * m[3] * m[14] + m[13] * m[2] * m[7] - m[13] * m[3] * m[6];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11] - m[5] * m[3] * m[10] - m[9] * m[2] * m[7] + m[9] * m[3] * m[6];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15] - m[8] * m[7] * m[14] - m[12] * m[6] * m[11] + m[12] * m[7] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15] + m[8] * m[3] * m[14] + m[12] * m[2] * m[11] - m[12] * m[3] * m[10];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15] - m[4] * m[3] * m[14] - m[12] * m[2] * m[7] + m[12] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11] + m[4] * m[3] * m[10] + m[8] * m[2] * m[7] - m[8] * m[3] * m[6];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15] + m[8] * m[7] * m[13] + m[12] * m[5] * m[11] - m[12] * m[7] * m[9];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15] - m[8] * m[3] * m[13] - m[12] * m[1] * m[11] + m[12] * m[3] * m[9];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15] + m[4] * m[3] * m[13] + m[12] * m[1] * m[7] - m[12] * m[3] * m[5];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11] - m[4] * m[3] * m[9] - m[8] * m[1] * m[7] + m[8] * m[3] * m[5];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14] - m[8] * m[6] * m[13] - m[12] * m[5] * m[10] + m[12] * m[6] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14] + m[8] * m[2] * m[13] + m[12] * m[1] * m[10] - m[12] * m[2] * m[9];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14] - m[4] * m[2] * m[13] - m[12] * m[1] * m[6] + m[12] * m[2] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10] + m[4] * m[2] * m[9] + m[8] * m[1] * m[6] - m[8] * m[2] * m[5];
    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-10 {
        return None;
    }
    let s = 1.0 / det;
    for x in &mut inv {
        *x *= s;
    }
    Some(inv)
}

/// Cached GPU buffers and world transform for one mesh.
struct CachedMesh {
    vertex_buf: Arc<wgpu::Buffer>,
    index_buf: Arc<wgpu::Buffer>,
    index_count: u32,
    vertex_len: usize,
    index_len: usize,
    transform: [f32; 16],
}

/// Lumelite plugin: owns the wgpu device/queue and renderer; implements RenderBackend.
pub struct LumelitePlugin {
    renderer: Renderer,
    /// Cache by entity_id. Updated in prepare() from ExtractedMeshes.
    mesh_cache: std::collections::HashMap<u64, CachedMesh>,
}

impl LumelitePlugin {
    /// Create with wgpu device and queue (default config).
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, String> {
        Self::new_with_config(device, queue, LumeliteConfig::default())
    }

    /// Create with config (swapchain format, max lights, shadow, tone mapping).
    pub fn new_with_config(device: wgpu::Device, queue: wgpu::Queue, config: LumeliteConfig) -> Result<Self, String> {
        let renderer = Renderer::new_with_config(device, queue, config)?;
        Ok(Self { renderer, mesh_cache: std::collections::HashMap::new() })
    }

    /// Access device/queue if the host needs them (e.g. for swapchain).
    pub fn device(&self) -> &wgpu::Device {
        self.renderer.device()
    }
    pub fn queue(&self) -> &wgpu::Queue {
        self.renderer.queue()
    }
    pub fn renderer(&self) -> &Renderer {
        &self.renderer
    }
}

impl RenderBackend for LumelitePlugin {
    fn prepare(&mut self, extracted: &ExtractedMeshes) {
        let device = self.renderer.device();
        let queue = self.renderer.queue();
        let current_entities: std::collections::HashSet<u64> =
            extracted.meshes.keys().copied().collect();
        self.mesh_cache.retain(|k, _| current_entities.contains(k));
        for (&entity_id, mesh) in &extracted.meshes {
            if !mesh.visible || mesh.vertex_data.is_empty() || mesh.index_data.is_empty() {
                continue;
            }
            let vertex_len = mesh.vertex_data.len();
            let index_len = mesh.index_data.len();
            let index_count = (index_len / 4) as u32;
            if let Some(cached) = self.mesh_cache.get_mut(&entity_id) {
                if cached.vertex_len == vertex_len && cached.index_len == index_len {
                    queue.write_buffer(&cached.vertex_buf, 0, &mesh.vertex_data);
                    queue.write_buffer(&cached.index_buf, 0, &mesh.index_data);
                    cached.transform = mesh.transform;
                    continue;
                }
            }
            let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("lumelite_mesh_vertex"),
                size: vertex_len as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&vertex_buf, 0, &mesh.vertex_data);
            let index_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("lumelite_mesh_index"),
                size: index_len as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&index_buf, 0, &mesh.index_data);
            self.mesh_cache.insert(
                entity_id,
                CachedMesh {
                    vertex_buf: Arc::new(vertex_buf),
                    index_buf: Arc::new(index_buf),
                    index_count,
                    vertex_len,
                    index_len,
                    transform: mesh.transform,
                },
            );
        }
    }

    fn render_frame(&mut self, view: &ExtractedView) -> Result<(), String> {
        self.render_frame_impl(view, None)
    }
}

impl LumelitePlugin {
    /// Render one frame and present to swapchain (tone-mapped blit from light buffer). Use this when displaying in a window.
    pub fn render_frame_to_swapchain(
        &mut self,
        view: &ExtractedView,
        swapchain_view: &wgpu::TextureView,
    ) -> Result<(), String> {
        self.render_frame_impl(view, Some(swapchain_view))
    }

    fn render_frame_impl(
        &mut self,
        view: &ExtractedView,
        swapchain_view: Option<&wgpu::TextureView>,
    ) -> Result<(), String> {
        let meshes: Vec<MeshDraw> = self
            .mesh_cache
            .values()
            .map(|c| MeshDraw {
                vertex_buf: Arc::clone(&c.vertex_buf),
                index_buf: Arc::clone(&c.index_buf),
                index_count: c.index_count,
                transform: c.transform,
            })
            .collect();
        let (width, height) = view.viewport_size;
        let directional_light = view.directional_light
            .unwrap_or(([0.3f32, -0.8, 0.5], [1.0, 1.0, 1.0]));
        let inv_view_proj = invert_view_proj(&view.view_proj).unwrap_or([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        let device = self.renderer.device();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("lumelite_plugin_frame"),
        });
        self.renderer.encode_frame(
            &mut encoder,
            width,
            height,
            &view.view_proj,
            &inv_view_proj,
            &meshes,
            directional_light,
        )?;
        if let Some(sv) = swapchain_view {
            self.renderer.encode_present_to(&mut encoder, sv)?;
        }
        let cmd = encoder.finish();
        self.renderer.submit([cmd]);
        Ok(())
    }
}
