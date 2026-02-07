//! PBR model viewer: load OBJ + PBR textures, render with Lumelite (prepare + render_frame_to_window).
//! Run from repo root: cargo run -p debug --bin pbr_model
//! Resources: 模型/green-vintage-metal-chair-with-books-and-flowers.obj and .../textures/

use std::collections::HashMap;
use std::path::Path;

use render_api::{
    ExtractedMeshes, ExtractedView, ExtractedPbrMaterial, PbrTextureData, RenderBackendWindow,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

fn load_image_rgba(path: &Path) -> Result<PbrTextureData, String> {
    let img = image::open(path).map_err(|e| e.to_string())?;
    let rgb = img.to_rgba8();
    let (w, h) = rgb.dimensions();
    Ok(PbrTextureData {
        data: rgb.into_raw(),
        width: w,
        height: h,
    })
}

fn find_texture(dir: &Path, pattern: &str) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        if p.file_name().and_then(|n| n.to_str()).map_or(false, |n| n.contains(pattern)) {
            return Some(p);
        }
    }
    None
}

fn load_pbr_material(texture_dir: &Path) -> Result<ExtractedPbrMaterial, String> {
    let base_color = find_texture(texture_dir, "BaseColor")
        .or_else(|| find_texture(texture_dir, "basecolor"))
        .and_then(|p| load_image_rgba(&p).ok());
    let normal = find_texture(texture_dir, "Normal")
        .or_else(|| find_texture(texture_dir, "normal"))
        .and_then(|p| load_image_rgba(&p).ok());
    let metallic = find_texture(texture_dir, "Metallic").or_else(|| find_texture(texture_dir, "metallic"));
    let roughness = find_texture(texture_dir, "Roughness").or_else(|| find_texture(texture_dir, "roughness"));
    let metallic_roughness = metallic
        .or(roughness)
        .and_then(|p| load_image_rgba(&p).ok());
    let ao = find_texture(texture_dir, "AO")
        .or_else(|| find_texture(texture_dir, "_AO"))
        .and_then(|p| load_image_rgba(&p).ok());
    Ok(ExtractedPbrMaterial {
        base_color,
        normal,
        metallic_roughness,
        ao,
    })
}

fn load_obj_mesh(obj_path: &Path) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (models, _) = tobj::load_obj(obj_path, &tobj::GPU_LOAD_OPTIONS)
        .map_err(|e| format!("load_obj: {:?}", e))?;
    let mesh = models.into_iter().next().ok_or("No mesh in OBJ")?.mesh;
    let positions: Vec<f32> = mesh.positions.iter().map(|&x| x as f32).collect();
    let normals: Vec<f32> = if mesh.normals.is_empty() {
        (0..positions.len()).map(|_| 0.0f32).collect()
    } else {
        mesh.normals.iter().map(|&x| x as f32).collect()
    };
    let texcoords: Vec<f32> = if mesh.texcoords.is_empty() {
        (0..(positions.len() / 3 * 2)).map(|_| 0.0f32).collect()
    } else {
        mesh.texcoords.iter().map(|&x| x as f32).collect()
    };
    let indices = mesh.indices;
    let n_pos = positions.len() / 3;
    let n_norm = normals.len() / 3;
    let n_tex = texcoords.len() / 2;

    let mut vertex_data = Vec::with_capacity(indices.len() * 32);
    for (i, &idx) in indices.iter().enumerate() {
        let pi = (idx as usize).min(n_pos.saturating_sub(1)) * 3;
        let ni = if mesh.normal_indices.is_empty() {
            (idx as usize).min(n_norm.saturating_sub(1)) * 3
        } else {
            let ni_idx = mesh.normal_indices.get(i).copied().unwrap_or(0) as usize;
            ni_idx.min(n_norm.saturating_sub(1)) * 3
        };
        let ti = if mesh.texcoord_indices.is_empty() {
            (idx as usize).min(n_tex.saturating_sub(1)) * 2
        } else {
            let ti_idx = mesh.texcoord_indices.get(i).copied().unwrap_or(0) as usize;
            ti_idx.min(n_tex.saturating_sub(1)) * 2
        };
        vertex_data.extend_from_slice(bytemuck::cast_slice(&[
            positions[pi],
            positions[pi + 1],
            positions[pi + 2],
            normals[ni],
            normals[ni + 1],
            normals[ni + 2],
            texcoords[ti],
            texcoords[ti + 1],
        ]));
    }
    let new_indices: Vec<u32> = (0..indices.len() as u32).collect();
    let index_data = bytemuck::cast_slice(new_indices.as_slice()).to_vec();
    Ok((vertex_data, index_data))
}

fn ortho_projection(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> [f32; 16] {
    let sx = 2.0 / (right - left);
    let sy = 2.0 / (top - bottom);
    let sz = -1.0 / (far - near);
    let tx = -(right + left) / (right - left);
    let ty = -(top + bottom) / (top - bottom);
    let tz = -near / (far - near);
    [
        sx, 0.0, 0.0, 0.0, 0.0, sy, 0.0, 0.0, 0.0, 0.0, sz, 0.0, tx, ty, tz, 1.0,
    ]
}

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
    let tx = -(s[0] * eye[0] + s[1] * eye[1] + s[2] * eye[2]);
    let ty = -(u[0] * eye[0] + u[1] * eye[1] + u[2] * eye[2]);
    let tz = f[0] * eye[0] + f[1] * eye[1] + f[2] * eye[2];
    [
        s[0], u[0], -f[0], 0.0, s[1], u[1], -f[1], 0.0, s[2], u[2], -f[2], 0.0, tx, ty, tz, 1.0,
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

struct App {
    window: Option<winit::window::Window>,
    backend: Option<Box<dyn RenderBackendWindow>>,
    size: (u32, u32),
    extracted_meshes: ExtractedMeshes,
}

impl App {
    fn new(obj_path: &Path, texture_dir: &Path) -> Result<Self, String> {
        let (vertex_data, index_data) = load_obj_mesh(obj_path)?;
        let material = load_pbr_material(texture_dir).ok();
        let identity: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let mut meshes = HashMap::new();
        meshes.insert(
            1u64,
            render_api::ExtractedMesh {
                entity_id: 1,
                vertex_data,
                index_data,
                transform: identity,
                visible: true,
                vertex_format: render_api::VertexFormat::PositionNormalUv,
                material,
            },
        );
        let extracted_meshes = ExtractedMeshes { meshes };
        Ok(Self {
            window: None,
            backend: None,
            size: (800, 600),
            extracted_meshes,
        })
    }

    fn build_view_projection(&self) -> [f32; 16] {
        let (w, h) = self.size;
        let aspect = if h > 0 { w as f32 / h as f32 } else { 1.0 };
        let proj = ortho_projection(-aspect, aspect, -1.0, 1.0, 0.1, 100.0);
        let view = look_at([2.0, 1.5, 2.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        mat4_mul(&proj, &view)
    }

    /// 构建带合理光照的 ExtractedView：主平行光模拟太阳 + 点光模拟背景/环境光。
    fn build_view(&self) -> ExtractedView {
        let view_proj = self.build_view_projection();
        let viewport_size = self.size;

        // 主平行光：模拟太阳，从右上前方照向场景，方向为光照射方向（指向场景）
        let sun_dir = normalize([-0.4f32, -0.88, -0.25]);
        let sun_color = [1.15, 1.1, 1.0];
        let directional_light = Some((sun_dir, sun_color));

        // 背景/环境光：用若干弱强度、大半径点光模拟天空与环境反射，避免背光面全黑
        let point_lights = vec![
            render_api::PointLight {
                position: [0.0, 4.0, 0.0],
                color: [0.28, 0.32, 0.38],
                radius: 18.0,
                falloff_exponent: 2.0,
            },
            render_api::PointLight {
                position: [-2.5, 1.0, 2.0],
                color: [0.22, 0.25, 0.3],
                radius: 14.0,
                falloff_exponent: 2.0,
            },
            render_api::PointLight {
                position: [2.0, 0.5, -1.5],
                color: [0.18, 0.2, 0.24],
                radius: 12.0,
                falloff_exponent: 2.0,
            },
        ];

        ExtractedView {
            view_proj,
            viewport_size,
            directional_light,
            point_lights,
            spot_lights: Vec::new(),
            sky_light: None,
        }
    }
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-6 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, -1.0, 0.0]
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = winit::window::WindowAttributes::default()
            .with_title("Lumelite PBR Model")
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
            WindowEvent::CloseRequested => event_loop.exit(),
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
                let view = self.build_view();
                if let Some(backend) = &mut self.backend {
                    backend.prepare(&self.extracted_meshes);
                    window.pre_present_notify();
                    let _ = backend.render_frame_to_window(&view, raw_window, raw_display);
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), String> {
    let base = std::env::current_dir().map_err(|e| e.to_string())?;
    let model_name = "green-vintage-metal-chair-with-books-and-flowers";
    let obj_path = base.join("模型").join(format!("{}.obj", model_name));
    let texture_dir = base.join("模型").join(model_name).join("textures");
    if !obj_path.exists() {
        return Err(format!("OBJ not found: {}", obj_path.display()));
    }
    let event_loop = winit::event_loop::EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App::new(&obj_path, &texture_dir)?;
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    Ok(())
}
