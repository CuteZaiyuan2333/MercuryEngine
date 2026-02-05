//! Lume plugin: initializes the renderer and provides the bridge API for the host engine.

use lume_rhi::Device;
use lume_renderer::Renderer;
use std::sync::Arc;

use crate::extract::{ExtractedMeshes, ExtractedView};

/// Plugin state: holds the Lume renderer and optional extracted data for the frame.
pub struct LumePlugin {
    /// The Lume renderer (backed by Vulkan or future Metal RHI).
    pub renderer: Renderer,
}

impl LumePlugin {
    /// Create the plugin with a device (e.g. Vulkan from lume_rhi::VulkanDevice::new()).
    pub fn new(device: Arc<dyn Device>) -> Self {
        Self {
            renderer: Renderer::new(device),
        }
    }

    /// Prepare phase: upload extracted meshes to GPU buffers and register with the render graph.
    /// The host should call this each frame after extraction, with the same `extracted` data.
    pub fn prepare(&mut self, _extracted: &ExtractedMeshes) {
        // TODO: Create/update RHI buffers from extracted mesh data and add to graph resources.
    }

    /// Render one frame. The host should call this after prepare, then submit returned command buffers via the device.
    /// Returns command buffers to submit (caller calls device.submit(...)).
    pub fn render_frame(
        &mut self,
        _view: &ExtractedView,
    ) -> Result<Vec<Box<dyn lume_rhi::CommandBuffer>>, String> {
        self.renderer.render_frame()
    }
}
