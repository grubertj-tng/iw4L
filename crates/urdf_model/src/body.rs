//! The robot as a soldier body: its links fitted onto the bind pose of an IW4
//! player body skeleton, each rigid on the bone that animates it.
//!
//! The skeleton, its hit boxes and its tags stay the soldier's, so player
//! animations, aiming, head and weapon attachment work unchanged. Only the
//! geometry is the robot's. Each limb segment is moved so its joints land on
//! the soldier's joints: uniformly scaled to soldier size across, stretched
//! along its length to the soldier's segment, and turned onto its direction.

use asset_core::WalkLocalMaterialIndex;
use asset_model::ModelSkel;
use bevy::math::{Quat, Vec3};

use crate::skel::{Geometry, RobotModel};

/// Which robot links ride which soldier segment. The bone names on the soldier
/// side are the IW4 multiplayer body rig.
pub struct HumanoidRig {
    pub pelvis: &'static [&'static str],
    pub waist: &'static [&'static str],
    pub chest: &'static [&'static str],
    pub head: &'static [&'static str],
    /// The link whose frame sits where the waist bends; the torso starts here.
    pub waist_joint: &'static str,
    pub legs: [LegRig; 2],
    pub arms: [ArmRig; 2],
}

/// Landmarks are link frames: the joint the link turns about.
pub struct LegRig {
    pub hip: &'static str,
    pub knee: &'static str,
    pub ankle: &'static str,
    pub thigh: &'static [&'static str],
    pub shin: &'static [&'static str],
    pub foot: &'static [&'static str],
}

pub struct ArmRig {
    pub shoulder: &'static str,
    pub elbow: &'static str,
    pub wrist: &'static str,
    /// A frame inside the hand, so the hand can be turned to point like the
    /// soldier's.
    pub hand_tip: &'static str,
    pub upper: &'static [&'static str],
    pub fore: &'static [&'static str],
    pub hand: &'static [&'static str],
}

/// Unitree G1 (29 DoF, rev 1.0 and its siblings), left side first.
pub const UNITREE_G1: HumanoidRig = HumanoidRig {
    pelvis: &["pelvis", "pelvis_contour_link"],
    waist: &["waist_yaw_link", "waist_roll_link"],
    chest: &["torso_link", "logo_link"],
    head: &["head_link"],
    waist_joint: "waist_yaw_link",
    legs: [
        LegRig {
            hip: "left_hip_roll_link",
            knee: "left_knee_link",
            ankle: "left_ankle_pitch_link",
            thigh: &[
                "left_hip_pitch_link",
                "left_hip_roll_link",
                "left_hip_yaw_link",
            ],
            shin: &["left_knee_link"],
            foot: &["left_ankle_pitch_link", "left_ankle_roll_link"],
        },
        LegRig {
            hip: "right_hip_roll_link",
            knee: "right_knee_link",
            ankle: "right_ankle_pitch_link",
            thigh: &[
                "right_hip_pitch_link",
                "right_hip_roll_link",
                "right_hip_yaw_link",
            ],
            shin: &["right_knee_link"],
            foot: &["right_ankle_pitch_link", "right_ankle_roll_link"],
        },
    ],
    arms: [
        ArmRig {
            shoulder: "left_shoulder_roll_link",
            elbow: "left_elbow_link",
            wrist: "left_wrist_pitch_link",
            hand_tip: "left_rubber_hand",
            upper: &[
                "left_shoulder_pitch_link",
                "left_shoulder_roll_link",
                "left_shoulder_yaw_link",
            ],
            fore: &["left_elbow_link", "left_wrist_roll_link"],
            hand: &[
                "left_wrist_pitch_link",
                "left_wrist_yaw_link",
                "left_rubber_hand",
            ],
        },
        ArmRig {
            shoulder: "right_shoulder_roll_link",
            elbow: "right_elbow_link",
            wrist: "right_wrist_pitch_link",
            hand_tip: "right_rubber_hand",
            upper: &[
                "right_shoulder_pitch_link",
                "right_shoulder_roll_link",
                "right_shoulder_yaw_link",
            ],
            fore: &["right_elbow_link", "right_wrist_roll_link"],
            hand: &[
                "right_wrist_pitch_link",
                "right_wrist_yaw_link",
                "right_rubber_hand",
            ],
        },
    ],
};

/// The rigs this import knows, tried in order against the robot's links.
pub const RIGS: &[(&str, &HumanoidRig)] = &[("Unitree G1", &UNITREE_G1)];

pub fn rig_for(model: &RobotModel) -> Option<(&'static str, &'static HumanoidRig)> {
    RIGS.iter()
        .copied()
        .find(|(_, rig)| rig.links().all(|link| model.bone(link).is_some()))
}

impl HumanoidRig {
    fn links(&self) -> impl Iterator<Item = &'static str> + '_ {
        let legs = self
            .legs
            .iter()
            .flat_map(|leg| [leg.thigh, leg.shin, leg.foot].into_iter().flatten());
        let arms = self
            .arms
            .iter()
            .flat_map(|arm| [arm.upper, arm.fore, arm.hand].into_iter().flatten());
        [self.pelvis, self.waist, self.chest, self.head]
            .into_iter()
            .flatten()
            .chain(legs)
            .chain(arms)
            .copied()
            .chain([self.waist_joint])
    }
}

/// Soldier bones per side, left then right.
const HIP: [&str; 2] = ["j_hip_le", "j_hip_ri"];
const KNEE: [&str; 2] = ["j_knee_le", "j_knee_ri"];
const ANKLE: [&str; 2] = ["j_ankle_le", "j_ankle_ri"];
const SHOULDER: [&str; 2] = ["j_shoulder_le", "j_shoulder_ri"];
const ELBOW: [&str; 2] = ["j_elbow_le", "j_elbow_ri"];
const WRIST: [&str; 2] = ["j_wrist_le", "j_wrist_ri"];
const KNUCKLE: [&str; 2] = ["j_mid_le_1", "j_mid_ri_1"];

/// A similarity that is allowed one extra stretch along `axis`: positions
/// relative to `from` are scaled by `axial` along the axis and `across`
/// perpendicular to it, turned by `rotation` and moved to `to`.
#[derive(Clone, Copy, Debug)]
struct Fit {
    from: Vec3,
    axis: Vec3,
    rotation: Quat,
    to: Vec3,
    axial: f32,
    across: f32,
}

impl Fit {
    fn uniform(from: Vec3, to: Vec3, scale: f32) -> Self {
        Self {
            from,
            axis: Vec3::Z,
            rotation: Quat::IDENTITY,
            to,
            axial: scale,
            across: scale,
        }
    }

    /// Maps the segment `from..from_end` onto `to..to_end`.
    fn segment(from: Vec3, from_end: Vec3, to: Vec3, to_end: Vec3, across: f32) -> Self {
        let source = from_end - from;
        let target = to_end - to;
        let axis = source.normalize();
        Self {
            from,
            axis,
            rotation: Quat::from_rotation_arc(axis, target.normalize()),
            to,
            axial: target.length() / source.length(),
            across,
        }
    }

    /// Turns the segment onto the target's direction without stretching it.
    fn pointing(from: Vec3, from_end: Vec3, to: Vec3, to_end: Vec3, scale: f32) -> Self {
        Self {
            axial: scale,
            ..Self::segment(from, from_end, to, to_end, scale)
        }
    }

    fn point(&self, p: Vec3) -> Vec3 {
        let relative = p - self.from;
        let along = self.axis * relative.dot(self.axis);
        self.to + self.rotation * (along * self.axial + (relative - along) * self.across)
    }

    /// Normals take the inverse transpose of the stretch.
    fn normal(&self, n: Vec3) -> Vec3 {
        let along = self.axis * n.dot(self.axis);
        (self.rotation * (along / self.axial + (n - along) / self.across)).normalize_or(n)
    }
}

/// The robot fitted to `template`'s skeleton. Everything but the geometry is
/// the template's.
pub fn fit_body(
    model: &RobotModel,
    rig: &HumanoidRig,
    template: &ModelSkel,
    name: &str,
    material_of: impl Fn([f32; 4]) -> WalkLocalMaterialIndex,
) -> Result<ModelSkel, String> {
    let link = |name: &str| -> Result<Vec3, String> {
        model
            .bone(name)
            .map(|bone| bone.translation.as_vec3())
            .ok_or_else(|| format!("robot has no link {name}"))
    };
    let soldier = |bone: &str| -> Result<(usize, Vec3), String> {
        template
            .bone_names
            .iter()
            .position(|name| name == bone)
            .map(|index| (index, Vec3::from_array(template.bones[index].trans)))
            .ok_or_else(|| format!("body {} has no bone {bone}", template.name))
    };
    let mid = |a: Vec3, b: Vec3| (a + b) * 0.5;

    // Soldier size across: the robot's shoulders brought to the soldier's.
    let robot_shoulders = mid(link(rig.arms[0].shoulder)?, link(rig.arms[1].shoulder)?);
    let soldier_shoulders = mid(soldier(SHOULDER[0])?.1, soldier(SHOULDER[1])?.1);
    let scale = soldier_shoulders.z / robot_shoulders.z;

    let mut fits: Vec<(&[&str], usize, Fit)> = Vec::new();
    // Pelvis and torso meet exactly at the waist; the thighs, hung from the
    // soldier's hips, tuck a little into the pelvis instead.
    let waist = link(rig.waist_joint)?;
    let soldier_waist = soldier("j_spinelower")?.1;
    fits.push((
        rig.pelvis,
        soldier("pelvis")?.0,
        Fit::uniform(waist, soldier_waist, scale),
    ));
    let torso = Fit::segment(
        waist,
        robot_shoulders,
        soldier_waist,
        soldier_shoulders,
        scale,
    );
    fits.push((rig.waist, soldier("j_spinelower")?.0, torso));
    fits.push((rig.chest, soldier("j_spine4")?.0, torso));
    // The robot's head is fixed to its torso; it keeps the torso's fit and
    // turns with the soldier's head.
    fits.push((rig.head, soldier("j_head")?.0, torso));

    for (side, leg) in rig.legs.iter().enumerate() {
        let (hip_bone, hip) = soldier(HIP[side])?;
        let (knee_bone, knee) = soldier(KNEE[side])?;
        let (ankle_bone, ankle) = soldier(ANKLE[side])?;
        let robot_ankle = link(leg.ankle)?;
        // The foot keeps the robot's shape and stands on the floor, so the
        // robot's ankle sits where its own height puts it, under the soldier's.
        let foot_ankle = Vec3::new(ankle.x, ankle.y, robot_ankle.z * scale);
        fits.push((
            leg.thigh,
            hip_bone,
            Fit::segment(link(leg.hip)?, link(leg.knee)?, hip, knee, scale),
        ));
        fits.push((
            leg.shin,
            knee_bone,
            Fit::segment(link(leg.knee)?, robot_ankle, knee, foot_ankle, scale),
        ));
        fits.push((
            leg.foot,
            ankle_bone,
            Fit::uniform(robot_ankle, foot_ankle, scale),
        ));
    }

    for (side, arm) in rig.arms.iter().enumerate() {
        let (shoulder_bone, shoulder) = soldier(SHOULDER[side])?;
        let (elbow_bone, elbow) = soldier(ELBOW[side])?;
        let (wrist_bone, wrist) = soldier(WRIST[side])?;
        let knuckle = soldier(KNUCKLE[side])?.1;
        let robot_wrist = link(arm.wrist)?;
        fits.push((
            arm.upper,
            shoulder_bone,
            Fit::segment(
                link(arm.shoulder)?,
                link(arm.elbow)?,
                shoulder,
                elbow,
                scale,
            ),
        ));
        fits.push((
            arm.fore,
            elbow_bone,
            Fit::segment(link(arm.elbow)?, robot_wrist, elbow, wrist, scale),
        ));
        fits.push((
            arm.hand,
            wrist_bone,
            Fit::pointing(robot_wrist, link(arm.hand_tip)?, wrist, knuckle, scale),
        ));
    }

    let mut geometry = Geometry::default();
    let mut fitted = 0;
    for (links, bone, fit) in &fits {
        for &link_name in *links {
            let part = model
                .part(link_name)
                .ok_or_else(|| format!("robot link {link_name} has no mesh"))?;
            let pose = &model.bones[part.bone];
            let rotation = bevy::math::DQuat::from_mat3(&pose.rotation).as_quat();
            let translation = pose.translation.as_vec3();
            geometry.push_mesh(
                &part.mesh,
                *bone,
                material_of(part.rgba),
                |p| fit.point(rotation * p + translation),
                |n| fit.normal(rotation * n),
            );
            fitted += 1;
        }
    }
    if fitted != model.parts.len() {
        return Err(format!(
            "the rig places {fitted} of the robot's {} links",
            model.parts.len()
        ));
    }

    let (radius, bounds) = geometry.radius_and_bounds();
    let mut skel = template.clone();
    skel.name = name.to_owned();
    if let Some(pose) = skel.pose.as_mut() {
        pose.name = name.to_owned();
    }
    skel.rigid_verts = geometry.vert_skin.len();
    skel.blend_verts = 0;
    skel.positions = geometry.positions;
    skel.normals = geometry.normals;
    skel.colors = geometry.colors;
    skel.uvs = geometry.uvs;
    skel.indices = geometry.indices;
    skel.packed_vertices = geometry.packed_vertices;
    skel.vert_skin = geometry.vert_skin;
    skel.surface_materials = geometry.surface_materials;
    skel.surface_vertex_ranges = geometry.surface_vertex_ranges;
    skel.surface_index_ranges = geometry.surface_index_ranges;
    skel.surface_part_bits = geometry.surface_part_bits;
    skel.surface_deformed = geometry.surface_deformed;
    skel.surface_vert_list_count = geometry.surface_vert_list_count;
    skel.radius = Some(radius.max(template.radius.unwrap_or(0.0)));
    skel.bounds = Some(bounds);
    // One level of detail: the template's spans count its own surfaces.
    skel.lod = None;
    skel.lod_smc = None;
    skel.lod_part_bits = None;
    skel.lod_surf_span = [(0, 0); 4];
    Ok(skel)
}
