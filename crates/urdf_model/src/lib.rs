//! Imports a robot from its URDF description (links, joints, STL meshes) and
//! puts it in a match: standing as an IW4 script model, and/or worn as the
//! soldier body of one or both teams.
//!
//! Opt-in through the environment, so a match without it loads exactly as
//! before:
//!
//! * `IW4L_ROBOT_URDF` — path to the `.urdf`; meshes resolve relative to it.
//! * `IW4L_ROBOT_ORIGIN` — optional `x y z` in map units (inches), feet height.
//!   Without it the robot stands in front of the first free-for-all spawn.
//! * `IW4L_ROBOT_YAW` — optional facing in degrees, with `IW4L_ROBOT_ORIGIN`.
//! * `IW4L_ROBOT_PROP` — `off` leaves the standing robot out.
//! * `IW4L_ROBOT_BODY` — `all`, `allies` or `axis`: those soldiers are robots.
//!
//! Every machine in a match must use the same setting: the standing robot
//! occupies an entity slot, and the body decides hit boxes, on the host and on
//! each client.

mod body;
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
pub const PROP_ENV: &str = "IW4L_ROBOT_PROP";
pub const BODY_ENV: &str = "IW4L_ROBOT_BODY";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    At { origin: bevy::math::Vec3, yaw: f32 },
    InFrontOfSpawn,
}

/// Which teams wear the robot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sides {
    All,
    Allies,
    Axis,
}

impl Sides {
    pub fn allies(self) -> bool {
        matches!(self, Self::All | Self::Allies)
    }

    pub fn axis(self) -> bool {
        matches!(self, Self::All | Self::Axis)
    }
}

impl std::fmt::Display for Sides {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "allies and axis",
            Self::Allies => "allies",
            Self::Axis => "axis",
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub urdf: PathBuf,
    /// Where the standing robot goes; `None` leaves it out.
    pub prop: Option<Placement>,
    /// Who wears the robot as a body; `None` keeps every soldier.
    pub body: Option<Sides>,
}

impl Config {
    /// `None` when no robot is configured.
    pub fn from_env() -> Option<Result<Self, String>> {
        let urdf = std::env::var_os(URDF_ENV).filter(|value| !value.is_empty())?;
        let var = |name| std::env::var(name).ok();
        Some(Self::parse(
            PathBuf::from(urdf),
            var(ORIGIN_ENV).as_deref(),
            var(YAW_ENV).as_deref(),
            var(PROP_ENV).as_deref(),
            var(BODY_ENV).as_deref(),
        ))
    }

    fn parse(
        urdf: PathBuf,
        origin: Option<&str>,
        yaw: Option<&str>,
        prop: Option<&str>,
        body: Option<&str>,
    ) -> Result<Self, String> {
        let prop_on = match prop.map(str::trim).unwrap_or("") {
            "" | "on" => true,
            "off" => false,
            other => return Err(format!("{PROP_ENV}=\"{other}\" is neither on nor off")),
        };
        let body = match body.map(str::trim).unwrap_or("") {
            "" | "off" => None,
            "all" => Some(Sides::All),
            "allies" => Some(Sides::Allies),
            "axis" => Some(Sides::Axis),
            other => {
                return Err(format!(
                    "{BODY_ENV}=\"{other}\" is none of all, allies, axis, off"
                ));
            }
        };
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
        Ok(Self {
            urdf,
            prop: prop_on.then_some(placement),
            body,
        })
    }
}

/// Loads (once per process and URDF) and installs the configured robot.
/// `None` when no robot is configured; otherwise its report lines.
pub fn install_from_env(slots: MatchSlots<'_>) -> Option<Result<Vec<String>, String>> {
    let config = match Config::from_env()? {
        Ok(config) => config,
        Err(error) => return Some(Err(error)),
    };
    Some(load_cached(&config.urdf).and_then(|model| install::install(&model, &config, slots)))
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
