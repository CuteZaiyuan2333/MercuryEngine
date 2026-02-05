//! Lume Renderer: High-level rendering logic.
//! Implements Virtual Geometry, Global Illumination, and Render Graph.

use lume_rhi::{CommandBuffer, Device};
use std::sync::Arc;

pub mod gi;
pub mod graph;
pub mod virtual_geom;

pub use graph::{RenderGraph, RenderGraphNode, ResourceHandle, ResourceId as GraphResourceId, NodeId};

pub struct Renderer {
    device: Arc<dyn Device>,
    graph: graph::RenderGraph,
}

impl Renderer {
    pub fn new(device: Arc<dyn Device>) -> Self {
        Self {
            device,
            graph: graph::RenderGraph::new(),
        }
    }

    /// Access the render graph to add nodes and resources.
    pub fn graph_mut(&mut self) -> &mut graph::RenderGraph {
        &mut self.graph
    }

    /// Execute the render graph and return command buffers (caller typically submits via Device::submit).
    pub fn render_frame(&mut self) -> Result<Vec<Box<dyn CommandBuffer>>, String> {
        self.graph.execute(&self.device)
    }
}