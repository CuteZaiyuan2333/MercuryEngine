//! Lumelite Render Graph: task dependency ordering (wgpu-based).

use std::collections::HashMap;
use wgpu::CommandEncoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceUsage {
    Read,
    Write,
    ReadWrite,
}

impl ResourceUsage {
    pub fn is_write(&self) -> bool {
        matches!(self, ResourceUsage::Write | ResourceUsage::ReadWrite)
    }
    pub fn is_read(&self) -> bool {
        matches!(self, ResourceUsage::Read | ResourceUsage::ReadWrite)
    }
}

#[derive(Debug, Clone)]
pub struct TextureBarrierHint {
    pub need_usage: wgpu::TextureUsages,
    pub after_pass_usage: Option<wgpu::TextureUsages>,
}

pub trait RenderGraphNode: Send + Sync {
    fn encode(
        &self,
        encoder: &mut CommandEncoder,
        resources: &HashMap<ResourceId, &ResourceHandle>,
        device: &wgpu::Device,
    ) -> Result<(), String>;
}

pub enum ResourceHandle {
    Buffer(wgpu::Buffer),
    Texture { texture: wgpu::Texture, view: wgpu::TextureView },
}

impl ResourceHandle {
    pub fn buffer(&self) -> Option<&wgpu::Buffer> {
        match self {
            ResourceHandle::Buffer(b) => Some(b),
            _ => None,
        }
    }
    pub fn texture_view(&self) -> Option<&wgpu::TextureView> {
        match self {
            ResourceHandle::Texture { view, .. } => Some(view),
            _ => None,
        }
    }
}

pub struct RenderGraph {
    nodes: Vec<Box<dyn RenderGraphNode>>,
    node_resource_usage: Vec<Vec<(ResourceId, ResourceUsage, Option<TextureBarrierHint>)>>,
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
    pub fn new() -> Self { Self::default() }
    pub fn add_node(&mut self, node: Box<dyn RenderGraphNode>, resource_usage: Vec<(ResourceId, ResourceUsage, Option<TextureBarrierHint>)>) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        self.nodes.push(node);
        self.node_resource_usage.push(resource_usage);
        id
    }
    pub fn add_edge(&mut self, before: NodeId, after: NodeId) { self.edges.push((before, after)); }
    pub fn add_resource(&mut self, handle: ResourceHandle) -> ResourceId {
        let id = ResourceId(self.next_resource_id);
        self.next_resource_id += 1;
        self.resources.insert(id, handle);
        id
    }
    fn topological_order(&self) -> Result<Vec<usize>, String> {
        let n = self.nodes.len();
        let mut in_degree = vec![0usize; n];
        let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(NodeId(a), NodeId(b)) in &self.edges {
            if a < n && b < n { in_degree[b] += 1; out_edges[a].push(b); }
        }
        let mut stack: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(u) = stack.pop() {
            order.push(u);
            for &v in &out_edges[u] {
                in_degree[v] -= 1;
                if in_degree[v] == 0 { stack.push(v); }
            }
        }
        if order.len() != n { return Err("Render graph has a cycle".to_string()); }
        Ok(order)
    }
    pub fn execute(&self, device: &wgpu::Device) -> Result<wgpu::CommandBuffer, String> {
        let order = self.topological_order()?;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("lumelite_render_graph") });
        let resource_refs: HashMap<ResourceId, &ResourceHandle> = self.resources.iter().map(|(k, v)| (*k, v)).collect();
        for index in order {
            self.nodes[index].encode(&mut encoder, &resource_refs, device)?;
        }
        Ok(encoder.finish())
    }
}
