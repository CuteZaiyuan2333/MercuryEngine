//! Lumelite plugin: implements RenderBackend for the host.

use std::sync::Arc;
use render_api::{ExtractedMeshes, ExtractedView, RenderBackend};
use lumelite_renderer::{LumeliteConfig, MeshDraw, Renderer};

/// Build orthographic projection (column-major): left, right, bottom, top, near, far.
fn ortho(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> [f32; 16] {
    let sx = 2.0 / (right - left);
    let sy = 2.0 / (top - bottom);
    let sz = -2.0 / (far - near);
    let tx = -(right + left) / (right - left);
    let ty = -(top + bottom) / (top - bottom);
    let tz = -(far + near) / (far - near);
    [
        sx, 0.0, 0.0, 0.0,
        0.0, sy, 0.0, 0.0,
        0.0, 0.0, sz, 0.0,
        tx, ty, tz, 1.0,
    ]
}

fn look_at(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let f = [center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]];
    let len_f = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
    let f = [f[0] / len_f, f[1] / len_f, f[2] / len_f];
    let s = [f[1] * up[2] - f[2] * up[1], f[2] * up[0] - f[0] * up[2], f[0] * up[1] - f[1] * up[0]];
    let len_s = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
    let s = [s[0] / len_s, s[1] / len_s, s[2] / len_s];
    let u = [s[1] * f[2] - s[2] * f[1], s[2] * f[0] - s[0] * f[2], s[0] * f[1] - s[1] * f[0]];
    [
        s[0], s[1], s[2], -(s[0] * eye[0] + s[1] * eye[1] + s[2] * eye[2]),
        u[0], u[1], u[2], -(u[0] * eye[0] + u[1] * eye[1] + u[2] * eye[2]),
        -f[0], -f[1], -f[2], f[0] * eye[0] + f[1] * eye[1] + f[2] * eye[2],
        0.0, 0.0, 0.0, 1.0,
    ]
}

fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut c = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            c[col * 4 + row] = a[row] * b[col * 4 + 0]
                + a[4 + row] * b[col * 4 + 1]
                + a[8 + row] * b[col * 4 + 2]
                + a[12 + row] * b[col * 4 + 3];
        }
    }
    c
}

/// Build light view-projection for shadow map (orthographic, directional light).
fn build_light_view_proj(direction: [f32; 3]) -> [f32; 16] {
    let dist = 20.0;
    let dir = {
        let len = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
        if len > 1e-6 {
            [direction[0] / len, direction[1] / len, direction[2] / len]
        } else {
            [0.0, -1.0, 0.0]
        }
    };
    let eye = [-dir[0] * dist, -dir[1] * dist, -dir[2] * dist];
    let view = look_at(eye, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let proj = ortho(-10.0, 10.0, -10.0, 10.0, 0.1, 50.0);
    mat4_mul(&proj, &view)
}

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
        let light_view_proj = if self.renderer.config().shadow_enabled {
            let lvp = build_light_view_proj(directional_light.0);
            Some(lvp)
        } else {
            None
        };
        if self.renderer.config().debug_direct_triangle {
            if let Some(sv) = swapchain_view {
                self.renderer.encode_direct_triangle(&mut encoder, sv, &meshes, &view.view_proj)?;
            }
        } else {
            self.renderer.encode_frame(
                &mut encoder,
                width,
                height,
                &view.view_proj,
                &inv_view_proj,
                &meshes,
                directional_light,
                &view.point_lights,
                &view.spot_lights,
                light_view_proj.as_ref(),
            )?;
            if let Some(sv) = swapchain_view {
                self.renderer.encode_present_to(&mut encoder, sv)?;
            }
        }
        let cmd = encoder.finish();
        self.renderer.submit([cmd]);
        Ok(())
    }
}
