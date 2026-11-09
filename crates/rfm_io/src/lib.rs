pub mod action;
pub mod action_file;
pub mod drive;
mod record;
pub mod table;

use std::path::PathBuf;

use bevy::prelude::*;
use frame::ClientSet;

use action::MoveScales;
use action_file::ActionFile;
use drive::Driver;
pub use record::MovementRecorder;

pub const RECORD_ENV: &str = "IW4L_RFM_RECORD";
pub const DRIVE_ENV: &str = "IW4L_RFM_DRIVE";
pub const DRIVE_LOG_ENV: &str = "IW4L_RFM_DRIVE_LOG";

/// Counts `Update` passes. Every interface keys its rows by it, so a row in
/// one stream and a row in another with the same index belong to the same
/// main-world frame.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameIndex(pub u64);

pub struct RfmIoPlugin;

impl Plugin for RfmIoPlugin {
    fn build(&self, app: &mut App) {
        let recorder = std::env::var_os(RECORD_ENV)
            .map(PathBuf::from)
            .and_then(|path| match MovementRecorder::create(&path) {
                Ok(recorder) => {
                    info!("rfm_io: recording movement to {}", path.display());
                    Some(recorder)
                }
                Err(error) => {
                    error!("rfm_io: cannot create {}: {error}", path.display());
                    None
                }
            });
        let driver = std::env::var_os(DRIVE_ENV)
            .map(PathBuf::from)
            .and_then(|path| create_driver(&path));
        if recorder.is_none() && driver.is_none() {
            return;
        }
        app.init_resource::<FrameIndex>()
            .add_systems(First, advance_frame_index);
        if let Some(recorder) = recorder {
            app.insert_resource(recorder)
                .add_systems(Update, record::record_frame.in_set(ClientSet::Diag))
                .add_systems(Last, record::flush_on_exit);
        }
        if let Some(driver) = driver {
            app.insert_resource(driver)
                .add_systems(
                    Update,
                    (
                        drive::drive_command
                            .in_set(ClientSet::Predict)
                            .after(net::sample_client_input)
                            .before(net::predict_local_move),
                        drive::commit_command
                            .in_set(ClientSet::Predict)
                            .after(net::predict_local_move),
                    ),
                )
                .add_systems(Last, drive::flush_drive_on_exit);
        }
    }
}

fn create_driver(path: &std::path::Path) -> Option<Driver> {
    let source = match ActionFile::load(path) {
        Ok(source) => source,
        Err(error) => {
            error!("rfm_io: cannot load actions {}: {error}", path.display());
            return None;
        }
    };
    let log = std::env::var_os(DRIVE_LOG_ENV).map(PathBuf::from);
    info!(
        "rfm_io: driving the local player from {} ({} actions)",
        path.display(),
        source.len()
    );
    match Driver::new(Box::new(source), log.as_deref()) {
        Ok(driver) => Some(driver),
        Err(error) => {
            error!("rfm_io: cannot create the drive log: {error}");
            None
        }
    }
}

/// Move scales of `weapon`, as the intended velocity uses them.
#[must_use]
pub fn weapon_scales(world: &sim::SimWorld, weapon: u32) -> MoveScales {
    let (weapon_move, weapon_ads_move, _) = world.scales_for(weapon);
    MoveScales {
        weapon_move,
        weapon_ads_move,
        shellshock_affects_movement: true,
    }
}

fn advance_frame_index(mut index: ResMut<FrameIndex>) {
    index.0 += 1;
}
