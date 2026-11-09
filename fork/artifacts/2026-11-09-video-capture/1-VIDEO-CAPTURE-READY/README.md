# 1 — Video capture: seam, attribution, cost

Code: `crates/rfm_io/src/video.rs`, wiring in `crates/rfm_io/src/lib.rs`, usage
in `crates/rfm_io/README.md` ("Video").

## Seam: V1, a per-frame Bevy `Screenshot`

Every frame, a system in `Last` spawns `Screenshot::primary_window()` with a
`FrameCapture` component (`FrameIndex`, `real_s`, presented tick, request
instant). A global observer on `ScreenshotCaptured` takes the pixel `Vec`
(no copy), files it under its `FrameIndex` and hands frames to a worker thread
through a bounded channel (8). The worker pipes raw `bgr0` into
`ffmpeg -f rawvideo … -c:v h264_nvenc` and writes `frames.csv`.

Why V1: no upstream edit, and the attribution is structural. Bevy's
`extract_screenshots` reads the `Screenshot` entities of the main world at the
extract that follows the frame which spawned them, and redirects that extract's
window output into the capture texture. So the image is the render of exactly
the frame that spawned the entity, whether the render runs in the same cycle
(pipelining off) or the next (on). V2 (fixed-size offscreen image) and V3 (a
render-world copy node with a buffer pool) would save the per-frame texture and
buffer allocation; not needed at 60 fps (below).

Findings on the way:
* **Readbacks complete out of order.** Bevy maps each screenshot buffer in its
  own async task. Run `video-3-nopipe` (before the fix) wrote frame 5 before
  frame 4. Fixed: the main thread keeps the set of outstanding requests and
  releases a frame to the encoder only when no older request is outstanding
  (a request older than 120 frames is written off). All later runs:
  `frames.csv` strictly increasing.
* **Bevy captures one screenshot per window per frame** and despawns the
  duplicates. With `IW4L_BENCH=1` the bench screenshot lost to ours, the bench
  never got its image and the run hung after `finish_run` until `timeout`
  (`video-5-bench`, not usable). Fixed: the request runs in `Last` and skips
  the frame when another `Screenshot` is pending (counted as "yielded");
  `video-6-bench` then completed with 1 yielded frame.
* `info!` from `rfm_io` was filtered by the log filter (`warn,iw4l=info`); the
  crate now logs through `diag` (`launch` channel), including the recorder.

## Attribution — evidence

Method (`attribution.py.txt`): decode the video to 192-px grey, mean absolute
difference to the previous video frame; join `frames.csv` (video frame →
`FrameIndex`) with `movement.csv` (`FrameIndex` → `view_yaw/view_pitch`). For
every isolated view change > 1° in the recorder (from scripted `look 90 0`,
`look 180 0`, `look 270 0`, `look 0 30`), find the largest image change within
−3…+4 frames. Still-view image difference is 0.04–0.11 (grey levels); the look
frames are 34–81.

| run | window | pipelined | codec | offsets (image − recorder) |
|---|---|---|---|---|
| video-2 | 3840×1531 | on | nvenc | 0, 0, 0, 0 |
| video-3-nopipe | 3840×1531 | off | nvenc | 0, 0, 0, 0 |
| video-4 | 3840×1531 | on | nvenc | 0, 0, 0, 0 |
| video-6-bench (uncapped) | 3840×1531 | on | nvenc | 0, 0, 0, 0 |
| video-7-720 | 1280×720 | on | nvenc | 0, 0, 0, 0 |
| video-8-720-bench (uncapped) | 1280×720 | on | nvenc | 0, 0, 0, 0 |
| video-9-720-lossless | 1280×720 | on | lossless | 0, 0, 0, 0 |
| video-10-final (committed code) | 3840×1531 | on | nvenc | 0, 0, 0, 0 |

Example (video-2): recorder row 698 has yaw 0 → 90; image difference of the
video frame tagged 697: 0.016, 698: **33.98**, 699: 0.053. **Measured offset:
0 frames.** The video frame tagged `frame` N shows the player state of
`movement.csv` row N. (8 runs × 4 events, 32/32.)

Note this is the frame *rendered from* row N's state. What a human saw when
producing row N's command is earlier (row N−1 or older plus display latency);
that is a modelling question, not a capture one.

## Cost — evidence

Frame time from `movement.csv` `real_s` while alive (`cost.py.txt`).

| run | median ms | p90 | p99 | fps |
|---|---|---|---|---|
| 3840×1531, 60 Hz throttle, no capture (video-4-base) | 16.63 | 19.92 | 28.06 | 59.7 |
| 3840×1531, 60 Hz throttle, capture (video-4) | 17.02 | 22.07 | 31.14 | 59.2 |
| 3840×1531, 60 Hz throttle, capture (video-10-final) | 16.63 | 20.68 | 28.21 | 60.0 |
| 1280×720, 60 Hz, no capture (video-7-720-base) | 16.64 | 20.21 | 32.31 | 59.8 |
| 1280×720, 60 Hz, capture (video-7-720) | 16.50 | 20.26 | 32.98 | 59.9 |
| 3840×1531, uncapped, no capture (video-5-bench-base) | 3.02 | 4.95 | 8.43 | 283.6 |
| 3840×1531, uncapped, capture (video-6-bench) | 4.91 | 8.37 | 13.35 | 183.7 |
| 1280×720, uncapped, no capture (video-8-720-bench-base) | 3.17 | 4.88 | 8.11 | 282.5 |
| 1280×720, uncapped, capture (video-8-720-bench) | 4.03 | 6.28 | 10.22 | 224.2 |

Losses (exit log line):
* 60 Hz runs: every alive frame captured except the last 2 (in flight at
  exit). 28–56 frames dropped at the start of the gameplay segment while ffmpeg
  and NVENC start (~0.5–1 s), during the loading screen. At 1280×720 the window
  never changes size, one segment, 0 dropped.
* Uncapped 3840×1531: 1600 of 5351 requested encoded (~79 fps through the
  encoder), 814 skipped (8 readbacks in flight), 3748 dropped (queue full).
* Uncapped 1280×720: 7019 of 7198 encoded, 176 dropped.
* Readback latency at 60 Hz: median 29–30 ms, 2 frames (max 4).

## Conditions of these measurements

* The session was locked and the monitor off (DPMS). With vsync the game
  presented at 1 Hz, so every run used `IW4L_PRESENT_MODE=AutoNoVsync`; the
  unfocused window then runs at Bevy's 60 Hz unfocused throttle.
  `IW4L_BENCH=1` lifts that throttle ("uncapped").
* Another `iw4l` instance (not started by this slice, `bot add 5`, running for
  the whole session) shared the GPU during all runs, baselines included.
* Scratch profile `context/scratch-video-unlocked/`: a copy of
  `context/scratch-video/` with `unlock all` applied (the original's classes
  were locked and the player never spawned). `settings-720.cfg` sets
  `resolution=1280x720`. The requested 3840×2160 becomes 3840×1531 after the
  window manager fits it to the 3840×1600 screen; the first frame is at
  3840×2160 and lands alone in `video.mp4`.
* Recordings: `context/recordings/video-*` (this machine only).

## What was not done

* **No real focused play.** `mouserate` turns were dropped (unfocused window),
  so continuous rotation and movement were not checked in video — only step
  view changes. A human session with the monitor on is the next check.
* Vsync-paced capture with a visible window was not measured (monitor off).
* No downscaling; capture size = window size. A 4K window at uncapped frame
  rate drops most frames; dataset runs need a frame cap and a smaller window
  (or a GPU downscale — V2/V3 — later).
* The first ~0.5–1 s of each segment is dropped while the encoder starts.
* The last 2 frames before exit are lost (readbacks in flight).
* The video's frame rate is nominal (`IW4L_RFM_VIDEO_FPS`, default 60), one
  video frame per game frame; real timing is `frames.csv` `real_s`. A
  LeRobot export has to resample to its fixed fps from that.
* Lossless output was not compared pixel-for-pixel against an independent
  screenshot.
* No tests (repository rule). Probe scripts are kept here as `.txt`; nothing
  probe-like is in `crates/`.
