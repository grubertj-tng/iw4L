//! Imports a robot from its URDF description (links, joints, STL meshes) and
//! stands it in a match as an IW4 script model.
//!
//! Opt-in through the environment, so a match without it loads exactly as
//! before:
//!
//! * `IW4L_ROBOT_URDF` — path to the `.urdf`; meshes resolve relative to it.
//! * `IW4L_ROBOT_ORIGIN` — optional `x y z` in map units (inches), feet height.
//!   Without it the robot stands in front of the first free-for-all spawn.
//! * `IW4L_ROBOT_YAW` — optional facing in degrees, with `IW4L_ROBOT_ORIGIN`.
//!
//! Every machine in a match must use the same setting: the robot occupies an
//! entity slot on the host and on each client.

mod install;
mod mesh;
mod skel;
mod stl;
mod urdf;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub use install::MatchSlots;
pub use mesh::Detail;
pub use skel::{INCHES_PER_METRE, ROOT_BONE, RobotModel};
pub use urdf::Robot;

pub const URDF_ENV: &str = "IW4L_ROBOT_URDF";
pub const ORIGIN_ENV: &str = "IW4L_ROBOT_ORIGIN";
pub const YAW_ENV: &str = "IW4L_ROBOT_YAW";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    At { origin: bevy::math::Vec3, yaw: f32 },
    InFrontOfSpawn,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub urdf: PathBuf,
    pub placement: Placement,
}

impl Config {
    /// `None` when no robot is configured.
    pub fn from_env() -> Option<Result<Self, String>> {
        let urdf = std::env::var_os(URDF_ENV).filter(|value| !value.is_empty())?;
        Some(Self::parse(
            PathBuf::from(urdf),
            std::env::var(ORIGIN_ENV).ok().as_deref(),
            std::env::var(YAW_ENV).ok().as_deref(),
        ))
    }

    fn parse(urdf: PathBuf, origin: Option<&str>, yaw: Option<&str>) -> Result<Self, String> {
        let origin = origin.map(str::trim).filter(|value| !value.is_empty());
        let yaw = yaw.map(str::trim).filter(|value| !value.is_empty());
        let placement = match (origin, yaw) {
            (None, None) => Placement::InFrontOfSpawn,
            (None, Some(_)) => return Err(format!("{YAW_ENV} needs {ORIGIN_ENV}")),
            (Some(origin), yaw) => {
                let words: Vec<f32> = origin
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| format!("{ORIGIN_ENV}=\"{origin}\" is not three numbers"))?;
                let [x, y, z] = words[..] else {
                    return Err(format!("{ORIGIN_ENV}=\"{origin}\" is not three numbers"));
                };
                let yaw = match yaw {
                    Some(yaw) => yaw
                        .parse()
                        .map_err(|_| format!("{YAW_ENV}=\"{yaw}\" is not a number"))?,
                    None => 0.0,
                };
                Placement::At {
                    origin: bevy::math::Vec3::new(x, y, z),
                    yaw,
                }
            }
        };
        Ok(Self { urdf, placement })
    }
}

/// Loads (once per process and URDF) and installs the configured robot.
/// `None` when no robot is configured; otherwise one report line.
pub fn install_from_env(slots: MatchSlots<'_>) -> Option<Result<String, String>> {
    let config = match Config::from_env()? {
        Ok(config) => config,
        Err(error) => return Some(Err(error)),
    };
    Some(
        load_cached(&config.urdf)
            .and_then(|model| install::install(&model, config.placement, slots)),
    )
}

/// The geometry costs a second or so to rebuild; matches on the same URDF
/// share it.
fn load_cached(urdf: &Path) -> Result<Arc<RobotModel>, String> {
    static CACHE: Mutex<Option<(PathBuf, Arc<RobotModel>)>> = Mutex::new(None);
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((path, model)) = cache.as_ref()
        && path == urdf
    {
        return Ok(model.clone());
    }
    let model = Arc::new(RobotModel::build(&Robot::load(urdf)?, Detail::default())?);
    *cache = Some((urdf.to_owned(), model.clone()));
    Ok(model)
}
