//! Data types for extraction from the host engine into the render world.
//! Used by both Lume and Lumelite backends; host fills these each frame.

use std::collections::HashMap;

/// Vertex layout for mesh data. Lumelite only accepts PositionNormalUv.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VertexFormat {
    /// Position (12 bytes) + normal (12 bytes) = 24 bytes per vertex.
    PositionNormal,
    /// Position (12) + normal (12) + uv (8) = 32 bytes per vertex. Default for Lumelite.
    #[default]
    PositionNormalUv,
}

/// CPU-side texture data for cross-backend transfer. RGBA8 row-major.
#[derive(Clone, Debug)]
pub struct PbrTextureData {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// PBR material data; all channels optional. Backends use defaults for missing channels.
#[derive(Clone, Debug, Default)]
pub struct ExtractedPbrMaterial {
    pub base_color: Option<PbrTextureData>,
    pub normal: Option<PbrTextureData>,
    /// R = metallic, G = roughness. Single RGBA texture.
    pub metallic_roughness: Option<PbrTextureData>,
    pub ao: Option<PbrTextureData>,
}

/// Per-mesh instance data extracted from the main world.
#[derive(Clone, Debug)]
pub struct ExtractedMesh {
    /// Host-defined entity or instance id.
    pub entity_id: u64,
    /// Vertex data in format given by vertex_format (e.g. position+normal+uv for PositionNormalUv).
    pub vertex_data: Vec<u8>,
    /// Index data (u32 indices).
    pub index_data: Vec<u8>,
    /// World transform: column-major 4x4 matrix (WGSL/wgpu convention).
    /// Index [col*4+row]; e.g. m[0..4] is the first column.
    pub transform: [f32; 16],
    /// Whether this instance is visible.
    pub visible: bool,
    /// Vertex layout. Lumelite only accepts PositionNormalUv.
    pub vertex_format: VertexFormat,
    /// Optional PBR material. When None, Lumelite uses default (flat) material.
    pub material: Option<ExtractedPbrMaterial>,
}

impl Default for ExtractedMesh {
    fn default() -> Self {
        Self {
            entity_id: 0,
            vertex_data: Vec::new(),
            index_data: Vec::new(),
            transform: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            visible: true,
            vertex_format: VertexFormat::default(),
            material: None,
        }
    }
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
