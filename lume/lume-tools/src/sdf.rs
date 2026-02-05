//! Mesh SDF generation for GI (signed distance field). Offline preprocessing.

/// Output of mesh SDF generation: a 3D grid of signed distances.
#[derive(Clone, Debug)]
pub struct MeshSdfOutput {
    /// Grid resolution (e.g. 32 or 64 per axis).
    pub resolution: (u32, u32, u32),
    /// Signed distance values, row-major, then slice. Length = resolution.0 * resolution.1 * resolution.2.
    pub data: Vec<f32>,
}

/// Generate a low-resolution SDF for a mesh (vertices + indices).
/// TODO: Implement actual SDF baking (e.g. voxelize mesh, compute distances).
pub fn generate_mesh_sdf(
    _positions: &[f32],
    _indices: &[u32],
    resolution: u32,
) -> MeshSdfOutput {
    let n = (resolution as usize) * (resolution as usize) * (resolution as usize);
    MeshSdfOutput {
        resolution: (resolution, resolution, resolution),
        data: vec![f32::MAX; n],
    }
}
