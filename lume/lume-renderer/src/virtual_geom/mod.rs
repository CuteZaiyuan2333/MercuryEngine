//! Virtual geometry: cluster-based mesh representation and culling (CPU path; GPU culling TODO).

use lume_rhi::{Buffer, BufferDescriptor, BufferUsage, Device};
use std::sync::Arc;

/// Represents a single cluster of triangles (e.g., 128 triangles).
#[derive(Clone, Debug)]
pub struct Cluster {
    pub vertex_offset: u32,
    pub index_offset: u32,
    pub triangle_count: u32,
    pub bounding_sphere: [f32; 4],
}

/// A high-level mesh made of multiple clusters. Buffers are typically created from lume-tools cluster output.
pub struct VirtualMesh {
    pub clusters: Vec<Cluster>,
    pub vertex_buffer: Box<dyn Buffer>,
    pub index_buffer: Box<dyn Buffer>,
}

/// One draw call in the indirect buffer (matches VkDrawIndexedIndirectCommand).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DrawIndexedIndirectCommand {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub vertex_offset: i32,
    pub first_instance: u32,
}

pub struct VirtualGeometryManager {
    device: Arc<dyn Device>,
    meshes: Vec<VirtualMesh>,
    /// Indirect buffer filled each frame by prepare_culling_pass (CPU culling path).
    indirect_buffer: Option<Box<dyn Buffer>>,
    /// Number of draw commands written to indirect_buffer.
    indirect_draw_count: u32,
}

impl VirtualGeometryManager {
    pub fn new(device: Arc<dyn Device>) -> Self {
        Self {
            device,
            meshes: Vec::new(),
            indirect_buffer: None,
            indirect_draw_count: 0,
        }
    }

    /// Registers a mesh. Buffers must be created by the caller (e.g. from lume-tools cluster output).
    pub fn upload_mesh(&mut self, mesh: VirtualMesh) {
        self.meshes.push(mesh);
    }

    /// CPU frustum culling (simplified: no frustum, accept all clusters) and fill indirect buffer.
    /// View-proj matrix can be used for proper frustum-sphere test in a follow-up.
    pub fn prepare_culling_pass(
        &mut self,
        _view_proj: [[f32; 4]; 4],
    ) -> Result<(), String> {
        let mut commands = Vec::<DrawIndexedIndirectCommand>::new();
        for mesh in &self.meshes {
            for cluster in &mesh.clusters {
                // TODO: frustum-sphere test using view_proj
                commands.push(DrawIndexedIndirectCommand {
                    index_count: cluster.triangle_count * 3,
                    instance_count: 1,
                    first_index: cluster.index_offset,
                    vertex_offset: cluster.vertex_offset as i32,
                    first_instance: 0,
                });
            }
        }
        self.indirect_draw_count = commands.len() as u32;
        if commands.is_empty() {
            self.indirect_buffer = None;
            return Ok(());
        }
        let size = (commands.len() * std::mem::size_of::<DrawIndexedIndirectCommand>()) as u64;
        // Reuse existing buffer when size is sufficient to avoid per-frame allocation.
        let buf = match self.indirect_buffer.as_ref() {
            Some(b) if b.size() >= size => self.indirect_buffer.take().unwrap(),
            _ => self.device.create_buffer(&BufferDescriptor {
                label: Some("vg_indirect"),
                size,
                usage: BufferUsage::INDIRECT,
                memory: lume_rhi::BufferMemoryPreference::HostVisible,
            })?,
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(
                commands.as_ptr() as *const u8,
                commands.len() * std::mem::size_of::<DrawIndexedIndirectCommand>(),
            )
        };
        self.device.write_buffer(buf.as_ref(), 0, bytes)?;
        self.indirect_buffer = Some(buf);
        Ok(())
    }

    /// Returns the indirect buffer and draw count for this frame (after prepare_culling_pass).
    pub fn indirect_draw_info(&self) -> (Option<&dyn Buffer>, u32) {
        (
            self.indirect_buffer.as_deref(),
            self.indirect_draw_count,
        )
    }

    /// All registered meshes (for iteration).
    pub fn meshes(&self) -> &[VirtualMesh] {
        &self.meshes
    }
}
