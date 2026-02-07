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
    /// When true, present GBuffer0 directly (debug: bypass Light pass).
    pub debug_show_gbuffer: bool,
    /// When true, Present pass only clears to GREEN (no draw) - verify swapchain works.
    pub debug_clear_green: bool,
    /// When true, draw triangle directly to swapchain (bypass GBuffer/Light/Present).
    pub debug_direct_triangle: bool,
    /// Max point lights (reserved for P1 extension).
    pub max_point_lights: u32,
    /// Max spot lights (reserved for P1 extension).
    pub max_spot_lights: u32,
    /// Enable shadow pass (single cascade, directional light).
    pub shadow_enabled: bool,
    /// Shadow map resolution (e.g. 1024).
    pub shadow_resolution: u32,
    /// Tone mapping for present pass.
    pub tone_mapping: ToneMapping,
    /// Swapchain texture format for present (e.g. Rgba8Unorm or Bgra8Unorm).
    pub swapchain_format: wgpu::TextureFormat,
}

impl Default for LumeliteConfig {
    fn default() -> Self {
        Self {
            debug_show_gbuffer: true, // TODO: set false after fixing Light pass
            debug_clear_green: false, // swapchain verified OK
            debug_direct_triangle: false,
            max_point_lights: 8,
            max_spot_lights: 4,
            shadow_enabled: false,
            shadow_resolution: 1024,
            tone_mapping: ToneMapping::default(),
            swapchain_format: wgpu::TextureFormat::Rgba8Unorm,
        }
    }
}
