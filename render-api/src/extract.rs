//! Data types for extraction from the host engine into the render world.
//! Used by both Lume and Lumelite backends; host fills these each frame.

use std::collections::HashMap;

/// Per-mesh instance data extracted from the main world.
#[derive(Clone, Debug)]
pub struct ExtractedMesh {
    /// Host-defined entity or instance id.
    pub entity_id: u64,
    /// Vertex data (e.g. position + normal) in a format agreed with the pipeline.
    pub vertex_data: Vec<u8>,
    /// Index data (u32 indices).
    pub index_data: Vec<u8>,
    /// World transform: column-major 4x4 matrix (WGSL/wgpu convention).
    /// Index [col*4+row]; e.g. m[0..4] is the first column.
    pub transform: [f32; 16],
    /// Whether this instance is visible.
    pub visible: bool,
}

/// All extracted meshes for the current frame.
#[derive(Default, Debug)]
pub struct ExtractedMeshes {
    pub meshes: HashMap<u64, ExtractedMesh>,
}

/// View/camera data for the current frame.
#[derive(Clone, Debug)]
pub struct ExtractedView {
    pub view_proj: [f32; 16],
    pub viewport_size: (u32, u32),
    /// Optional: main directional light. If None, Lumelite uses a default.
    /// (direction: unit vector, color: RGB)
    pub directional_light: Option<([f32; 3], [f32; 3])>,
}

impl Default for ExtractedView {
    fn default() -> Self {
        Self {
            view_proj: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            viewport_size: (800, 600),
            directional_light: None,
        }
    }
}
