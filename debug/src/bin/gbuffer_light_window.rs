//! 后端无关示例：仅依赖 render-api + LumeliteWindowBackend，宿主不直接调用 wgpu。
//! Run: cargo run -p debug --bin gbuffer_light_window

use std::collections::HashMap;
use render_api::{ExtractedMeshes, ExtractedView, RenderBackendWindow};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

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
                if self.backend.is_none() {
                    match lumelite_bridge::LumeliteWindowBackend::from_window(window) {
                        Ok(backend) => self.backend = Some(backend),
                        Err(e) => {
                            eprintln!("LumeliteWindowBackend::from_window failed: {}", e);
                            return;
                        }
                    }
                }
                let backend = match &mut self.backend {
                    Some(b) => b,
                    None => return,
                };
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
                    view_proj: self.identity,
                    viewport_size: self.size,
                    directional_light: Some(([0.3, -0.8, 0.5], [1.0, 1.0, 1.0])),
                };
                backend.prepare(&extracted);
                if backend.render_frame_to_window(&view, raw_window, raw_display).is_ok() {
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                }
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
