//! Trait for render backends (Lume or Lumelite). Host uses this to call prepare/render_frame uniformly.

use crate::{ExtractedMeshes, ExtractedView};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

/// Render backend that the host can use regardless of whether the implementation is Lume or Lumelite.
pub trait RenderBackend: Send {
    /// Prepare phase: upload extracted meshes to GPU and register resources.
    fn prepare(&mut self, extracted: &ExtractedMeshes);

    /// Render one frame. Submits work internally; caller does not need to submit command buffers.
    fn render_frame(&mut self, view: &ExtractedView) -> Result<(), String>;
}

/// Extension for backends that can present to a window. Host passes raw handles (e.g. from winit);
/// the backend owns swapchain/surface and performs get_current_texture + present internally.
pub trait RenderBackendWindow: RenderBackend + Send {
    /// Render one frame and present to the window identified by the given raw handles.
    /// The backend configures the surface from `view.viewport_size` and submits work.
    fn render_frame_to_window(
        &mut self,
        view: &ExtractedView,
        raw_window_handle: RawWindowHandle,
        raw_display_handle: RawDisplayHandle,
    ) -> Result<(), String>;
}
