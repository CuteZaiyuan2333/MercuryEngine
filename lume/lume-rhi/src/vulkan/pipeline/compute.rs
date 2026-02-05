//! Vulkan Compute Pipeline.

use crate::{ComputePipeline, ComputePipelineDescriptor};
use ash::vk;
use std::ffi::CString;

use super::super::descriptor;

pub struct VulkanComputePipeline {
    pub(crate) device: ash::Device,
    pub(crate) pipeline: vk::Pipeline,
    pub(crate) layout: vk::PipelineLayout,
    pub(crate) _set_layout: Option<descriptor::VulkanDescriptorSetLayout>,
}

impl VulkanComputePipeline {
    pub fn create(device: &ash::Device, desc: &ComputePipelineDescriptor) -> Result<Self, String> {
        let code = desc.shader_source.as_bytes();
        if code.len() % 4 != 0 {
            return Err("SPIR-V must be 4-byte aligned".to_string());
        }
        let code_u32: Vec<u32> = code
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let shader_create_info = vk::ShaderModuleCreateInfo::default().code(&code_u32);
        let shader_module = unsafe {
            device
                .create_shader_module(&shader_create_info, None)
                .map_err(|e| e.to_string())?
        };

        let (pipeline_layout, set_layout) = if desc.layout_bindings.is_empty() {
            let layout_create_info = vk::PipelineLayoutCreateInfo::default();
            let layout = unsafe {
                device
                    .create_pipeline_layout(&layout_create_info, None)
                    .map_err(|e| e.to_string())?
            };
            (layout, None)
        } else {
            let ds_layout = descriptor::create_descriptor_set_layout(device, &desc.layout_bindings)?;
            let layout_create_info = vk::PipelineLayoutCreateInfo::default()
                .set_layouts(std::slice::from_ref(&ds_layout.layout));
            let layout = unsafe {
                device
                    .create_pipeline_layout(&layout_create_info, None)
                    .map_err(|e| e.to_string())?
            };
            (layout, Some(ds_layout))
        };
        let entry_name = CString::new(desc.entry_point.as_str()).map_err(|e| e.to_string())?;
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader_module)
            .name(&entry_name);
        let create_info =
            vk::ComputePipelineCreateInfo::default().stage(stage).layout(pipeline_layout);
        let pipelines = unsafe {
            device
                .create_compute_pipelines(vk::PipelineCache::null(), &[create_info], None)
                .map_err(|(_partial, res)| format!("{:?}", res))?
        };
        let pipeline = pipelines[0];
        unsafe {
            device.destroy_shader_module(shader_module, None);
        }
        Ok(Self {
            device: device.clone(),
            pipeline,
            layout: pipeline_layout,
            _set_layout: set_layout,
        })
    }
}

impl Drop for VulkanComputePipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

impl std::fmt::Debug for VulkanComputePipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanComputePipeline").finish()
    }
}

impl ComputePipeline for VulkanComputePipeline {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
