//! Data types for extraction from the host engine (e.g. MercuryEngine) into the render world.
//! The host fills these each frame; Lume uses them in Prepare and the render graph.

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
    /// World transform (4x4 row-major or column-major as agreed).
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
}
