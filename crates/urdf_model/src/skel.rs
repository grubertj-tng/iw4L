//! A URDF robot as an IW4 XModel skeleton: one rigid bone per link, posed with
//! every joint at zero, feet on the model origin, in inches.

use asset_core::WalkLocalMaterialIndex;
use asset_model::{BoneBind, ModelSkel, VertSkin};
use bevy::math::{DMat3, DQuat, DVec3, Quat, Vec3};
use xmodel_runtime::{BoneCollision, ModelPoseSrc};

use crate::mesh::{self, Detail, LinkMesh};
use crate::urdf::{Pose, Robot};

pub const INCHES_PER_METRE: f64 = 1.0 / 0.0254;

/// The root every IW4 model hangs its skeleton from.
pub const ROOT_BONE: &str = "tag_origin";

/// The engine reads at most 192 part bits per model.
const MAX_BONES: usize = 192;

#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    /// Bind pose in model space, inches.
    pub rotation: DMat3,
    pub translation: DVec3,
}

/// One link's geometry, in its bone's frame, inches.
#[derive(Clone, Debug)]
pub struct Part {
    pub bone: usize,
    pub rgba: [f32; 4],
    pub mesh: LinkMesh,
}

/// The robot's geometry and skeleton, independent of any match's materials.
#[derive(Clone, Debug)]
pub struct RobotModel {
    pub name: String,
    pub bones: Vec<Bone>,
    pub parts: Vec<Part>,
    pub source_triangles: usize,
}

impl RobotModel {
    pub fn build(robot: &Robot, detail: Detail) -> Result<Self, String> {
        let link_pose = robot.zero_pose()?;
        let root = robot.root_link()?;

        let mut source_triangles = 0;
        let mut link_meshes = Vec::with_capacity(robot.links.len());
        for link in &robot.links {
            let mut soups = Vec::with_capacity(link.visuals.len());
            for visual in &link.visuals {
                let triangles = crate::stl::load(&visual.mesh)?;
                source_triangles += triangles.len();
                soups.push((visual, triangles));
            }
            let mesh = (!soups.is_empty()).then(|| mesh::build(&soups, detail));
            let rgba = link.visuals.first().map_or([1.0; 4], |visual| visual.rgba);
            link_meshes.push(
                mesh.filter(|mesh| mesh.triangle_count() > 0)
                    .map(|m| (m, rgba)),
            );
        }

        // Keep the links that carry geometry and every link on their way to
        // the root; sensors and frames with nothing below them add no bone.
        let mut keep = vec![false; robot.links.len()];
        let parent_of = |link: usize| {
            robot
                .joints
                .iter()
                .find(|joint| joint.child == link)
                .map(|joint| joint.parent)
        };
        for link in (0..robot.links.len()).filter(|&link| link_meshes[link].is_some()) {
            let mut at = Some(link);
            while let Some(link) = at.filter(|&link| !keep[link]) {
                keep[link] = true;
                at = parent_of(link);
            }
        }
        if !keep[root] {
            return Err(format!("robot {} has no mesh visuals", robot.name));
        }

        // Feet on the floor: the lowest point of the posed geometry is z = 0.
        let lowest = link_meshes
            .iter()
            .enumerate()
            .filter_map(|(link, mesh)| Some((link, &mesh.as_ref()?.0)))
            .flat_map(|(link, mesh)| {
                let pose = link_pose[link];
                mesh.positions
                    .iter()
                    .map(move |p| pose.apply(p.as_dvec3()).z)
            })
            .fold(f64::INFINITY, f64::min);
        let to_model = |pose: Pose| Pose {
            rotation: pose.rotation,
            translation: (pose.translation - DVec3::Z * lowest) * INCHES_PER_METRE,
        };

        let mut bones = vec![Bone {
            name: ROOT_BONE.to_owned(),
            parent: None,
            rotation: DMat3::IDENTITY,
            translation: DVec3::ZERO,
        }];
        let mut bone_of = vec![None; robot.links.len()];
        let mut parts = Vec::new();
        // Breadth-first from the root so every parent precedes its children.
        let mut queue = std::collections::VecDeque::from([(root, 0usize)]);
        while let Some((link, parent_bone)) = queue.pop_front() {
            let pose = to_model(link_pose[link]);
            let bone = bones.len();
            bones.push(Bone {
                name: robot.links[link].name.clone(),
                parent: Some(parent_bone),
                rotation: pose.rotation,
                translation: pose.translation,
            });
            bone_of[link] = Some(bone);
            if let Some((mesh, rgba)) = link_meshes[link].take() {
                let mesh = LinkMesh {
                    positions: mesh
                        .positions
                        .iter()
                        .map(|p| *p * INCHES_PER_METRE as f32)
                        .collect(),
                    ..mesh
                };
                parts.push(Part { bone, rgba, mesh });
            }
            for joint in robot.joints.iter().filter(|joint| joint.parent == link) {
                if keep[joint.child] {
                    queue.push_back((joint.child, bone));
                }
            }
        }
        if bones.len() > MAX_BONES {
            return Err(format!(
                "robot {} needs {} bones; IW4 models have at most {MAX_BONES}",
                robot.name,
                bones.len()
            ));
        }
        Ok(Self {
            name: robot.name.clone(),
            bones,
            parts,
            source_triangles,
        })
    }

    pub fn triangle_count(&self) -> usize {
        self.parts
            .iter()
            .map(|part| part.mesh.triangle_count())
            .sum()
    }

    pub fn vertex_count(&self) -> usize {
        self.parts
            .iter()
            .map(|part| part.mesh.positions.len())
            .sum()
    }

    /// Distinct part colours, in first-use order.
    pub fn tones(&self) -> Vec<[f32; 4]> {
        let mut tones: Vec<[f32; 4]> = Vec::new();
        for part in &self.parts {
            if !tones.contains(&part.rgba) {
                tones.push(part.rgba);
            }
        }
        tones
    }

    /// The model as an XModel skeleton. `material_of` names the walk-local
    /// material for each tone returned by [`Self::tones`].
    pub fn skel(
        &self,
        name: &str,
        material_of: impl Fn([f32; 4]) -> WalkLocalMaterialIndex,
    ) -> ModelSkel {
        let bind: Vec<(Quat, Vec3)> = self
            .bones
            .iter()
            .map(|bone| {
                (
                    DQuat::from_mat3(&bone.rotation).normalize().as_quat(),
                    bone.translation.as_vec3(),
                )
            })
            .collect();

        let mut geometry = Geometry::default();
        for part in &self.parts {
            let (rotation, translation) = bind[part.bone];
            geometry.push_part(part, rotation, translation, material_of(part.rgba));
        }

        let mut bone_collision = vec![None; self.bones.len()];
        for part in &self.parts {
            bone_collision[part.bone] = collision_box(&part.mesh);
        }

        let radius = geometry
            .positions
            .iter()
            .map(|p| Vec3::from_array(*p).length())
            .fold(0.0f32, f32::max);
        let bounds =
            geometry
                .positions
                .iter()
                .fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
                    (
                        std::array::from_fn(|i| lo[i].min(p[i])),
                        std::array::from_fn(|i| hi[i].max(p[i])),
                    )
                });
        let rigid_verts = geometry.vert_skin.len();

        ModelSkel {
            name: name.to_owned(),
            bones: bind
                .iter()
                .map(|(rotation, translation)| BoneBind {
                    quat: rotation.to_array(),
                    trans: translation.to_array(),
                })
                .collect(),
            bone_collision,
            bone_names: self.bones.iter().map(|bone| bone.name.clone()).collect(),
            tag_view: None,
            tag_weapon: None,
            pose: Some(self.pose_src(name, &bind)),
            positions: geometry.positions,
            normals: geometry.normals,
            colors: geometry.colors,
            uvs: geometry.uvs,
            indices: geometry.indices,
            surface_materials: geometry.surface_materials,
            surface_vertex_ranges: geometry.surface_vertex_ranges,
            surface_index_ranges: geometry.surface_index_ranges,
            surface_part_bits: geometry.surface_part_bits,
            surface_deformed: geometry.surface_deformed,
            surface_vert_list_count: geometry.surface_vert_list_count,
            vert_skin: geometry.vert_skin,
            rigid_verts,
            blend_verts: 0,
            packed_vertices: geometry.packed_vertices,
            radius: Some(radius),
            bounds: Some(bounds),
            // No collision triangles: bullets test the per-bone boxes.
            contents: None,
            coll_lod: -1,
            coll_surfs: Vec::new(),
            movement_brushes: Vec::new(),
            mount_tag: None,
            lod: None,
            lod_smc: None,
            lod_part_bits: None,
            lod_surf_span: [(0, 0); 4],
        }
    }

    fn pose_src(&self, name: &str, bind: &[(Quat, Vec3)]) -> ModelPoseSrc {
        let children = &self.bones[1..];
        let mut parent_list = Vec::with_capacity(children.len());
        let mut quats = Vec::with_capacity(children.len());
        let mut trans = Vec::with_capacity(children.len());
        for (child, bone) in children.iter().enumerate() {
            let index = child + 1;
            let parent = bone.parent.expect("only the root bone has no parent");
            parent_list.push(
                u8::try_from(index - parent).expect("bone count is at most 192, so steps fit u8"),
            );
            let parent_bone = &self.bones[parent];
            let local_rotation = parent_bone.rotation.transpose() * bone.rotation;
            let local_translation =
                parent_bone.rotation.transpose() * (bone.translation - parent_bone.translation);
            quats.push(quat16(DQuat::from_mat3(&local_rotation).normalize()));
            trans.push(local_translation.as_vec3().to_array());
        }
        ModelPoseSrc {
            name: name.to_owned(),
            num_bones: self.bones.len(),
            num_root_bones: 1,
            scale: 1.0,
            no_scale_part_bits: [0; 6],
            bone_names: self.bones.iter().map(|bone| bone.name.clone()).collect(),
            parent_list,
            quats,
            trans,
            base_mat: bind.to_vec(),
            root_rest: None,
        }
    }
}

/// IW4 stores bind rotations as signed 16-bit quaternion components.
fn quat16(q: DQuat) -> [i16; 4] {
    q.to_array()
        .map(|c| (c * 32767.0).round().clamp(-32767.0, 32767.0) as i16)
}

/// The surfaces an XModel draws, accumulated part by part.
#[derive(Default)]
struct Geometry {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    packed_vertices: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    vert_skin: Vec<VertSkin>,
    surface_materials: Vec<Option<WalkLocalMaterialIndex>>,
    surface_vertex_ranges: Vec<(usize, usize)>,
    surface_index_ranges: Vec<(usize, usize)>,
    surface_part_bits: Vec<[u32; 6]>,
    surface_deformed: Vec<Option<bool>>,
    surface_vert_list_count: Vec<Option<u32>>,
}

/// An XSurface addresses its vertices with 16-bit indices.
const MAX_SURFACE_VERTICES: usize = u16::MAX as usize + 1;

/// Solid-colour textures read the same texel everywhere.
const TEXCOORD: [f32; 2] = [0.5, 0.5];

impl Geometry {
    fn push_part(
        &mut self,
        part: &Part,
        rotation: Quat,
        translation: Vec3,
        material: WalkLocalMaterialIndex,
    ) {
        let mut part_bits = [0u32; 6];
        part_bits[part.bone / 32] |= 1 << (part.bone % 32);
        let mesh = &part.mesh;
        // A link too dense for one XSurface becomes several.
        let mut surface_vertex: Vec<Option<u32>> = vec![None; mesh.positions.len()];
        let mut surface: Option<(usize, usize)> = None;
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let fresh = triangle
                .iter()
                .filter(|&&corner| surface_vertex[corner as usize].is_none())
                .count();
            if surface
                .is_none_or(|(base, _)| self.positions.len() - base + fresh > MAX_SURFACE_VERTICES)
            {
                self.close_surface(surface.take());
                surface_vertex.fill(None);
                surface = Some((self.positions.len(), self.indices.len()));
                self.surface_materials.push(Some(material));
                self.surface_part_bits.push(part_bits);
                // Rigid: the whole surface is one vertex list on one bone.
                self.surface_deformed.push(Some(false));
                self.surface_vert_list_count.push(Some(1));
            }
            // IW4 draws clockwise triangles as front faces; STL is counter-clockwise.
            for &corner in [triangle[0], triangle[2], triangle[1]].iter() {
                let index = match surface_vertex[corner as usize] {
                    Some(index) => index,
                    None => {
                        let index = self.positions.len() as u32;
                        self.push_vertex(
                            rotation * mesh.positions[corner as usize] + translation,
                            rotation * mesh.normals[corner as usize],
                            part.bone,
                        );
                        surface_vertex[corner as usize] = Some(index);
                        index
                    }
                };
                self.indices.push(index);
            }
        }
        self.close_surface(surface);
    }

    fn close_surface(&mut self, surface: Option<(usize, usize)>) {
        if let Some((vertex_base, index_base)) = surface {
            self.surface_vertex_ranges
                .push((vertex_base, self.positions.len() - vertex_base));
            self.surface_index_ranges
                .push((index_base, self.indices.len() - index_base));
        }
    }

    fn push_vertex(&mut self, position: Vec3, normal: Vec3, bone: usize) {
        let normal = normal.normalize_or(Vec3::Z);
        let tangent = normal.any_orthonormal_vector();
        let packed = fx_iw4::pack_code_mesh_vertex_signed(
            position.to_array(),
            [255; 4],
            fx_iw4::trail_pack_texcoord(TEXCOORD[0], TEXCOORD[1]),
            asset_model::pack_unit_vec(normal.to_array()),
            asset_model::pack_unit_vec(tangent.to_array()),
            -1.0,
        );
        // The CPU copy is decoded from the packed row so it matches what the
        // GPU reads bit for bit.
        let word = |o: usize| u32::from_le_bytes(packed[o..o + 4].try_into().unwrap());
        self.positions.push(position.to_array());
        self.normals
            .push(asset_model::normalize_or_up(asset_model::unpack_unit_vec(
                word(24),
            )));
        self.colors.push(asset_model::unpack_color(word(16)));
        self.uvs
            .push(asset_model::unpack_packed_tex_coords(word(20)));
        self.packed_vertices.push(packed);
        self.vert_skin.push(VertSkin {
            bones: [bone as u16, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
            weight_u16: [0; 4],
        });
    }
}

/// The bone-space box bullets test against this link.
fn collision_box(mesh: &LinkMesh) -> Option<BoneCollision> {
    let (lo, hi) = mesh
        .positions
        .iter()
        .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &p| {
            (lo.min(p), hi.max(p))
        });
    let half = (hi - lo) * 0.5;
    (half.min_element() >= 0.0 && half.max_element() > 0.0).then(|| BoneCollision {
        midpoint: ((lo + hi) * 0.5).to_array(),
        half_size: half.to_array(),
        radius_sq: half.length_squared(),
        part_classification: 0,
    })
}
