//! Lume Bridge: MercuryEngine integration layer.
//! Uses render_api types and RenderBackend; Lume RHI (Vulkan) and Lume Renderer.

mod plugin;

pub use plugin::LumePlugin;
pub use lume_renderer::Renderer;
pub use render_api::{ExtractedMesh, ExtractedMeshes, ExtractedView};
