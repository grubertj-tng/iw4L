# 8 — 1080p session, images in the LeRobot dataset, zero-time rotation fix

Decisions by the user before this slice:
* episode = one life;
* play at 1920×1080;
* the model's input resolution may differ from the played resolution.
  Inference renders at 1080p for the viewer, and the model gets the
  downscaled image.

## Session `play-3` (local: `context/recordings/play-3/`)

The user played against 5 bots. The game ran from a **copy** of the user's
settings with `resolution=1920x1080` (`context/session-settings/settings.cfg`;
the user's settings are unchanged), windowed, vsync, FOV 65. The recorder and
video capture were on.

* **Recorder:** 16,797 frames, 15,658 commands, 15,026 of them while alive,
  12 respawns. Frame time: median 16.66 ms, p99 18.3 ms.
* **Video:** `video.mp4`, 1920×1080 H.264 (NVENC), one segment.
  * 17,360 frames encoded of 17,366 requested: 1 dropped (encoder queue), 5
    lost (readback never returned).
  * **Every alive frame has its video frame (15,026 of 15,026).**
  * Capture latency from render to pixels: 5 frames (median and p99) with
    vsync on. Earlier runs without vsync: 2–3 frames. Relevant for inference.
  * 1.2 GB for 5 min.

## Round trip, and a fix: commands with a zero-length step

`rfm_roundtrip` on `play-3` first showed **78 rotation mismatches** (max
0.66°); play-2 had 0.
* **Cause:** one command (frame 1924) had `msec = 0` but a 12-unit view
  change. A rate cannot express a turn in no time, so it was labelled 0. The
  lost angle persisted until the next respawn.
* **Fix:** `action::RateTracker` carries the angle change of a zero-time
  command into the next command with time. The recorder and `rfm_roundtrip`
  both use it. `rfm_roundtrip` now recomputes rates from the command angles
  instead of reading the recorded column, so existing sessions relabel
  correctly without re-recording.
* **After the fix:**
  * play-2: rotation 0 mismatches, 7 zero-time commands carried;
  * play-3: 0 mismatches, 24 carried;
  * movement bit-exact on both (play-3 reconstructs 99.93 % of commands
    exactly).
* The adapter already repeats the previous command on `msec = 0` and consumes
  no action, which is consistent with this.

## Images in the converter

**`fork/tools/rfm_image.py`** is the one transform from a captured frame to
the model input.
* `ImageSpec(width, height, mode)` and `prepare(frame, spec)`.
* PIL box filter; `fit` letterboxes, `stretch` scales each axis.
* The converter imports it, and the inference runner must import it too.

**`fork/tools/to_lerobot.py`** now:
* looks up each step's observation `FrameIndex` in `frames.csv`;
* decodes the segment's mp4 with PyAV and prepares the image;
* writes `observation.images.main` as a LeRobot video feature.

Options: `--image-size` (default 256x144) and `--image-mode`; `--no-video`
still works.

**`play-3` → `context/datasets/play-3-lerobot`:**
* 13 episodes (lives), 4,992 steps at 20 Hz, 256×144 fit (16:9, so no bars);
* LeRobot encoded AV1 (SVT, CRF 30): 22 MB;
* conversion took 42 s.

**Frame-alignment check.** For 40 random dataset rows, I compared the
dataset image with the prepared raw frame at offsets −3 … +3 around its
`game_frame`.

| offset | −3 | −2 | −1 | 0 | +1 | +2 | +3 |
|---|---|---|---|---|---|---|---|
| median MSE | 528 | 437 | 270 | **35** | 235 | 423 | 553 |

39 of 40 match best at offset 0. The one at −1 is probably a nearly static
scene; it was not checked further. The 35 is AV1 compression.

**Preview:** `context/datasets/play-3-preview.png`, 12 dataset frames with
episode, game frame and action.

## Found: menus inside episodes

The last episode ends on the in-game quit dialog. The player was still alive
while the menu had input focus. Such frames carry no control (zero commands)
and should not be in the action data.
* The recorder does not log whether a menu, console or chat captures input.
* **Fix (next):** log it (the client's input-capture state, the same gate that
  drops scripted keys) and cut episodes there.

## What was not done

* Menu, console and chat frames are not cut from episodes yet.
* Image size and HUD on/off are still the user's choice. At 256×144 the HUD
  text is unreadable, as expected.
* There is no inference runner yet that uses `rfm_image.prepare` on live
  capture. The capture latency (5 frames with vsync) is measured but not
  handled.
* Raw 1080p NVENC is about 4 MB/s. Long sessions need disk space or a lower
  bitrate.
* Scaffolding: none in `crates/`. The alignment check and preview were inline
  scripts (above).
