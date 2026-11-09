pub mod action;
mod record;
mod video;

use std::path::PathBuf;

use bevy::prelude::*;
use frame::ClientSet;

pub use record::MovementRecorder;
pub use video::{VIDEO_CODEC_ENV, VIDEO_ENV, VIDEO_FPS_ENV, VideoRecorder};

pub const RECORD_ENV: &str = "IW4L_RFM_RECORD";

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
                    diag::info!(Launch, "rfm_io: recording movement to {}", path.display());
                    Some(recorder)
                }
                Err(error) => {
                    diag::error!(Launch, "rfm_io: cannot create {}: {error}", path.display());
                    None
                }
            });
        let video = std::env::var_os(VIDEO_ENV)
            .map(PathBuf::from)
            .and_then(|dir| match VideoRecorder::create(&dir) {
                Ok(video) => {
                    diag::info!(Launch, "rfm_io: capturing video to {}", dir.display());
                    Some(video)
                }
                Err(error) => {
                    diag::error!(
                        Launch,
                        "rfm_io: cannot capture video to {}: {error}",
                        dir.display()
                    );
                    None
                }
            });
        if recorder.is_none() && video.is_none() {
            return;
        }
        app.init_resource::<FrameIndex>()
            .add_systems(First, advance_frame_index);
        if let Some(recorder) = recorder {
            app.insert_resource(recorder)
                .add_systems(Update, record::record_frame.in_set(ClientSet::Diag))
                .add_systems(Last, record::flush_on_exit);
        }
        if let Some(video) = video {
            app.insert_resource(video)
                .add_observer(video::on_captured)
                .add_systems(
                    Last,
                    (video::request_capture, video::finish_on_exit).chain(),
                );
        }
    }
}

fn advance_frame_index(mut index: ResMut<FrameIndex>) {
    index.0 += 1;
}
