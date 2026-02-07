//! Shared render backend API for MercuryEngine.
//! Defines Extract types and RenderBackend trait so the host can use Lume or Lumelite
//! with the same code path (prepare + render_frame).

mod extract;
mod backend;

pub use extract::{
    ExtractedMesh, ExtractedMeshes, ExtractedPbrMaterial, ExtractedView, PbrTextureData, PointLight,
    SpotLight, SkyLight, VertexFormat,
};
pub use backend::{RenderBackend, RenderBackendWindow};
pub use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
