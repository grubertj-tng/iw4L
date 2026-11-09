use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread::JoinHandle;
use std::time::Instant;

use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Capturing, Screenshot, ScreenshotCaptured};
use net::PresentedSnapshot;

use crate::FrameIndex;

pub const VIDEO_ENV: &str = "IW4L_RFM_VIDEO";
pub const VIDEO_CODEC_ENV: &str = "IW4L_RFM_VIDEO_CODEC";
pub const VIDEO_FPS_ENV: &str = "IW4L_RFM_VIDEO_FPS";

const FFMPEG_ENV: &str = "IW4L_FFMPEG";
const DEFAULT_FPS: u32 = 60;

/// Frames read back but not yet handed to the encoder. A full queue drops the
/// frame instead of stalling the game.
const QUEUE_FRAMES: usize = 8;

/// Readbacks requested but not yet returned. Each holds a window-sized texture
/// and buffer on the GPU.
const IN_FLIGHT_LIMIT: usize = 8;

/// A requested frame whose readback has not returned this many frames later is
/// written off, so the frames after it can be released.
const GIVE_UP_FRAMES: u64 = 120;

const CSV_HEADER: &str =
    "segment,video_frame,frame,real_s,presented_tick,latency_frames,latency_ms";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Codec {
    Nvenc,
    X264,
    Lossless,
}

impl Codec {
    fn from_env() -> Result<Self, String> {
        match std::env::var(VIDEO_CODEC_ENV).ok().as_deref() {
            None | Some("" | "nvenc") => Ok(Self::Nvenc),
            Some("x264") => Ok(Self::X264),
            Some("lossless") => Ok(Self::Lossless),
            Some(other) => Err(format!(
                "{VIDEO_CODEC_ENV}={other}: expected nvenc, x264 or lossless"
            )),
        }
    }

    fn args(self) -> &'static [&'static str] {
        match self {
            Self::Nvenc => &[
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p4",
                "-rc",
                "vbr",
                "-cq",
                "20",
                "-b:v",
                "0",
            ],
            Self::X264 => &[
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p",
            ],
            Self::Lossless => &["-c:v", "libx264rgb", "-preset", "ultrafast", "-qp", "0"],
        }
    }
}

/// Main-world facts of the frame whose picture a `Screenshot` entity captures.
/// The screenshot is extracted with the frame that spawned it, so the image
/// shows that frame.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct FrameCapture {
    frame: u64,
    real_s: f64,
    tick: Option<sim::Tick>,
    requested: Instant,
}

struct CapturedFrame {
    capture: FrameCapture,
    latency_frames: u64,
    latency_ms: f64,
    width: u32,
    height: u32,
    format: TextureFormat,
    data: Vec<u8>,
}

#[derive(Default, Debug)]
struct WorkerReport {
    encoded: u64,
    segments: u32,
    unsupported: u64,
    encoder_failed: u64,
}

#[derive(Resource)]
pub struct VideoRecorder {
    dir: PathBuf,
    sender: Option<SyncSender<CapturedFrame>>,
    worker: Option<JoinHandle<WorkerReport>>,
    outstanding: BTreeSet<u64>,
    ready: BTreeMap<u64, CapturedFrame>,
    requested: u64,
    skipped: u64,
    yielded: u64,
    dropped: u64,
    lost: u64,
}

impl VideoRecorder {
    pub fn create(dir: &Path) -> Result<Self, String> {
        let codec = Codec::from_env()?;
        let fps = match std::env::var(VIDEO_FPS_ENV) {
            Ok(value) => value
                .parse::<u32>()
                .ok()
                .filter(|fps| *fps > 0)
                .ok_or_else(|| format!("{VIDEO_FPS_ENV}={value}: expected a positive integer"))?,
            Err(_) => DEFAULT_FPS,
        };
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let csv_path = dir.join("frames.csv");
        let mut csv = BufWriter::new(
            File::create(&csv_path).map_err(|e| format!("{}: {e}", csv_path.display()))?,
        );
        writeln!(csv, "{CSV_HEADER}").map_err(|e| e.to_string())?;
        let (sender, receiver) = sync_channel(QUEUE_FRAMES);
        let encoder = Encoder {
            dir: dir.to_owned(),
            codec,
            fps,
            csv,
            receiver,
        };
        let worker = std::thread::Builder::new()
            .name("rfm-video".into())
            .spawn(move || encoder.run())
            .map_err(|e| e.to_string())?;
        Ok(Self {
            dir: dir.to_owned(),
            sender: Some(sender),
            worker: Some(worker),
            outstanding: BTreeSet::new(),
            ready: BTreeMap::new(),
            requested: 0,
            skipped: 0,
            yielded: 0,
            dropped: 0,
            lost: 0,
        })
    }

    /// Readbacks complete out of order. A frame goes to the encoder only once
    /// every older requested frame has arrived or been given up, so the video
    /// is in `FrameIndex` order.
    fn release(&mut self) {
        let limit = self.outstanding.first().copied().unwrap_or(u64::MAX);
        while let Some(entry) = self.ready.first_entry()
            && *entry.key() < limit
        {
            let frame = entry.remove();
            let Some(sender) = self.sender.as_ref() else {
                return;
            };
            match sender.try_send(frame) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => self.dropped += 1,
                Err(TrySendError::Disconnected(_)) => self.sender = None,
            }
        }
    }

    fn finish(&mut self) {
        self.lost += self.outstanding.len() as u64;
        self.outstanding.clear();
        self.release();
        self.sender = None;
        let Some(worker) = self.worker.take() else {
            return;
        };
        let Ok(report) = worker.join() else {
            diag::error!(Launch, "rfm_io: the video encoder thread panicked");
            return;
        };
        diag::info!(
            Launch,
            "rfm_io: video in {}: {} frames requested, {} encoded in {} segment(s), \
             {} skipped (readbacks in flight), {} yielded to another screenshot, {} dropped (encoder queue full), \
             {} lost (readback never returned), {} unsupported format, \
             {} lost to an encoder failure",
            self.dir.display(),
            self.requested,
            report.encoded,
            report.segments,
            self.skipped,
            self.yielded,
            self.dropped,
            self.lost,
            report.unsupported,
            report.encoder_failed,
        );
    }
}

impl Drop for VideoRecorder {
    fn drop(&mut self) {
        self.finish();
    }
}

type OtherScreenshot = (With<Screenshot>, Without<FrameCapture>, Without<Capturing>);

pub(crate) fn request_capture(
    mut commands: Commands,
    mut video: ResMut<VideoRecorder>,
    frame: Res<FrameIndex>,
    real: Res<Time<Real>>,
    presented: Option<Res<PresentedSnapshot>>,
    other_screenshots: Query<(), OtherScreenshot>,
) {
    if video.sender.is_none() {
        return;
    }
    while let Some(&oldest) = video.outstanding.first()
        && frame.0.saturating_sub(oldest) > GIVE_UP_FRAMES
    {
        video.outstanding.pop_first();
        video.lost += 1;
    }
    video.release();
    // Bevy captures one screenshot per window and frame and drops the others,
    // so a screenshot taken by the game itself goes first.
    if !other_screenshots.is_empty() {
        video.yielded += 1;
        return;
    }
    if video.outstanding.len() >= IN_FLIGHT_LIMIT {
        video.skipped += 1;
        return;
    }
    video.requested += 1;
    video.outstanding.insert(frame.0);
    commands.spawn((
        Screenshot::primary_window(),
        FrameCapture {
            frame: frame.0,
            real_s: real.elapsed_secs_f64(),
            tick: presented.as_deref().and_then(PresentedSnapshot::tick),
            requested: Instant::now(),
        },
    ));
}

pub(crate) fn on_captured(
    mut captured: On<ScreenshotCaptured>,
    captures: Query<&FrameCapture>,
    frame: Res<FrameIndex>,
    mut video: ResMut<VideoRecorder>,
) {
    let Ok(capture) = captures.get(captured.entity).copied() else {
        return;
    };
    if !video.outstanding.remove(&capture.frame) {
        return;
    }
    let image = &mut captured.event_mut().image;
    let size = image.texture_descriptor.size;
    let format = image.texture_descriptor.format;
    let Some(data) = image.data.take() else {
        video.lost += 1;
        return;
    };
    video.ready.insert(
        capture.frame,
        CapturedFrame {
            capture,
            latency_frames: frame.0.saturating_sub(capture.frame),
            latency_ms: capture.requested.elapsed().as_secs_f64() * 1e3,
            width: size.width,
            height: size.height,
            format,
            data,
        },
    );
    video.release();
}

pub(crate) fn finish_on_exit(mut exit: MessageReader<AppExit>, mut video: ResMut<VideoRecorder>) {
    if exit.read().next().is_some() {
        video.finish();
    }
}

fn raw_pixel_format(format: TextureFormat) -> Option<&'static str> {
    match format {
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => Some("bgr0"),
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => Some("rgb0"),
        _ => None,
    }
}

struct Segment {
    child: Child,
    stdin: Option<ChildStdin>,
    width: u32,
    height: u32,
    format: TextureFormat,
    frames: u64,
    index: u32,
}

impl Segment {
    fn close(mut self) {
        drop(self.stdin.take());
        match self.child.wait() {
            Ok(status) if status.success() => {}
            Ok(status) => diag::error!(
                Launch,
                "rfm_io: ffmpeg for segment {} exited with {status}",
                self.index
            ),
            Err(error) => diag::error!(Launch, "rfm_io: waiting for ffmpeg failed: {error}"),
        }
    }
}

struct Encoder {
    dir: PathBuf,
    codec: Codec,
    fps: u32,
    csv: BufWriter<File>,
    receiver: Receiver<CapturedFrame>,
}

impl Encoder {
    fn run(mut self) -> WorkerReport {
        let mut report = WorkerReport::default();
        let mut segment: Option<Segment> = None;
        let mut failed_shape: Option<(u32, u32, TextureFormat)> = None;
        while let Ok(frame) = self.receiver.recv() {
            let shape = (frame.width, frame.height, frame.format);
            let Some(pix_fmt) = raw_pixel_format(frame.format) else {
                if report.unsupported == 0 {
                    diag::error!(Launch, "rfm_io: cannot encode {:?} frames", frame.format);
                }
                report.unsupported += 1;
                continue;
            };
            if segment
                .as_ref()
                .is_some_and(|s| (s.width, s.height, s.format) != shape)
                && let Some(open) = segment.take()
            {
                open.close();
            }
            if segment.is_none() && failed_shape != Some(shape) {
                match self.open(report.segments, &frame, pix_fmt) {
                    Ok(opened) => {
                        report.segments += 1;
                        failed_shape = None;
                        segment = Some(opened);
                    }
                    Err(error) => {
                        diag::error!(Launch, "rfm_io: cannot start ffmpeg: {error}");
                        failed_shape = Some(shape);
                    }
                }
            }
            let Some(open) = segment.as_mut() else {
                report.encoder_failed += 1;
                continue;
            };
            let written = open
                .stdin
                .as_mut()
                .map(|stdin| stdin.write_all(&frame.data));
            if let Some(Err(error)) = written {
                diag::error!(
                    Launch,
                    "rfm_io: ffmpeg stopped taking frames (see ffmpeg.log): {error}"
                );
                if let Some(open) = segment.take() {
                    open.close();
                }
                failed_shape = Some(shape);
                report.encoder_failed += 1;
                continue;
            }
            let c = frame.capture;
            let tick = c.tick.map_or(String::new(), |t| t.0.to_string());
            let _ = writeln!(
                self.csv,
                "{},{},{},{:.6},{tick},{},{:.3}",
                open.index, open.frames, c.frame, c.real_s, frame.latency_frames, frame.latency_ms,
            );
            open.frames += 1;
            report.encoded += 1;
        }
        if let Some(open) = segment.take() {
            open.close();
        }
        if let Err(error) = self.csv.flush() {
            diag::error!(Launch, "rfm_io: writing frames.csv failed: {error}");
        }
        report
    }

    fn open(&self, index: u32, frame: &CapturedFrame, pix_fmt: &str) -> Result<Segment, String> {
        let name = if index == 0 {
            "video.mp4".to_owned()
        } else {
            format!("video-{index}.mp4")
        };
        let log_path = self.dir.join("ffmpeg.log");
        let log = File::options()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("{}: {e}", log_path.display()))?;
        let ffmpeg = std::env::var_os(FFMPEG_ENV).unwrap_or_else(|| "ffmpeg".into());
        let mut child = Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "warning", "-y"])
            .args(["-f", "rawvideo", "-pix_fmt", pix_fmt])
            .args(["-video_size", &format!("{}x{}", frame.width, frame.height)])
            .args(["-framerate", &self.fps.to_string(), "-i", "pipe:0"])
            .args(self.codec.args())
            .args(["-movflags", "+faststart"])
            .arg(self.dir.join(&name))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .map_err(|e| format!("{}: {e}", PathBuf::from(&ffmpeg).display()))?;
        let stdin = child.stdin.take();
        diag::info!(
            Launch,
            "rfm_io: encoding {}x{} {:?} at {} fps ({:?}) to {}",
            frame.width,
            frame.height,
            frame.format,
            self.fps,
            self.codec,
            self.dir.join(&name).display()
        );
        Ok(Segment {
            child,
            stdin,
            width: frame.width,
            height: frame.height,
            format: frame.format,
            frames: 0,
            index,
        })
    }
}
