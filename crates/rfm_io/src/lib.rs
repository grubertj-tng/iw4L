pub mod action;
mod record;

use std::path::PathBuf;

use bevy::prelude::*;
use frame::ClientSet;

pub use record::MovementRecorder;

pub const RECORD_ENV: &str = "IW4L_RFM_RECORD";

/// Counts `Update` passes. Every interface keys its rows by it, so a row in
/// one stream and a row in another with the same index belong to the same
/// main-world frame.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameIndex(pub u64);

pub struct RfmIoPlugin;

impl Plugin for RfmIoPlugin {
    fn build(&self, app: &mut App) {
        let Some(path) = std::env::var_os(RECORD_ENV).map(PathBuf::from) else {
            return;
        };
        let recorder = match MovementRecorder::create(&path) {
            Ok(recorder) => recorder,
            Err(error) => {
                error!("rfm_io: cannot create {}: {error}", path.display());
                return;
            }
        };
        info!("rfm_io: recording movement to {}", path.display());
        app.init_resource::<FrameIndex>()
            .insert_resource(recorder)
            .add_systems(First, advance_frame_index)
            .add_systems(Update, record::record_frame.in_set(ClientSet::Diag))
            .add_systems(Last, record::flush_on_exit);
    }
}

fn advance_frame_index(mut index: ResMut<FrameIndex>) {
    index.0 += 1;
}
