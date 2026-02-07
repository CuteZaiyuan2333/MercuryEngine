//! Lumelite configuration: lights, shadows, tone mapping, swapchain.

/// Tone mapping mode for present pass.
#[derive(Clone, Copy, Debug, Default)]
pub enum ToneMapping {
    #[default]
    Reinhard,
    /// No tone mapping (clamp).
    None,
}

/// Lumelite renderer and bridge configuration.
#[derive(Clone, Debug)]
pub struct LumeliteConfig {
    /// Max point lights (reserved for P1 extension).
    pub max_point_lights: u32,
    /// Max spot lights (reserved for P1 extension).
    pub max_spot_lights: u32,
    /// Enable shadow pass (P2; currently no-op).
    pub shadow_enabled: bool,
    /// Tone mapping for present pass.
    pub tone_mapping: ToneMapping,
    /// Swapchain texture format for present (e.g. Rgba8Unorm or Bgra8Unorm).
    pub swapchain_format: wgpu::TextureFormat,
}

impl Default for LumeliteConfig {
    fn default() -> Self {
        Self {
            max_point_lights: 8,
            max_spot_lights: 4,
            shadow_enabled: false,
            tone_mapping: ToneMapping::default(),
            swapchain_format: wgpu::TextureFormat::Rgba8Unorm,
        }
    }
}
