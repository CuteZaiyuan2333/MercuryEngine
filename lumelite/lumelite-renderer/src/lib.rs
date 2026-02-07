//! Lumelite Renderer: wgpu-based GBuffer + Flax-style Light Pass + Present.

pub mod config;
pub mod direct_triangle;
pub mod gbuffer;
pub mod gi;
pub mod graph;
pub mod light_pass;
pub mod present;
pub mod resources;
pub mod shadows;
pub mod virtual_geom;

pub use config::{LumeliteConfig, ToneMapping};
pub use direct_triangle::DirectTrianglePass;
pub use gbuffer::{GBufferPass, MeshDraw};
pub use graph::{NodeId, RenderGraph, RenderGraphNode, ResourceHandle, ResourceId, ResourceUsage, TextureBarrierHint};
pub use light_pass::LightPass;
pub use present::PresentPass;
pub use shadows::ShadowPass;
pub use resources::FrameResources;

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: LumeliteConfig,
    direct_triangle_pass: DirectTrianglePass,
    gbuffer_pass: GBufferPass,
    light_pass: LightPass,
    present_pass: PresentPass,
    shadow_pass: Option<ShadowPass>,
    frame_resources: Option<FrameResources>,
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, String> {
        Self::new_with_config(device, queue, LumeliteConfig::default())
    }

    pub fn new_with_config(device: wgpu::Device, queue: wgpu::Queue, config: LumeliteConfig) -> Result<Self, String> {
        let direct_triangle_pass = DirectTrianglePass::new(&device, config.swapchain_format)?;
        let gbuffer_pass = GBufferPass::new(&device, wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureFormat::Depth32Float)?;
        let light_pass = LightPass::new(&device, wgpu::TextureFormat::Rgba16Float)?;
        let present_pass = PresentPass::new(&device, config.swapchain_format, config.tone_mapping)?;
        let shadow_pass = if config.shadow_enabled {
            Some(ShadowPass::new(&device, config.shadow_resolution)?)
        } else {
            None
        };
        Ok(Self {
            device,
            queue,
            config,
            direct_triangle_pass,
            gbuffer_pass,
            light_pass,
            present_pass,
            shadow_pass,
            frame_resources: None,
        })
    }

    pub fn device(&self) -> &wgpu::Device { &self.device }
    pub fn queue(&self) -> &wgpu::Queue { &self.queue }
    pub fn config(&self) -> &LumeliteConfig { &self.config }

    pub fn ensure_frame_resources(&mut self, width: u32, height: u32) -> Result<(), String> {
        let existing = self.frame_resources.take();
        let new_res = FrameResources::ensure_size(
            &self.device,
            existing,
            width,
            height,
            self.config.shadow_enabled,
            self.config.shadow_resolution,
        )?;
        self.frame_resources = Some(new_res);
        Ok(())
    }

    pub fn current_light_buffer(&self) -> Option<&wgpu::Texture> {
        self.frame_resources.as_ref().map(|f| &f.light_buffer)
    }

    /// Encode direct triangle to output view (debug path). Bypasses GBuffer/Light/Present.
    pub fn encode_direct_triangle(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        meshes: &[MeshDraw],
        view_proj: &[f32; 16],
    ) -> Result<(), String> {
        self.direct_triangle_pass.encode(
            encoder,
            &self.device,
            &self.queue,
            output_view,
            meshes,
            view_proj,
        )
    }

    /// Encode GBuffer + Light pass into the given encoder. Call ensure_frame_resources (or render_frame) first so frame size is set.
    pub fn encode_frame(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        view_proj: &[f32; 16],
        inv_view_proj: &[f32; 16],
        meshes: &[MeshDraw],
        directional_light: ([f32; 3], [f32; 3]),
        point_lights: &[render_api::PointLight],
        spot_lights: &[render_api::SpotLight],
        light_view_proj: Option<&[f32; 16]>,
    ) -> Result<(), String> {
        self.ensure_frame_resources(width, height)?;
        let frame = self.frame_resources.as_ref().unwrap();
        if let (Some(ref shadow_pass), Some(lvp)) = (&self.shadow_pass, light_view_proj) {
            shadow_pass.encode(encoder, &self.device, &self.queue, frame, meshes, lvp)?;
        }
        self.gbuffer_pass.encode(encoder, &self.device, &self.queue, frame, meshes, view_proj)?;
        self.light_pass.encode_directional(
            encoder,
            &self.device,
            &self.queue,
            frame,
            directional_light.0,
            directional_light.1,
            inv_view_proj,
        )?;
        let max_point = self.config.max_point_lights as usize;
        for light in point_lights.iter().take(max_point) {
            self.light_pass.encode_point(
                encoder,
                &self.device,
                &self.queue,
                frame,
                light,
                inv_view_proj,
            )?;
        }
        let max_spot = self.config.max_spot_lights as usize;
        for light in spot_lights.iter().take(max_spot) {
            self.light_pass.encode_spot(encoder, &self.device, &self.queue, frame, light, inv_view_proj)?;
        }
        Ok(())
    }

    /// Encode present pass: light buffer -> output view (e.g. swapchain). Requires encode_frame to have been called this frame.
    /// When debug_show_gbuffer is true, presents GBuffer0 directly (bypasses Light pass for debugging).
    pub fn encode_present_to(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
    ) -> Result<(), String> {
        let frame = self.frame_resources.as_ref().ok_or("encode_present_to: no frame (call encode_frame first)")?;
        let source = if self.config.debug_show_gbuffer {
            frame.gbuffer0_view()
        } else {
            frame.light_buffer_view()
        };
        self.present_pass.encode(
            encoder,
            &self.device,
            &self.queue,
            &source,
            output_view,
            self.config.debug_clear_green,
        )
    }

    pub fn render_frame(
        &mut self,
        width: u32,
        height: u32,
        view_proj: &[f32; 16],
        inv_view_proj: &[f32; 16],
        meshes: &[MeshDraw],
        directional_light: ([f32; 3], [f32; 3]),
        point_lights: &[render_api::PointLight],
        spot_lights: &[render_api::SpotLight],
        light_view_proj: Option<&[f32; 16]>,
    ) -> Result<wgpu::CommandBuffer, String> {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("lumelite_frame") });
        self.encode_frame(&mut encoder, width, height, view_proj, inv_view_proj, meshes, directional_light, point_lights, spot_lights, light_view_proj)?;
        Ok(encoder.finish())
    }

    pub fn submit(&self, command_buffers: impl IntoIterator<Item = wgpu::CommandBuffer>) {
        self.queue.submit(command_buffers);
    }
}
