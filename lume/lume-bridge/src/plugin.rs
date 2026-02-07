//! Lume plugin: implements render_api::RenderBackend for the host.

use lume_rhi::Device;
use lume_renderer::Renderer;
use render_api::{ExtractedMeshes, ExtractedView, RenderBackend};
use std::sync::Arc;

/// Plugin state: holds the Lume renderer and device for submission.
pub struct LumePlugin {
    device: Arc<dyn Device>,
    renderer: Renderer,
}

impl LumePlugin {
    /// Create the plugin with a device (e.g. from `lume_rhi::create_device(DeviceCreateParams::default())` or with `surface: Some(window)` for swapchain).
    pub fn new(device: Arc<dyn Device>) -> Self {
        Self {
            device: Arc::clone(&device),
            renderer: Renderer::new(device),
        }
    }

    /// Prepare phase: upload extracted meshes to GPU buffers and register with the render graph.
    pub fn prepare(&mut self, _extracted: &ExtractedMeshes) {
        // TODO: Create/update RHI buffers from extracted mesh data and add to graph resources.
    }

    /// Render one frame; returns command buffers (caller may submit via device). Used internally by RenderBackend.
    pub fn render_frame(
        &mut self,
        _view: &ExtractedView,
    ) -> Result<Vec<Box<dyn lume_rhi::CommandBuffer>>, String> {
        self.renderer.render_frame()
    }
}

impl RenderBackend for LumePlugin {
    fn prepare(&mut self, extracted: &ExtractedMeshes) {
        LumePlugin::prepare(self, extracted);
    }

    fn render_frame(&mut self, _view: &ExtractedView) -> Result<(), String> {
        let command_buffers = self.renderer.render_frame()?;
        self.device.submit(command_buffers)?;
        Ok(())
    }
}
