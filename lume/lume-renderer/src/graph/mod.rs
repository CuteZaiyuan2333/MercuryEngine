//! Lume Render Graph: task dependency ordering and resource lifecycle.
//!
//! Nodes run in topological order. **Buffer** barriers are inserted automatically when a resource
//! is written by one node and read or written by a later node (`pipeline_barrier_buffer`).
//!
//! **Texture** barriers are optional: when a node declares a [`TextureBarrierHint`] for a texture
//! resource, the graph will insert `pipeline_barrier_texture` before that node if a previous node
//! wrote to the texture, transitioning from the tracked layout to `need_layout`. If no hint is
//! given for a texture, nodes must perform layout transitions themselves (dependency ordering
//! is still enforced).

use lume_rhi::{CommandBuffer, Device, ImageLayout};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Identifier for a node in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// Identifier for a resource slot (buffer or texture) in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(pub usize);

/// Usage of a resource by a node. Used to insert automatic pipeline_barrier_* between nodes
/// when a resource is written by one node and read by another (e.g. compute write → fragment read).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceUsage {
    Read,
    Write,
    ReadWrite,
}

impl ResourceUsage {
    /// True if this usage may write to the resource (needs barrier after a previous writer).
    pub fn is_write(&self) -> bool {
        matches!(self, ResourceUsage::Write | ResourceUsage::ReadWrite)
    }
    /// True if this usage reads the resource (needs barrier after a previous writer).
    pub fn is_read(&self) -> bool {
        matches!(self, ResourceUsage::Read | ResourceUsage::ReadWrite)
    }
}

/// Optional hint for texture resources so the graph can insert `pipeline_barrier_texture` automatically.
/// When a previous node wrote to the texture, the graph will transition it from the tracked layout
/// to `need_layout` before this node runs. If this node writes, set `after_pass_layout` so the
/// next node's barrier uses the correct source layout.
#[derive(Debug, Clone)]
pub struct TextureBarrierHint {
    /// Layout the texture must be in when this node runs (barrier target).
    pub need_layout: ImageLayout,
    /// Layout the texture will be in after this node (for writers). If None, tracked layout stays `need_layout`.
    pub after_pass_layout: Option<ImageLayout>,
}

/// A single node in the render graph. Executes and returns command buffers.
pub trait RenderGraphNode: Send + Sync {
    /// Run the node; may record commands and return command buffers.
    fn execute(
        &self,
        device: &Arc<dyn Device>,
        resources: &HashMap<ResourceId, &ResourceHandle>,
    ) -> Vec<Box<dyn CommandBuffer>>;
}

/// Handle to a graph-managed resource (buffer or texture).
pub enum ResourceHandle {
    Buffer(Box<dyn lume_rhi::Buffer>),
    Texture(Box<dyn lume_rhi::Texture>),
}

/// Builds and executes the render graph.
pub struct RenderGraph {
    nodes: Vec<Box<dyn RenderGraphNode>>,
    /// Per-node resource usage for automatic barrier insertion. Third element is optional texture barrier hint.
    node_resource_usage: Vec<Vec<(ResourceId, ResourceUsage, Option<TextureBarrierHint>)>>,
    /// Edges: (from, to) means from runs before to.
    edges: Vec<(NodeId, NodeId)>,
    resources: HashMap<ResourceId, ResourceHandle>,
    next_node_id: usize,
    next_resource_id: usize,
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            node_resource_usage: Vec::new(),
            edges: Vec::new(),
            resources: HashMap::new(),
            next_node_id: 0,
            next_resource_id: 0,
        }
    }
}

impl RenderGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node and return its id. `resource_usage` declares which resources this node reads/writes
    /// so the graph can insert automatic pipeline barriers between nodes. For texture resources,
    /// pass `Some(TextureBarrierHint)` to have the graph insert `pipeline_barrier_texture` automatically.
    pub fn add_node(
        &mut self,
        node: Box<dyn RenderGraphNode>,
        resource_usage: Vec<(ResourceId, ResourceUsage, Option<TextureBarrierHint>)>,
    ) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        self.nodes.push(node);
        self.node_resource_usage.push(resource_usage);
        id
    }

    /// Add a dependency: `before` runs before `after`.
    pub fn add_edge(&mut self, before: NodeId, after: NodeId) {
        self.edges.push((before, after));
    }

    /// Register a resource for use by nodes.
    pub fn add_resource(&mut self, handle: ResourceHandle) -> ResourceId {
        let id = ResourceId(self.next_resource_id);
        self.next_resource_id += 1;
        self.resources.insert(id, handle);
        id
    }

    /// Topological sort of node indices by edges. Returns indices in execution order.
    fn topological_order(&self) -> Result<Vec<usize>, String> {
        let n = self.nodes.len();
        let mut in_degree = vec![0usize; n];
        let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(NodeId(a), NodeId(b)) in &self.edges {
            if a < n && b < n {
                in_degree[b] += 1;
                out_edges[a].push(b);
            }
        }
        let mut stack: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(u) = stack.pop() {
            order.push(u);
            for &v in &out_edges[u] {
                in_degree[v] -= 1;
                if in_degree[v] == 0 {
                    stack.push(v);
                }
            }
        }
        if order.len() != n {
            return Err("Render graph has a cycle".to_string());
        }
        Ok(order)
    }

    /// Execute the graph in dependency order; returns all command buffers from all nodes.
    /// Inserts `pipeline_barrier_buffer` between nodes when a buffer was written by a previous node
    /// and is read or written by the current node. For texture resources with a [`TextureBarrierHint`],
    /// inserts `pipeline_barrier_texture` from the tracked layout to `need_layout` when a previous
    /// node wrote the texture.
    pub fn execute(&self, device: &Arc<dyn Device>) -> Result<Vec<Box<dyn CommandBuffer>>, String> {
        let order = self.topological_order()?;
        let mut all_cmds = Vec::new();
        let mut resources_written: HashSet<ResourceId> = HashSet::new();
        let mut texture_layout: HashMap<ResourceId, ImageLayout> = HashMap::new();
        for index in order {
            let usage = self
                .node_resource_usage
                .get(index)
                .map(|u| u.as_slice())
                .unwrap_or(&[]);
            let mut need_buffer_barrier: Vec<ResourceId> = Vec::new();
            let mut need_texture_barriers: Vec<(ResourceId, ImageLayout, ImageLayout)> = Vec::new();
            for (rid, ru, hint_opt) in usage {
                if !ru.is_read() && !ru.is_write() {
                    continue;
                }
                if resources_written.contains(rid) {
                    if let Some(ResourceHandle::Buffer(_)) = self.resources.get(rid) {
                        need_buffer_barrier.push(*rid);
                    } else if let Some(ResourceHandle::Texture(_)) = self.resources.get(rid) {
                        if let Some(ref hint) = hint_opt {
                            let old = texture_layout.get(rid).copied().unwrap_or(ImageLayout::Undefined);
                            if old != hint.need_layout {
                                need_texture_barriers.push((*rid, old, hint.need_layout));
                            }
                        }
                    }
                }
            }
            if !need_buffer_barrier.is_empty() || !need_texture_barriers.is_empty() {
                let mut encoder = device.create_command_encoder()?;
                for rid in need_buffer_barrier {
                    if let Some(ResourceHandle::Buffer(ref b)) = self.resources.get(&rid) {
                        let size = b.size();
                        encoder.pipeline_barrier_buffer(b.as_ref(), 0, size);
                    }
                }
                for (rid, old_layout, new_layout) in need_texture_barriers {
                    if let Some(ResourceHandle::Texture(ref t)) = self.resources.get(&rid) {
                        encoder.pipeline_barrier_texture(t.as_ref(), old_layout, new_layout);
                    }
                }
                let barrier_cmd = encoder.finish()?;
                all_cmds.push(barrier_cmd);
            }
            let node = &self.nodes[index];
            let resource_refs: HashMap<ResourceId, &ResourceHandle> = self
                .resources
                .iter()
                .map(|(k, v)| (*k, v))
                .collect();
            let cmds = node.execute(device, &resource_refs);
            all_cmds.extend(cmds);
            for (rid, ru, hint_opt) in usage {
                if ru.is_write() {
                    resources_written.insert(*rid);
                    if let Some(ResourceHandle::Texture(_)) = self.resources.get(rid) {
                        if let Some(ref hint) = hint_opt {
                            let new_layout = hint.after_pass_layout.unwrap_or(hint.need_layout);
                            texture_layout.insert(*rid, new_layout);
                        }
                    }
                } else if let Some(ResourceHandle::Texture(_)) = self.resources.get(rid) {
                    if let Some(ref hint) = hint_opt {
                        texture_layout.insert(*rid, hint.need_layout);
                    }
                }
            }
        }
        Ok(all_cmds)
    }
}
