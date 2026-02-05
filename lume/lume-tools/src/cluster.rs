//! Cluster subdivision: split a mesh into clusters of ~N triangles for virtual geometry.

/// Descriptor for one cluster (matches lume-renderer virtual_geom::Cluster layout).
#[derive(Clone, Debug)]
pub struct ClusterDesc {
    pub vertex_offset: u32,
    pub index_offset: u32,
    pub triangle_count: u32,
    pub bounding_sphere: [f32; 4], // center x,y,z, radius
}

#[derive(Clone, Debug)]
pub struct SubdivideOptions {
    /// Max triangles per cluster (default 128).
    pub max_triangles_per_cluster: usize,
}

impl Default for SubdivideOptions {
    fn default() -> Self {
        Self {
            max_triangles_per_cluster: 128,
        }
    }
}

/// Subdivide a mesh into clusters. Vertices are position-only (3 floats) or position+normal (6 floats).
/// Indices are u32. Returns cluster descriptors; vertex and index data are unchanged (clusters reference into them).
pub fn subdivide_mesh(
    positions: &[f32],
    indices: &[u32],
    options: SubdivideOptions,
) -> Vec<ClusterDesc> {
    let max_tris = options.max_triangles_per_cluster.max(1);
    let num_triangles = indices.len() / 3;
    if num_triangles == 0 {
        return Vec::new();
    }
    let max_index = indices.iter().copied().max().unwrap_or(0) as usize;
    let vertex_count = max_index + 1;
    let stride = if vertex_count > 0 && positions.len() / vertex_count >= 6 {
        6
    } else {
        3
    };
    let mut clusters = Vec::new();
    let mut tri_offset = 0usize;
    while tri_offset < num_triangles {
        let batch = (num_triangles - tri_offset).min(max_tris);
        let index_offset = (tri_offset * 3) as u32;
        let triangle_count = batch as u32;
        let (vertex_offset, bounding_sphere) = bounding_sphere_for_tri_range(
            positions,
            indices,
            stride,
            tri_offset,
            tri_offset + batch,
        );
        clusters.push(ClusterDesc {
            vertex_offset,
            index_offset,
            triangle_count,
            bounding_sphere,
        });
        tri_offset += batch;
    }
    clusters
}

fn bounding_sphere_for_tri_range(
    positions: &[f32],
    indices: &[u32],
    stride: usize,
    tri_start: usize,
    tri_end: usize,
) -> (u32, [f32; 4]) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut min_z = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut max_z = f32::MIN;
    for t in tri_start..tri_end {
        let i0 = indices[t * 3] as usize;
        let i1 = indices[t * 3 + 1] as usize;
        let i2 = indices[t * 3 + 2] as usize;
        for &i in &[i0, i1, i2] {
            let x = positions[i * stride];
            let y = positions[i * stride + 1];
            let z = positions[i * stride + 2];
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            min_z = min_z.min(z);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            max_z = max_z.max(z);
        }
    }
    let cx = (min_x + max_x) * 0.5;
    let cy = (min_y + max_y) * 0.5;
    let cz = (min_z + max_z) * 0.5;
    let mut r2 = 0f32;
    for t in tri_start..tri_end {
        for k in 0..3 {
            let i = indices[t * 3 + k] as usize;
            let x = positions[i * stride];
            let y = positions[i * stride + 1];
            let z = positions[i * stride + 2];
            let dx = x - cx;
            let dy = y - cy;
            let dz = z - cz;
            r2 = r2.max(dx * dx + dy * dy + dz * dz);
        }
    }
    let radius = r2.sqrt();
    let vertex_offset = (tri_start * 3) as u32;
    (vertex_offset, [cx, cy, cz, radius])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdivide_small() {
        let positions = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5, 1.0, 0.0];
        let indices = [0u32, 1, 2];
        let opts = SubdivideOptions::default();
        let clusters = subdivide_mesh(&positions, &indices, opts);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].triangle_count, 1);
    }
}
