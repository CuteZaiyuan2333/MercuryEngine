//! Vulkan Render Pass creation and recording.

use crate::{DescriptorSet, IndexFormat, LoadOp, StoreOp};
use ash::vk;
use std::sync::Arc;

use super::buffer::VulkanBuffer;
use super::descriptor::VulkanDescriptorSet;
use super::pipeline::VulkanGraphicsPipeline;
use super::texture::texture_format_to_vk;

/// Create a VkRenderPass from attachment formats and load/store ops.
/// Used by both pipeline creation and begin_render_pass.
pub fn create_vk_render_pass(
    device: &ash::Device,
    color_attachments: &[ColorAttachmentInfo],
    depth_attachment: Option<&DepthAttachmentInfo>,
) -> Result<vk::RenderPass, String> {
    let mut attachments = Vec::new();
    let mut color_refs = Vec::new();
    let mut depth_ref = None;

    for (i, att) in color_attachments.iter().enumerate() {
        let (load_op, store_op) = (load_op_to_vk(att.load_op), store_op_to_vk(att.store_op));
        let format = texture_format_to_vk(att.format);
        attachments.push(
            vk::AttachmentDescription::default()
                .format(format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(load_op)
                .store_op(store_op)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
        );
        color_refs.push(
            vk::AttachmentReference::default()
                .attachment(i as u32)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
        );
    }

    if let Some(dep) = depth_attachment {
        let idx = attachments.len();
        attachments.push(
            vk::AttachmentDescription::default()
                .format(texture_format_to_vk(dep.format))
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(load_op_to_vk(dep.depth_load_op))
                .store_op(store_op_to_vk(dep.depth_store_op))
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
        );
        depth_ref = Some(
            vk::AttachmentReference::default()
                .attachment(idx as u32)
                .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
        );
    }

    let subpass = if let Some(ref d) = depth_ref {
        vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_refs)
            .depth_stencil_attachment(d)
    } else {
        vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_refs)
    };

    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(std::slice::from_ref(&subpass));

    unsafe {
        device
            .create_render_pass(&create_info, None)
            .map_err(|e| e.to_string())
    }
}

pub struct ColorAttachmentInfo {
    pub format: crate::TextureFormat,
    pub load_op: LoadOp,
    pub store_op: StoreOp,
}

pub struct DepthAttachmentInfo {
    pub format: crate::TextureFormat,
    pub depth_load_op: LoadOp,
    pub depth_store_op: StoreOp,
}

fn load_op_to_vk(op: LoadOp) -> vk::AttachmentLoadOp {
    match op {
        LoadOp::Load => vk::AttachmentLoadOp::LOAD,
        LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
    }
}

fn store_op_to_vk(op: StoreOp) -> vk::AttachmentStoreOp {
    match op {
        StoreOp::Store => vk::AttachmentStoreOp::STORE,
        StoreOp::DontCare => vk::AttachmentStoreOp::DONT_CARE,
    }
}

/// Vulkan render pass recording - implements RenderPass trait.
pub struct VulkanRenderPassRecorder {
    pub(crate) device: Arc<ash::Device>,
    pub(crate) command_buffer: vk::CommandBuffer,
    pub(crate) render_pass: vk::RenderPass,
    pub(crate) framebuffer: vk::Framebuffer,
    pub(crate) extent: vk::Extent2D,
    pub(crate) pipeline_bound: Option<vk::Pipeline>,
    pub(crate) pipeline_layout: Option<vk::PipelineLayout>,
    pub(crate) vertex_buffers: Vec<Option<(vk::Buffer, u64)>>,
    pub(crate) index_buffer: Option<(vk::Buffer, u64, vk::IndexType)>,
}

impl VulkanRenderPassRecorder {
    pub fn new(
        device: Arc<ash::Device>,
        command_buffer: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffer: vk::Framebuffer,
        extent: vk::Extent2D,
    ) -> Self {
        Self {
            device,
            command_buffer,
            render_pass,
            framebuffer,
            extent,
            pipeline_bound: None,
            pipeline_layout: None,
            vertex_buffers: vec![],
            index_buffer: None,
        }
    }
}

impl crate::RenderPass for VulkanRenderPassRecorder {
    fn set_pipeline(&mut self, pipeline: &dyn crate::GraphicsPipeline) {
        if let Some(vk_pipe) = pipeline
            .as_any()
            .downcast_ref::<VulkanGraphicsPipeline>()
        {
            unsafe {
                self.device.cmd_bind_pipeline(
                    self.command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    vk_pipe.pipeline,
                );
                // Pipeline uses dynamic viewport/scissor; set default to full extent to avoid undefined behavior.
                let viewport = vk::Viewport::default()
                    .width(self.extent.width as f32)
                    .height(self.extent.height as f32)
                    .max_depth(1.0);
                self.device.cmd_set_viewport(self.command_buffer, 0, &[viewport]);
                let scissor = vk::Rect2D::default()
                    .offset(vk::Offset2D { x: 0, y: 0 })
                    .extent(self.extent);
                self.device.cmd_set_scissor(self.command_buffer, 0, &[scissor]);
            }
            self.pipeline_bound = Some(vk_pipe.pipeline);
            self.pipeline_layout = Some(vk_pipe.layout);
        }
    }

    fn bind_descriptor_set(&mut self, set_index: u32, set: &dyn DescriptorSet) {
        if let Some(layout) = self.pipeline_layout {
            if let Some(vk_set) = set.as_any().downcast_ref::<VulkanDescriptorSet>() {
                unsafe {
                    self.device.cmd_bind_descriptor_sets(
                        self.command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        layout,
                        set_index,
                        &[vk_set.set],
                        &[],
                    );
                }
            }
        }
    }

    fn set_vertex_buffer(&mut self, index: u32, buffer: &dyn crate::Buffer, offset: u64) {
        let vk_buf = buffer
            .as_any()
            .downcast_ref::<VulkanBuffer>()
            .expect("Buffer must be VulkanBuffer");
        let vk_buffer = vk_buf.buffer;
        while self.vertex_buffers.len() <= index as usize {
            self.vertex_buffers.push(None);
        }
        self.vertex_buffers[index as usize] = Some((vk_buffer, offset));
        unsafe {
            self.device.cmd_bind_vertex_buffers(
                self.command_buffer,
                index,
                &[vk_buffer],
                &[offset],
            );
        }
    }

    fn set_index_buffer(&mut self, buffer: &dyn crate::Buffer, offset: u64, index_format: IndexFormat) {
        let vk_buf = buffer
            .as_any()
            .downcast_ref::<VulkanBuffer>()
            .expect("Buffer must be VulkanBuffer");
        let index_type = match index_format {
            IndexFormat::Uint16 => vk::IndexType::UINT16,
            IndexFormat::Uint32 => vk::IndexType::UINT32,
        };
        self.index_buffer = Some((vk_buf.buffer, offset, index_type));
        unsafe {
            self.device.cmd_bind_index_buffer(
                self.command_buffer,
                vk_buf.buffer,
                offset,
                index_type,
            );
        }
    }

    fn draw(
        &mut self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        unsafe {
            self.device.cmd_draw(
                self.command_buffer,
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            );
        }
    }

    fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        unsafe {
            self.device.cmd_draw_indexed(
                self.command_buffer,
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
    }

    fn draw_indexed_indirect(&mut self, buffer: &dyn crate::Buffer, offset: u64) {
        let vk_buf = buffer
            .as_any()
            .downcast_ref::<VulkanBuffer>()
            .expect("Buffer must be VulkanBuffer");
        unsafe {
            self.device.cmd_draw_indexed_indirect(
                self.command_buffer,
                vk_buf.buffer,
                offset,
                1,
                std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32,
            );
        }
    }

    fn end(self: Box<Self>) {
        unsafe {
            self.device.cmd_end_render_pass(self.command_buffer);
            self.device.destroy_framebuffer(self.framebuffer, None);
            self.device.destroy_render_pass(self.render_pass, None);
        }
    }
}

impl std::fmt::Debug for VulkanRenderPassRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanRenderPassRecorder")
            .field("extent", &self.extent)
            .finish_non_exhaustive()
    }
}
