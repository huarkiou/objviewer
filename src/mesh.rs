// Mesh data and conversion from parsed OBJ data
// Ported from MeshDynamicLoad.cs

use crate::parser::ObjData;

/// Vertex format for GPU: position (3), normal (3) = 6 floats
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Build mesh from parsed OBJ data.
    /// Centers the model at origin and scales to fit within a unit sphere (radius = 1).
    /// Auto-generates normals if not present in OBJ file.
    pub fn from_obj(obj: &ObjData) -> Self {
        if obj.positions.is_empty() {
            return Self {
                vertices: Vec::new(),
                indices: Vec::new(),
            };
        }

        // Compute bounding box and center
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in &obj.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }

        let center = [
            (min[0] + max[0]) / 2.0,
            (min[1] + max[1]) / 2.0,
            (min[2] + max[2]) / 2.0,
        ];

        // Compute bounding sphere radius (max distance from center)
        let max_dist = obj
            .positions
            .iter()
            .map(|p| {
                let dx = p[0] - center[0];
                let dy = p[1] - center[1];
                let dz = p[2] - center[2];
                (dx * dx + dy * dy + dz * dz).sqrt()
            })
            .fold(0.0f32, f32::max);

        let inv_radius = if max_dist > f32::EPSILON {
            1.0 / max_dist
        } else {
            1.0
        };

        // Build vertices: center at origin, scaled to unit sphere
        let mut vertices: Vec<Vertex> = obj
            .positions
            .iter()
            .map(|p| Vertex {
                position: [
                    (p[0] - center[0]) * inv_radius,
                    (p[1] - center[1]) * inv_radius,
                    (p[2] - center[2]) * inv_radius,
                ],
                normal: [0.0, 0.0, 0.0],
            })
            .collect();

        // Build index buffer (triangles only: first 3 vertices per face)
        let mut indices: Vec<u32> = Vec::with_capacity(obj.faces.len() * 3);
        for face in &obj.faces {
            if face.v_idx.len() >= 3 {
                for j in 0..3 {
                    let idx = face.v_idx[j];
                    if idx >= 0 {
                        indices.push(idx as u32);
                    }
                }
            }
        }

        // Set normals: use from file if available and count matches, else generate
        let has_valid_normals = obj.has_normals && obj.normals.len() == obj.positions.len();

        if has_valid_normals {
            for (i, n) in obj.normals.iter().enumerate() {
                vertices[i].normal = [n[0], n[1], n[2]];
            }
        } else {
            generate_normals(&mut vertices, &indices);
        }

        Self { vertices, indices }
    }
}

/// Generate smooth vertex normals by averaging face normals.
fn generate_normals(vertices: &mut [Vertex], indices: &[u32]) {
    // Reset normals
    for v in vertices.iter_mut() {
        v.normal = [0.0, 0.0, 0.0];
    }

    // Accumulate face normals to each vertex
    for chunk in indices.chunks(3) {
        if chunk.len() < 3 {
            continue;
        }
        let i0 = chunk[0] as usize;
        let i1 = chunk[1] as usize;
        let i2 = chunk[2] as usize;

        if i0 >= vertices.len() || i1 >= vertices.len() || i2 >= vertices.len() {
            continue;
        }

        let p0 = vertices[i0].position;
        let p1 = vertices[i1].position;
        let p2 = vertices[i2].position;

        // Compute face normal
        let edge1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let edge2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];

        let nx = edge1[1] * edge2[2] - edge1[2] * edge2[1];
        let ny = edge1[2] * edge2[0] - edge1[0] * edge2[2];
        let nz = edge1[0] * edge2[1] - edge1[1] * edge2[0];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();

        if len < 1e-10 {
            continue;
        }

        let face_normal = [nx / len, ny / len, nz / len];

        vertices[i0].normal[0] += face_normal[0];
        vertices[i0].normal[1] += face_normal[1];
        vertices[i0].normal[2] += face_normal[2];

        vertices[i1].normal[0] += face_normal[0];
        vertices[i1].normal[1] += face_normal[1];
        vertices[i1].normal[2] += face_normal[2];

        vertices[i2].normal[0] += face_normal[0];
        vertices[i2].normal[1] += face_normal[1];
        vertices[i2].normal[2] += face_normal[2];
    }

    // Normalize all accumulated normals
    for v in vertices.iter_mut() {
        let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
        if len > 1e-10 {
            v.normal[0] /= len;
            v.normal[1] /= len;
            v.normal[2] /= len;
        } else {
            v.normal = [0.0, 1.0, 0.0]; // default up
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Face, ObjData};

    fn make_triangle_obj() -> ObjData {
        ObjData {
            positions: vec![
                [0.0, 0.0, 0.0, 0.0],
                [2.0, 0.0, 0.0, 0.0],
                [1.0, 2.0, 0.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            tex_coords: Vec::new(),
            faces: vec![Face {
                v_idx: vec![0, 1, 2],
                n_idx: vec![0, 1, 2],
                tex_idx: vec![-1, -1, -1],
            }],
            has_normals: true,
            num_faces: 1,
            num_indices: 3,
        }
    }

    #[test]
    fn test_empty_obj() {
        let obj = ObjData {
            positions: Vec::new(),
            normals: Vec::new(),
            tex_coords: Vec::new(),
            faces: Vec::new(),
            has_normals: false,
            num_faces: 0,
            num_indices: 0,
        };
        let mesh = MeshData::from_obj(&obj);
        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn test_normals_from_file() {
        let obj = make_triangle_obj();
        let mesh = MeshData::from_obj(&obj);

        // Should use normals from file since has_normals=true and counts match
        for v in &mesh.vertices {
            assert!((v.normal[0] - 0.0).abs() < 0.001);
            assert!((v.normal[1] - 0.0).abs() < 0.001);
            assert!((v.normal[2] - 1.0).abs() < 0.001);
        }
    }

    #[test]
    fn test_auto_generate_normals() {
        let mut obj = make_triangle_obj();
        obj.has_normals = false;
        let mesh = MeshData::from_obj(&obj);

        // Should auto-generate normals
        for v in &mesh.vertices {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "normal not unit length: {:?}",
                v.normal
            );
        }
    }

    #[test]
    fn test_normalization() {
        let obj = make_triangle_obj();
        let mesh = MeshData::from_obj(&obj);

        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.indices.len(), 3);

        // Model should fit within unit sphere (radius <= 1)
        for v in &mesh.vertices {
            let dist =
                (v.position[0].powi(2) + v.position[1].powi(2) + v.position[2].powi(2)).sqrt();
            assert!(
                dist <= 1.0 + 0.001,
                "vertex outside unit sphere: dist={}",
                dist
            );
        }

        // Bounding box should be centered at origin
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in &mesh.vertices {
            for i in 0..3 {
                min[i] = min[i].min(v.position[i]);
                max[i] = max[i].max(v.position[i]);
            }
        }
        let center_x = (min[0] + max[0]) / 2.0;
        let center_y = (min[1] + max[1]) / 2.0;
        let center_z = (min[2] + max[2]) / 2.0;
        assert!(
            center_x.abs() < 0.001,
            "bbox not centered in X: {}",
            center_x
        );
        assert!(
            center_y.abs() < 0.001,
            "bbox not centered in Y: {}",
            center_y
        );
        assert!(
            center_z.abs() < 0.001,
            "bbox not centered in Z: {}",
            center_z
        );
    }

    #[test]
    fn test_vertex_format() {
        let obj = make_triangle_obj();
        let _mesh = MeshData::from_obj(&obj);

        // Verify Vertex is Pod (can be used in GPU buffer)
        let vertex_size = std::mem::size_of::<Vertex>();
        assert_eq!(vertex_size, 24); // 6 * f32 = 24 bytes
    }
}
