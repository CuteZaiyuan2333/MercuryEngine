//! Lume Bridge: MercuryEngine integration layer.
//! No dependency on Bevy, WGPU, or any other rendering engine.
//! Uses only Lume RHI (Vulkan) and Lume Renderer.

mod extract;
mod plugin;

pub use extract::{ExtractedMesh, ExtractedMeshes, ExtractedView};
pub use plugin::LumePlugin;
pub use lume_renderer::Renderer;
