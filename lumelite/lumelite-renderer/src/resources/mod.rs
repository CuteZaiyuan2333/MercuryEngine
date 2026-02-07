//! Frame resources: GBuffer (4 RTs), Depth, Light Buffer, optional Shadow Map. Flax-compatible layout.

use wgpu::TextureView;

pub struct FrameResources {
    pub gbuffer0: wgpu::Texture,
    pub gbuffer1: wgpu::Texture,
    pub gbuffer2: wgpu::Texture,
    pub gbuffer3: wgpu::Texture,
    pub depth: wgpu::Texture,
    pub light_buffer: wgpu::Texture,
    pub shadow_map: Option<wgpu::Texture>,
    width: u32,
    height: u32,
}

impl FrameResources {
    pub fn ensure_size(
        device: &wgpu::Device,
        existing: Option<Self>,
        width: u32,
        height: u32,
        shadow_enabled: bool,
        shadow_resolution: u32,
    ) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("FrameResources: width and height must be > 0".to_string());
        }
        if let Some(r) = existing {
            if r.width == width && r.height == height && r.shadow_map.is_some() == shadow_enabled {
                return Ok(r);
            }
        }
        let make_rt = |label: &str, format: wgpu::TextureFormat| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let gbuffer0 = make_rt("gbuffer0", wgpu::TextureFormat::Rgba8Unorm);
        let gbuffer1 = make_rt("gbuffer1", wgpu::TextureFormat::Rgba8Unorm);
        let gbuffer2 = make_rt("gbuffer2", wgpu::TextureFormat::Rgba8Unorm);
        let gbuffer3 = make_rt("gbuffer3", wgpu::TextureFormat::Rgba8Unorm);
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let light_buffer = make_rt("light_buffer", wgpu::TextureFormat::Rgba16Float);
        let shadow_map = if shadow_enabled && shadow_resolution > 0 {
            Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some("shadow_map"),
                size: wgpu::Extent3d {
                    width: shadow_resolution,
                    height: shadow_resolution,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }))
        } else {
            None
        };
        Ok(Self {
            gbuffer0,
            gbuffer1,
            gbuffer2,
            gbuffer3,
            depth,
            light_buffer,
            shadow_map,
            width,
            height,
        })
    }
    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn gbuffer0_view(&self) -> TextureView { self.gbuffer0.create_view(&Default::default()) }
    pub fn gbuffer1_view(&self) -> TextureView { self.gbuffer1.create_view(&Default::default()) }
    pub fn gbuffer2_view(&self) -> TextureView { self.gbuffer2.create_view(&Default::default()) }
    pub fn gbuffer3_view(&self) -> TextureView { self.gbuffer3.create_view(&Default::default()) }
    pub fn depth_view(&self) -> TextureView { self.depth.create_view(&Default::default()) }
    pub fn light_buffer_view(&self) -> TextureView {
        self.light_buffer.create_view(&Default::default())
    }
    pub fn shadow_map_view(&self) -> TextureView {
        self.shadow_map
            .as_ref()
            .expect("shadow_map_view called but shadow_map is None")
            .create_view(&Default::default())
    }
}
