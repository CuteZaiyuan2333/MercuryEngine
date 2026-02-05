//! Vulkan Graphics Pipeline.

use crate::{
    BlendOp, CullMode, FrontFace, GraphicsPipeline, GraphicsPipelineDescriptor, PolygonMode,
    PrimitiveTopology, VertexFormat, VertexInputRate,
};
use ash::vk;
use std::ffi::CString;

use super::super::descriptor;
use super::super::render_pass::{ColorAttachmentInfo, DepthAttachmentInfo};
use super::super::texture::texture_format_to_vk;

pub struct VulkanGraphicsPipeline {
    pub(crate) device: ash::Device,
    pub(crate) pipeline: vk::Pipeline,
    pub(crate) layout: vk::PipelineLayout,
    pub(crate) render_pass: vk::RenderPass,
    #[allow(dead_code)]
    pub(crate) _set_layout: Option<descriptor::VulkanDescriptorSetLayout>,
}

impl VulkanGraphicsPipeline {
    pub fn create(device: &ash::Device, desc: &GraphicsPipelineDescriptor) -> Result<Self, String> {
        let color_attachments: Vec<ColorAttachmentInfo> = desc
            .color_targets
            .iter()
            .map(|t| ColorAttachmentInfo {
                format: t.format,
                load_op: crate::LoadOp::Load,
                store_op: crate::StoreOp::Store,
            })
            .collect();

        let depth_attachment = desc.depth_stencil.as_ref().map(|ds| DepthAttachmentInfo {
            format: ds.format,
            depth_load_op: crate::LoadOp::Load,
            depth_store_op: crate::StoreOp::Store,
        });

        let render_pass = super::super::render_pass::create_vk_render_pass(
            device,
            &color_attachments,
            depth_attachment.as_ref(),
        )?;
        let mut stage_modules = Vec::new();
        let mut entry_names: Vec<CString> = Vec::new();

        let vs_module = Self::create_shader_module(device, &desc.vertex_shader.source)?;
        stage_modules.push(vs_module);
        entry_names.push(CString::new(desc.vertex_shader.entry_point.as_str()).map_err(|e| e.to_string())?);

        if let Some(ref fs) = desc.fragment_shader {
            let fs_module = Self::create_shader_module(device, &fs.source)?;
            stage_modules.push(fs_module);
            entry_names.push(CString::new(fs.entry_point.as_str()).map_err(|e| e.to_string())?);
        }

        let mut stages = Vec::new();
        stages.push(
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(stage_modules[0])
                .name(&entry_names[0]),
        );
        if desc.fragment_shader.is_some() {
            stages.push(
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(stage_modules[1])
                    .name(&entry_names[1]),
            );
        }

        let (binding_descriptions, attribute_descriptions) = Self::vertex_input_descriptions(&desc.vertex_input);
        let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&binding_descriptions)
            .vertex_attribute_descriptions(&attribute_descriptions);
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(Self::topology_to_vk(desc.primitive_topology))
            .primitive_restart_enable(false);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(Self::polygon_mode_to_vk(desc.rasterization.polygon_mode))
            .line_width(1.0)
            .cull_mode(Self::cull_mode_to_vk(desc.rasterization.cull_mode))
            .front_face(Self::front_face_to_vk(desc.rasterization.front_face))
            .depth_bias_enable(false);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let _color_formats: Vec<vk::Format> = desc
            .color_targets
            .iter()
            .map(|t| texture_format_to_vk(t.format))
            .collect();

        let color_blend_attachments: Vec<vk::PipelineColorBlendAttachmentState> = desc
            .color_targets
            .iter()
            .map(|t| {
                let blend = t.blend.as_ref().map_or(
                    vk::PipelineColorBlendAttachmentState::default()
                        .blend_enable(false)
                        .color_write_mask(vk::ColorComponentFlags::RGBA),
                    |b| {
                        vk::PipelineColorBlendAttachmentState::default()
                            .blend_enable(true)
                            .src_color_blend_factor(Self::blend_factor_to_vk(b.color.src_factor))
                            .dst_color_blend_factor(Self::blend_factor_to_vk(b.color.dst_factor))
                            .color_blend_op(Self::blend_op_to_vk(b.color.operation))
                            .src_alpha_blend_factor(Self::blend_factor_to_vk(b.alpha.src_factor))
                            .dst_alpha_blend_factor(Self::blend_factor_to_vk(b.alpha.dst_factor))
                            .alpha_blend_op(Self::blend_op_to_vk(b.alpha.operation))
                            .color_write_mask(vk::ColorComponentFlags::RGBA)
                    },
                );
                blend
            })
            .collect();

        let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let depth_stencil_create_info = desc.depth_stencil.as_ref().map_or(
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(false)
                .depth_write_enable(false)
                .stencil_test_enable(false),
            |ds| {
                vk::PipelineDepthStencilStateCreateInfo::default()
                    .depth_test_enable(true)
                    .depth_write_enable(ds.depth_write_enabled)
                    .depth_compare_op(Self::compare_op_to_vk(ds.depth_compare))
                    .depth_bounds_test_enable(false)
                    .stencil_test_enable(false)
            },
        );

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let (pipeline_layout, _set_layout) = if desc.layout_bindings.is_empty() {
            let layout_create_info = vk::PipelineLayoutCreateInfo::default();
            let layout = unsafe {
                device
                    .create_pipeline_layout(&layout_create_info, None)
                    .map_err(|e| e.to_string())?
            };
            (layout, None)
        } else {
            let ds_layout = descriptor::create_descriptor_set_layout(device, &desc.layout_bindings)
                .map_err(|e| e.to_string())?;
            let layout_create_info = vk::PipelineLayoutCreateInfo::default()
                .set_layouts(std::slice::from_ref(&ds_layout.layout));
            let layout = unsafe {
                device
                    .create_pipeline_layout(&layout_create_info, None)
                    .map_err(|e| e.to_string())?
            };
            (layout, Some(ds_layout))
        };

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blend)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0)
            .depth_stencil_state(&depth_stencil_create_info)
            .dynamic_state(&dynamic_state);

        let pipelines = unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    &[pipeline_info],
                    None,
                )
                .map_err(|(_partial, res)| format!("{:?}", res))?
        };
        let pipeline = pipelines[0];

        for module in stage_modules {
            unsafe {
                device.destroy_shader_module(module, None);
            }
        }

        Ok(Self {
            device: device.clone(),
            pipeline,
            layout: pipeline_layout,
            render_pass,
            _set_layout,
        })
    }

    fn create_shader_module(device: &ash::Device, source: &[u8]) -> Result<vk::ShaderModule, String> {
        if source.len() % 4 != 0 {
            return Err("SPIR-V must be 4-byte aligned".to_string());
        }
        let code_u32: Vec<u32> = source
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let create_info = vk::ShaderModuleCreateInfo::default().code(&code_u32);
        unsafe {
            device
                .create_shader_module(&create_info, None)
                .map_err(|e| e.to_string())
        }
    }

    fn vertex_input_descriptions(
        desc: &crate::VertexInputDescriptor,
    ) -> (
        Vec<vk::VertexInputBindingDescription>,
        Vec<vk::VertexInputAttributeDescription>,
    ) {
        let binding_descriptions: Vec<vk::VertexInputBindingDescription> = desc
            .bindings
            .iter()
            .map(|b| {
                vk::VertexInputBindingDescription::default()
                    .binding(b.binding)
                    .stride(b.stride)
                    .input_rate(match b.input_rate {
                        VertexInputRate::Vertex => vk::VertexInputRate::VERTEX,
                        VertexInputRate::Instance => vk::VertexInputRate::INSTANCE,
                    })
            })
            .collect();

        let attribute_descriptions: Vec<vk::VertexInputAttributeDescription> = desc
            .attributes
            .iter()
            .map(|a| {
                vk::VertexInputAttributeDescription::default()
                    .location(a.location)
                    .binding(a.binding)
                    .format(Self::vertex_format_to_vk(a.format))
                    .offset(a.offset)
            })
            .collect();

        (binding_descriptions, attribute_descriptions)
    }

    fn vertex_format_to_vk(f: VertexFormat) -> vk::Format {
        match f {
            VertexFormat::Float32x3 => vk::Format::R32G32B32_SFLOAT,
            VertexFormat::Float32x2 => vk::Format::R32G32_SFLOAT,
            VertexFormat::Float32x4 => vk::Format::R32G32B32A32_SFLOAT,
            VertexFormat::Uint32 => vk::Format::R32_UINT,
        }
    }

    fn topology_to_vk(t: PrimitiveTopology) -> vk::PrimitiveTopology {
        match t {
            PrimitiveTopology::TriangleList => vk::PrimitiveTopology::TRIANGLE_LIST,
            PrimitiveTopology::TriangleStrip => vk::PrimitiveTopology::TRIANGLE_STRIP,
            PrimitiveTopology::LineList => vk::PrimitiveTopology::LINE_LIST,
            PrimitiveTopology::PointList => vk::PrimitiveTopology::POINT_LIST,
        }
    }

    fn polygon_mode_to_vk(p: PolygonMode) -> vk::PolygonMode {
        match p {
            PolygonMode::Fill => vk::PolygonMode::FILL,
            PolygonMode::Line => vk::PolygonMode::LINE,
            PolygonMode::Point => vk::PolygonMode::POINT,
        }
    }

    fn cull_mode_to_vk(c: CullMode) -> vk::CullModeFlags {
        match c {
            CullMode::None => vk::CullModeFlags::NONE,
            CullMode::Back => vk::CullModeFlags::BACK,
            CullMode::Front => vk::CullModeFlags::FRONT,
            CullMode::FrontAndBack => vk::CullModeFlags::FRONT_AND_BACK,
        }
    }

    fn front_face_to_vk(f: FrontFace) -> vk::FrontFace {
        match f {
            FrontFace::CounterClockwise => vk::FrontFace::COUNTER_CLOCKWISE,
            FrontFace::Clockwise => vk::FrontFace::CLOCKWISE,
        }
    }

    fn blend_factor_to_vk(f: crate::BlendFactor) -> vk::BlendFactor {
        match f {
            crate::BlendFactor::One => vk::BlendFactor::ONE,
            crate::BlendFactor::Zero => vk::BlendFactor::ZERO,
            crate::BlendFactor::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
            crate::BlendFactor::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            crate::BlendFactor::DstAlpha => vk::BlendFactor::DST_ALPHA,
            crate::BlendFactor::OneMinusDstAlpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
        }
    }

    fn blend_op_to_vk(o: BlendOp) -> vk::BlendOp {
        match o {
            BlendOp::Add => vk::BlendOp::ADD,
            BlendOp::Subtract => vk::BlendOp::SUBTRACT,
        }
    }

    fn compare_op_to_vk(o: crate::CompareOp) -> vk::CompareOp {
        match o {
            crate::CompareOp::Never => vk::CompareOp::NEVER,
            crate::CompareOp::Less => vk::CompareOp::LESS,
            crate::CompareOp::Equal => vk::CompareOp::EQUAL,
            crate::CompareOp::LessOrEqual => vk::CompareOp::LESS_OR_EQUAL,
            crate::CompareOp::Greater => vk::CompareOp::GREATER,
            crate::CompareOp::NotEqual => vk::CompareOp::NOT_EQUAL,
            crate::CompareOp::GreaterOrEqual => vk::CompareOp::GREATER_OR_EQUAL,
            crate::CompareOp::Always => vk::CompareOp::ALWAYS,
        }
    }
}

impl Drop for VulkanGraphicsPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
            self.device.destroy_render_pass(self.render_pass, None);
            // _set_layout drops and destroys descriptor set layout
        }
    }
}

impl std::fmt::Debug for VulkanGraphicsPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanGraphicsPipeline").finish()
    }
}

impl GraphicsPipeline for VulkanGraphicsPipeline {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
