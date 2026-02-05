//! Lume Render Graph: task dependency ordering and resource lifecycle.
//! Nodes run in topological order; automatic barriers can be added when resources are shared.

use lume_rhi::{CommandBuffer, Device};
use std::collections::HashMap;
use std::sync::Arc;

/// Identifier for a node in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// Identifier for a resource slot (buffer or texture) in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(pub usize);

/// Usage of a resource by a node (for future barrier insertion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceUsage {
    Read,
    Write,
    ReadWrite,
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

    /// Add a node and return its id.
    pub fn add_node(&mut self, node: Box<dyn RenderGraphNode>) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        self.nodes.push(node);
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
    pub fn execute(&self, device: &Arc<dyn Device>) -> Result<Vec<Box<dyn CommandBuffer>>, String> {
        let order = self.topological_order()?;
        let mut all_cmds = Vec::new();
        for index in order {
            let node = &self.nodes[index];
            let resource_refs: HashMap<ResourceId, &ResourceHandle> = self
                .resources
                .iter()
                .map(|(k, v)| (*k, v))
                .collect();
            let cmds = node.execute(device, &resource_refs);
            all_cmds.extend(cmds);
        }
        Ok(all_cmds)
    }
}
