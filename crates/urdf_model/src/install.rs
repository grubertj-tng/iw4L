//! Puts the imported robot into one match: two stand-in materials, one
//! catalog XModel and one script model placed in front of a spawn point.

use std::sync::Arc;

use asset_core::{AssetNamespace, WalkLocalMaterialIndex};
use asset_material::{
    MaterialCatalog, StandInTextures, TS_COLOR_MAP, TS_NORMAL_MAP, TS_SPECULAR_MAP,
};
use asset_world::{
    ClipCollision, MapXModelAssetKey, MapXModelSceneAsset, MapXModelSceneCatalog, ScriptModelId,
    ScriptModelMetadata, ScriptModelSceneInstance, SpawnPoint,
};
use bevy::math::{Quat, Vec3};
use bevy::prelude::Transform;

use crate::Placement;
use crate::skel::RobotModel;

/// Map entity ordinals count up from zero and GSC-spawned movers start at
/// 0x4000_0000; the robot takes an ordinal neither will reach.
const ROBOT_ORDINAL: u32 = 0x3f00_0000;

/// The IW4 lit model technique that samples colour, normal and specular maps
/// and receives sun shadows. Stand-ins borrow its state and shaders.
const DONOR_TECHNIQUE_SET: &str = "mc_l_sm_r0c0n0s0";

/// common_mp materials on that technique, so every multiplayer map lends the
/// robot the same shaders. Any other material on it is the fallback.
const PREFERRED_DONORS: [&str; 2] = ["mc/mtl_weapon_claymore", "mc/mtl_weapon_c4"];

/// The donor's per-material constants are replaced so the robot looks the same
/// whichever donor lent the shaders: the most common environment-map response
/// on that technique (a dull plastic sheen) and no tint.
const STAND_IN_CONSTANTS: [(&str, [f32; 4]); 2] = [
    ("envMapParms", [0.07, 0.33, 1.4, 2.0]),
    ("colorTint", [1.0, 1.0, 1.0, 1.0]),
];

const CONTENTS_SOLID: u32 = 0x1;

/// Where the robot stands relative to the spawn it is placed in front of.
const SPAWN_CLEARANCE: f32 = 96.0;
const EYE_HEIGHT: f32 = 48.0;
const FOOTPRINT_HALF: f32 = 12.0;

/// What one match receives.
pub struct MatchSlots<'a> {
    pub materials: &'a mut MaterialCatalog,
    pub scene_assets: &'a mut MapXModelSceneCatalog,
    pub script_models: &'a mut Vec<ScriptModelSceneInstance>,
    pub spawns: &'a [SpawnPoint],
    pub clip: Option<&'a ClipCollision>,
}

pub fn install(
    model: &RobotModel,
    placement: Placement,
    slots: MatchSlots<'_>,
) -> Result<String, String> {
    let key = MapXModelAssetKey(model.name.clone());
    if slots.scene_assets.get(&key).is_some() {
        return Err(format!(
            "the map already has an XModel named {}",
            model.name
        ));
    }
    // Everything that can refuse is settled before the match is touched.
    let (origin, yaw, from) = match placement {
        Placement::At { origin, yaw } => (origin, yaw, "configured".to_owned()),
        Placement::InFrontOfSpawn => {
            let (origin, yaw, spawn) = in_front_of_spawn(slots.spawns, slots.clip)
                .ok_or("the map has no spawn point to stand the robot near")?;
            (origin, yaw, format!("in front of {spawn}"))
        }
    };
    let donor = pick_donor(slots.materials)
        .ok_or_else(|| format!("no lit model material ({DONOR_TECHNIQUE_SET}) to borrow"))?;
    let donor_name = slots.materials.materials[donor].name.as_str().to_owned();

    let flat_normal = Arc::new(asset_material::solid_texture([128, 128, 255, 128], false));
    let specular = Arc::new(asset_material::solid_texture([48, 48, 48, 160], true));
    let mut tone_materials = Vec::new();
    for rgba in model.tones() {
        let bytes = rgba.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
        let hex = format!("{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2]);
        let textures = StandInTextures {
            color: Some((
                format!("$urdf_{hex}"),
                Arc::new(asset_material::solid_texture(
                    [bytes[0], bytes[1], bytes[2], 255],
                    true,
                )),
                true,
            )),
            normal: Some(("$urdf_flat_normal".to_owned(), flat_normal.clone(), false)),
            specular: Some(("$urdf_specular".to_owned(), specular.clone(), true)),
        };
        let name = format!("urdf/{}/{hex}", model.name);
        let index = slots
            .materials
            .stand_in_material(donor, &name, textures)
            .ok_or("the donor material vanished")?;
        for constant in &mut slots.materials.materials[index].constants {
            let name = constant.name.split(|&b| b == 0).next().unwrap_or(&[]);
            if let Some((_, literal)) = STAND_IN_CONSTANTS
                .iter()
                .find(|(wanted, _)| wanted.as_bytes() == name)
            {
                constant.literal = *literal;
            }
        }
        tone_materials.push((rgba, WalkLocalMaterialIndex::from_walk(index)));
    }

    let skel = model.skel(&model.name, |rgba| {
        tone_materials
            .iter()
            .find(|(tone, _)| *tone == rgba)
            .map(|(_, index)| *index)
            .expect("every part colour is a tone")
    });
    let radius = skel.radius.unwrap_or(0.0);
    slots
        .scene_assets
        .insert(key.clone(), MapXModelSceneAsset::Iw4(Arc::new(skel)));
    slots.script_models.push(ScriptModelSceneInstance {
        id: ScriptModelId::from_source_ordinal(ROBOT_ORDINAL),
        dobj_state: xmodel_runtime::DObjSemanticState::bind_pose(model.name.clone(), 1, 1),
        current_model: key,
        transform: Transform {
            translation: origin,
            rotation: Quat::from_rotation_z(yaw.to_radians()),
            scale: Vec3::ONE,
        },
        // The model origin is on the floor; sample light from inside the body.
        lighting_origin: (origin + Vec3::Z * radius.min(EYE_HEIGHT)).to_array(),
        metadata: ScriptModelMetadata {
            targetname: model.name.clone(),
            ..Default::default()
        },
    });

    Ok(format!(
        "urdf robot {}: bones={} surfaces={} triangles={} (from {}) vertices={} donor={donor_name} placed {from} at ({:.0} {:.0} {:.0}) yaw {:.0}",
        model.name,
        model.bones.len(),
        model.parts.len(),
        model.triangle_count(),
        model.source_triangles,
        model.vertex_count(),
        origin.x,
        origin.y,
        origin.z,
        yaw,
    ))
}

/// A real IW4 material on the lit model technique with all three maps bound,
/// a preferred one when the match has it.
fn pick_donor(materials: &MaterialCatalog) -> Option<usize> {
    let candidates: Vec<(usize, &str)> = materials
        .materials
        .iter()
        .enumerate()
        .filter(|(_, material)| {
            material.namespace == AssetNamespace::Iw4
                && material.name.is_real()
                && material.technique_set.as_str() == DONOR_TECHNIQUE_SET
                && [TS_COLOR_MAP, TS_NORMAL_MAP, TS_SPECULAR_MAP]
                    .iter()
                    .all(|&semantic| {
                        material
                            .textures
                            .iter()
                            .any(|binding| binding.semantic == semantic && binding.image.is_some())
                    })
        })
        .map(|(index, material)| (index, material.name.as_str()))
        .collect();
    PREFERRED_DONORS
        .iter()
        .find_map(|preferred| candidates.iter().find(|(_, name)| name == preferred))
        .or_else(|| candidates.iter().min_by_key(|(_, name)| *name))
        .map(|(index, _)| *index)
}

/// The first free-for-all spawn (or any spawn), looking at the robot: a step
/// forward, stopped short of walls, dropped onto the floor.
fn in_front_of_spawn(
    spawns: &[SpawnPoint],
    clip: Option<&ClipCollision>,
) -> Option<(Vec3, f32, String)> {
    let spawn = spawns
        .iter()
        .find(|spawn| spawn.classname == "mp_dm_spawn")
        .or_else(|| spawns.first())?;
    let yaw = spawn.angles[1];
    let forward = Vec3::new(yaw.to_radians().cos(), yaw.to_radians().sin(), 0.0);
    let feet = Vec3::from_array(spawn.origin);
    let label = format!(
        "{} ({:.0} {:.0} {:.0})",
        spawn.classname, feet.x, feet.y, feet.z
    );
    // Turned around to look at the spawn, kept in (-180, 180].
    let facing_spawn = 180.0 - (-yaw).rem_euclid(360.0);
    let Some(clip) = clip else {
        return Some((feet + forward * SPAWN_CLEARANCE, facing_spawn, label));
    };
    let half = [FOOTPRINT_HALF, FOOTPRINT_HALF, 0.0];
    let lo = [-FOOTPRINT_HALF, -FOOTPRINT_HALF, 0.0];
    let eye = feet + Vec3::Z * EYE_HEIGHT;
    let ahead = clip.sweep_box(
        eye.to_array(),
        (eye + forward * SPAWN_CLEARANCE).to_array(),
        lo,
        half,
        CONTENTS_SOLID,
    );
    let stand = Vec3::from_array(ahead.endpos);
    let down = clip.sweep_box(
        stand.to_array(),
        (stand - Vec3::Z * (EYE_HEIGHT + 256.0)).to_array(),
        lo,
        half,
        CONTENTS_SOLID,
    );
    let floor = if down.fraction < 1.0 && !down.startsolid {
        Vec3::from_array(down.endpos)
    } else {
        Vec3::new(stand.x, stand.y, feet.z)
    };
    Some((floor, facing_spawn, label))
}
