//! 后端无关示例：仅依赖 render-api + LumeliteWindowBackend，宿主不直接调用 wgpu。
//! Run: cargo run -p debug --bin gbuffer_light_window

use std::collections::HashMap;
use render_api::{ExtractedMeshes, ExtractedView, RenderBackendWindow};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

/// Build perspective projection matrix (column-major, WebGPU NDC z in [0,1]).
/// View space: -Z forward, maps -near->NDC 0, -far->NDC 1.
fn perspective_projection(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let t = (fov_y_rad / 2.0).tan();
    let sy = 1.0 / t;
    let sx = sy / aspect;
    let a = far / (near - far);
    let b = (near * far) / (near - far);
    [
        sx, 0.0, 0.0, 0.0,
        0.0, sy, 0.0, 0.0,
        0.0, 0.0, a, -1.0,
        0.0, 0.0, b, 0.0,
    ]
}

/// Build look-at view matrix (column-major). Camera at eye looking at center.
fn look_at(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let f = [
        center[0] - eye[0],
        center[1] - eye[1],
        center[2] - eye[2],
    ];
    let len_f = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
    let f = [f[0] / len_f, f[1] / len_f, f[2] / len_f];
    let s = [
        f[1] * up[2] - f[2] * up[1],
        f[2] * up[0] - f[0] * up[2],
        f[0] * up[1] - f[1] * up[0],
    ];
    let len_s = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
    let s = [s[0] / len_s, s[1] / len_s, s[2] / len_s];
    let u = [
        s[1] * f[2] - s[2] * f[1],
        s[2] * f[0] - s[0] * f[2],
        s[0] * f[1] - s[1] * f[0],
    ];
    // View matrix (column-major): right, up, -forward, translation
    [
        s[0], s[1], s[2], -(s[0] * eye[0] + s[1] * eye[1] + s[2] * eye[2]),
        u[0], u[1], u[2], -(u[0] * eye[0] + u[1] * eye[1] + u[2] * eye[2]),
        -f[0], -f[1], -f[2], f[0] * eye[0] + f[1] * eye[1] + f[2] * eye[2],
        0.0, 0.0, 0.0, 1.0,
    ]
}

/// Multiply two 4x4 column-major matrices: C = A * B.
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

struct App {
    window: Option<winit::window::Window>,
    /// 后端：通过 render-api 的 RenderBackendWindow 渲染，不持有任何 wgpu 类型
    backend: Option<Box<dyn RenderBackendWindow>>,
    size: (u32, u32),
    identity: [f32; 16],
    vertex_data: Vec<u8>,
    index_data: Vec<u8>,
}

impl App {
    fn new() -> Self {
        let identity: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let vertex_data: Vec<u8> = bytemuck::cast_slice(&[
            0.0f32, 0.5, 0.0, 0.0, 1.0, 0.0,
            -0.5, -0.5, 0.0, 0.0, 1.0, 0.0,
            0.5, -0.5, 0.0, 0.0, 1.0, 0.0,
        ]).to_vec();
        let index_data: Vec<u8> = bytemuck::cast_slice(&[0u32, 1u32, 2u32]).to_vec();
        Self {
            window: None,
            backend: None,
            size: (800, 600),
            identity,
            vertex_data,
            index_data,
        }
    }

    fn build_view_projection(&self) -> [f32; 16] {
        let (w, h) = self.size;
        let aspect = if h > 0 { w as f32 / h as f32 } else { 1.0 };
        let proj = perspective_projection(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
        let view = look_at([0.0, 0.0, 2.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        mat4_mul(&proj, &view)
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = winit::window::WindowAttributes::default()
            .with_title("Lumelite GBuffer + Light (backend-agnostic)")
            .with_inner_size(winit::dpi::LogicalSize::new(800, 600));
        let window = event_loop.create_window(attrs).expect("create window");
        let phys = window.inner_size();
        self.size = (phys.width, phys.height);
        self.window = Some(window);
        if let Some(ref w) = self.window {
            w.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(physical) => {
                self.size = (physical.width.max(1), physical.height.max(1));
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let window = match &self.window {
                    Some(w) => w,
                    None => return,
                };
                self.size = {
                    let phys = window.inner_size();
                    (phys.width.max(1), phys.height.max(1))
                };
                if self.backend.is_none() {
                    match lumelite_bridge::LumeliteWindowBackend::from_window(window) {
                        Ok(backend) => self.backend = Some(backend),
                        Err(e) => {
                            eprintln!("LumeliteWindowBackend::from_window failed: {}", e);
                            return;
                        }
                    }
                }
                let (raw_window, raw_display) = match (window.window_handle(), window.display_handle()) {
                    (Ok(wh), Ok(dh)) => (wh.as_raw(), dh.as_raw()),
                    _ => return,
                };
                let mut meshes = HashMap::new();
                meshes.insert(1u64, render_api::ExtractedMesh {
                    entity_id: 1,
                    vertex_data: self.vertex_data.clone(),
                    index_data: self.index_data.clone(),
                    transform: self.identity,
                    visible: true,
                });
                let extracted = ExtractedMeshes { meshes };
                let view = ExtractedView {
                    view_proj: self.build_view_projection(),
                    viewport_size: self.size,
                    directional_light: Some(([0.3, -0.8, 0.5], [1.0, 1.0, 1.0])),
                    point_lights: Vec::new(),
                    spot_lights: Vec::new(),
                    sky_light: None,
                };
                let backend = match &mut self.backend {
                    Some(b) => b,
                    None => return,
                };
                backend.prepare(&extracted);
                let _ = backend.render_frame_to_window(&view, raw_window, raw_display);
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), String> {
    let event_loop = winit::event_loop::EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App::new();
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    Ok(())
}
