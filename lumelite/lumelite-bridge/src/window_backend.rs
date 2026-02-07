//! Window-capable backend: created from a window, implements RenderBackendWindow.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use render_api::{ExtractedMeshes, ExtractedView, RenderBackend, RenderBackendWindow};
use wgpu::SurfaceTargetUnsafe;

use crate::plugin::LumelitePlugin;
use lumelite_renderer::LumeliteConfig;

/// Backend that owns wgpu Instance and LumelitePlugin; can present to a window.
/// Created via `LumeliteWindowBackend::from_window(window)`; each frame use
/// `render_frame_to_window(view, raw_window_handle, raw_display_handle)`.
/// Surface is recreated each frame (wgpu::Surface lifetime tied to window; avoids
/// transmute and platform-specific staleness when window is dragged/resized).
pub struct LumeliteWindowBackend {
    instance: wgpu::Instance,
    plugin: LumelitePlugin,
}

impl LumeliteWindowBackend {
    /// Create a window-capable backend from a window (e.g. winit). The window is only used
    /// to get raw handles and to create an initial surface for adapter selection.
    /// The host must keep the window alive; each frame pass its raw handles to
    /// `render_frame_to_window`.
    pub fn from_window(
        window: &(impl HasWindowHandle + HasDisplayHandle),
    ) -> Result<Box<dyn RenderBackendWindow>, String> {
        let (raw_window, raw_display) = {
            let wh = window.window_handle().map_err(|e| e.to_string())?;
            let dh = window.display_handle().map_err(|e| e.to_string())?;
            (wh.as_raw(), dh.as_raw())
        };
        let backend = pollster::block_on(Self::from_raw_handles_async(raw_window, raw_display))?;
        Ok(Box::new(backend))
    }

    async fn from_raw_handles_async(
        raw_window_handle: raw_window_handle::RawWindowHandle,
        raw_display_handle: raw_window_handle::RawDisplayHandle,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let target = SurfaceTargetUnsafe::RawHandle {
            raw_window_handle,
            raw_display_handle,
        };
        let surface = unsafe { instance.create_surface_unsafe(target).map_err(|e| e.to_string())? };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("No adapter")?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .first()
            .copied()
            .unwrap_or(wgpu::TextureFormat::Rgba8Unorm);
        let config = LumeliteConfig {
            swapchain_format: format,
            ..LumeliteConfig::default()
        };
        let plugin = LumelitePlugin::new_with_config(device, queue, config)?;
        drop(surface);
        Ok(Self { instance, plugin })
    }

    fn surface_config(format: wgpu::TextureFormat, width: u32, height: u32) -> wgpu::SurfaceConfiguration {
        wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        }
    }
}

impl RenderBackend for LumeliteWindowBackend {
    fn prepare(&mut self, extracted: &ExtractedMeshes) {
        self.plugin.prepare(extracted);
    }

    fn render_frame(&mut self, view: &ExtractedView) -> Result<(), String> {
        self.plugin.render_frame(view)
    }
}

impl RenderBackendWindow for LumeliteWindowBackend {
    fn render_frame_to_window(
        &mut self,
        view: &ExtractedView,
        raw_window_handle: raw_window_handle::RawWindowHandle,
        raw_display_handle: raw_window_handle::RawDisplayHandle,
    ) -> Result<(), String> {
        let target = SurfaceTargetUnsafe::RawHandle {
            raw_window_handle,
            raw_display_handle,
        };
        let surface = unsafe {
            self.instance
                .create_surface_unsafe(target)
                .map_err(|e| e.to_string())?
        };
        let (width, height) = view.viewport_size;
        let config = Self::surface_config(
            self.plugin.renderer().config().swapchain_format,
            width.max(1),
            height.max(1),
        );
        surface.configure(self.plugin.device(), &config);

        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Outdated) => {
                surface.configure(self.plugin.device(), &config);
                surface.get_current_texture().map_err(|e| e.to_string())?
            }
            Err(wgpu::SurfaceError::Lost) => {
                surface.configure(self.plugin.device(), &config);
                surface.get_current_texture().map_err(|e| e.to_string())?
            }
            Err(wgpu::SurfaceError::Timeout) => return Err("Surface get_current_texture timeout".to_string()),
            Err(e) => return Err(e.to_string()),
        };
        let swapchain_format = self.plugin.renderer().config().swapchain_format;
        let viewport = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(swapchain_format.add_srgb_suffix()),
            ..Default::default()
        });
        self.plugin
            .render_frame_to_swapchain(view, &viewport)
            .map_err(|e| e.to_string())?;
        frame.present();
        Ok(())
    }
}
