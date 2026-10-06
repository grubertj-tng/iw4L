//! One link's visual geometry as a game mesh: welded, simplified, with normals
//! that stay smooth across curved CAD surfaces and break at hard edges.

use bevy::math::{DVec3, Vec3};

use crate::stl::Triangle;
use crate::urdf::Visual;

/// How far the import may move a surface while simplifying, and where a
/// normal stops being shared.
#[derive(Clone, Copy, Debug)]
pub struct Detail {
    /// Absolute deviation allowed by simplification, in metres.
    pub max_error_m: f32,
    /// Simplification stops at this fraction of the source triangles even
    /// when the error budget would allow fewer.
    pub min_triangle_ratio: f32,
    /// Faces meeting at a sharper angle than this get separate normals.
    pub crease_degrees: f32,
}

impl Default for Detail {
    fn default() -> Self {
        Self {
            max_error_m: 0.0015,
            min_triangle_ratio: 0.02,
            crease_degrees: 40.0,
        }
    }
}

/// Indexed triangles in the link frame, metres. Triangles keep the source
/// winding: counter-clockwise seen from outside.
#[derive(Clone, Debug, Default)]
pub struct LinkMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub indices: Vec<u32>,
}

impl LinkMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// `soup` holds each visual's STL triangles in that visual's mesh frame.
pub fn build(visuals: &[(&Visual, Vec<Triangle>)], detail: Detail) -> LinkMesh {
    let mut corners: Vec<[f32; 3]> = Vec::new();
    for (visual, triangles) in visuals {
        for triangle in triangles {
            for corner in triangle {
                let local = DVec3::from_array(corner.map(f64::from)) * visual.scale;
                // `+ 0.0` folds -0.0 into 0.0 so the bitwise weld sees one point.
                corners.push(
                    visual
                        .origin
                        .apply(local)
                        .as_vec3()
                        .to_array()
                        .map(|x| x + 0.0),
                );
            }
        }
    }
    let (positions, indices) = weld(&corners);
    let indices = drop_degenerate(indices);
    let indices = simplify(&positions, &indices, detail);
    let mesh = crease_normals(&positions, &indices, detail.crease_degrees);
    let indices = meshopt::optimize_vertex_cache(&mesh.indices, mesh.positions.len());
    LinkMesh { indices, ..mesh }
}

fn weld(corners: &[[f32; 3]]) -> (Vec<[f32; 3]>, Vec<u32>) {
    let (unique, remap) = meshopt::generate_vertex_remap(corners, None);
    let positions = meshopt::remap_vertex_buffer(corners, unique, &remap);
    let indices = meshopt::remap_index_buffer(None, corners.len(), &remap);
    (positions, indices)
}

fn drop_degenerate(indices: Vec<u32>) -> Vec<u32> {
    indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
        .flatten()
        .copied()
        .collect()
}

fn simplify(positions: &[[f32; 3]], indices: &[u32], detail: Detail) -> Vec<u32> {
    if indices.is_empty() {
        return Vec::new();
    }
    let adapter = meshopt::VertexDataAdapter::new(
        meshopt::typed_to_bytes(positions),
        std::mem::size_of::<[f32; 3]>(),
        0,
    )
    .expect("a [f32; 3] slice is a whole number of 12-byte vertices");
    let floor = (indices.len() as f32 * detail.min_triangle_ratio) as usize / 3 * 3;
    let simplified = meshopt::simplify(
        indices,
        &adapter,
        floor.max(3),
        detail.max_error_m,
        meshopt::SimplifyOptions::ErrorAbsolute,
        None,
    );
    drop_degenerate(simplified)
}

/// Splits every vertex by the faces around it: a corner's normal is the
/// area-weighted sum of the faces within `crease_degrees` of its own face.
fn crease_normals(positions: &[[f32; 3]], indices: &[u32], crease_degrees: f32) -> LinkMesh {
    let point = |i: u32| Vec3::from_array(positions[i as usize]);
    let face_area_normals: Vec<Vec3> = indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| (point(t[1]) - point(t[0])).cross(point(t[2]) - point(t[0])))
        .collect();
    let face_units: Vec<Vec3> = face_area_normals
        .iter()
        .map(|n| n.normalize_or_zero())
        .collect();

    let mut faces_of = vec![Vec::new(); positions.len()];
    for (face, t) in indices.as_chunks::<3>().0.iter().enumerate() {
        for &corner in t {
            faces_of[corner as usize].push(face as u32);
        }
    }

    let cos_crease = crease_degrees.to_radians().cos();
    let mut out = LinkMesh::default();
    let mut emitted: std::collections::HashMap<(u32, [u32; 3]), u32> = Default::default();
    for (face, t) in indices.as_chunks::<3>().0.iter().enumerate() {
        let own = face_units[face];
        for &corner in t {
            let mut sum = Vec3::ZERO;
            for &other in &faces_of[corner as usize] {
                if face_units[other as usize].dot(own) >= cos_crease {
                    sum += face_area_normals[other as usize];
                }
            }
            let normal = sum.try_normalize().unwrap_or(own);
            let key = (corner, normal.to_array().map(f32::to_bits));
            let index = *emitted.entry(key).or_insert_with(|| {
                out.positions.push(point(corner));
                out.normals.push(normal);
                (out.positions.len() - 1) as u32
            });
            out.indices.push(index);
        }
    }
    out
}
