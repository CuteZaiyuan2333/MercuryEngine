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

/// Point light: position, color, radius, falloff exponent for attenuation.
#[derive(Clone, Debug, Default)]
pub struct PointLight {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub radius: f32,
    pub falloff_exponent: f32,
}

/// Spot light: position, direction (unit vector), color, radius, inner/outer angles (radians).
#[derive(Clone, Debug, Default)]
pub struct SpotLight {
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub radius: f32,
    pub inner_angle: f32,
    pub outer_angle: f32,
}

/// Sky light (simplified): direction, color, intensity.
#[derive(Clone, Debug, Default)]
pub struct SkyLight {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
}

/// View/camera data for the current frame.
#[derive(Clone, Debug)]
pub struct ExtractedView {
    pub view_proj: [f32; 16],
    pub viewport_size: (u32, u32),
    /// Optional: main directional light. If None, Lumelite uses a default.
    /// (direction: unit vector, color: RGB)
    pub directional_light: Option<([f32; 3], [f32; 3])>,
    /// Point lights (capped by LumeliteConfig::max_point_lights).
    pub point_lights: Vec<PointLight>,
    /// Spot lights (capped by LumeliteConfig::max_spot_lights).
    pub spot_lights: Vec<SpotLight>,
    /// Optional sky light.
    pub sky_light: Option<SkyLight>,
}

impl Default for ExtractedView {
    fn default() -> Self {
        Self {
            view_proj: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            viewport_size: (800, 600),
            directional_light: None,
            point_lights: Vec::new(),
            spot_lights: Vec::new(),
            sky_light: None,
        }
    }
}
