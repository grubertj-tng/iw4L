# 7 — Saving episodes as LeRobot datasets: design and first conversion

User request: record and save episodes in LeRobot format. Start thinking about
how. This slice ran in parallel with slice 6 (inference adapter) and with
video capture (`fork/artifacts/2026-11-09-video-capture/`). Two agents built
those in `context/mrs/adapter` and `context/mrs/video`. It was written as slice
6 too and renumbered to 7 when the branches were merged.

## The format (checked against the installed package, not only the docs)

`lerobot` 0.7.0 is installed in `context/venv-lerobot` (`lerobot[dataset]`).
Dataset format v3.0 ([docs](https://huggingface.co/docs/lerobot/en/lerobot-dataset-v3)).

**Layout**
* `meta/info.json`: schema, fps, path templates.
* `meta/stats.json`: per-feature mean/std/min/max, used for normalisation.
* `meta/tasks.parquet`: task texts → ids.
* `meta/episodes/…parquet`: per-episode lengths and offsets.
* `data/chunk-*/file-*.parquet`: frame rows, many episodes per file.
* `videos/<key>/chunk-*/file-*.mp4`: many episodes per file.

**API**
* `LeRobotDataset.create(repo_id, fps, features, root, robot_type, use_videos, …)`
* `add_frame({...features, "task": str})`
* `save_episode()`
* `finalize()` — mandatory; without it the parquet footers are missing.

**Key constraints**
* `add_frame` sets **`timestamp = frame_index / fps`**. The dataset has a
  constant rate by construction (`dataset_writer.py`).
* Video frames are matched to rows by timestamp, with `tolerance_s = 1e-4`.
* Images are passed per frame as `(H, W, 3)` uint8. LeRobot encodes the
  episode video itself; the RGB default codec comes from `rgb_encoder_defaults()`.

## Design: two stages, raw first

1. **Raw session** (what the game writes, full frame rate, nothing lost):
   * `movement.csv` (recorder);
   * video and `frames.csv` (capture, keyed by the same `FrameIndex`);
   * the session's facts: map, mode, build, window size.

   Raw sessions are the source of truth. They stay under
   `context/recordings/`, local to the machine.
2. **Converter** `fork/tools/to_lerobot.py` (runs with `context/venv-lerobot`).
   It reads a raw session (plus `rfm_roundtrip`'s `roundtrip.csv`, so actions
   use the current definition) and writes a LeRobot dataset. It can be re-run
   with another fps, another action definition or another resolution, without
   re-recording.

This keeps the game side free of LeRobot and Python. It also matches the
earlier decision that normalisation and control rate belong to the training
side.

**Why not write LeRobot directly from the game?** The game runs at about 60 Hz
with jitter (median 16.66 ms, spikes ≥ 100 ms), and LeRobot needs a constant
rate. Resampling needs to see the next frame anyway. And `lerobot` is Python
plus torch, which does not belong in the runtime.

### Mapping

| LeRobot | from | notes |
|---|---|---|
| episode | one life: commands while moving, split at respawn (`delta_angles` change) and death | frames without control (menus, killcam, spectating) are not in the action data |
| step k | the last game frame at or before `t0 + k/fps` (wall time) | default fps 20 |
| `observation.images.<cam>` | the video frame with that step's `FrameIndex` | **pending** video capture |
| `observation.state` (10) | actual velocity in the yaw frame (fwd, right, up), view pitch, on ground, sprinting, crouched, prone, ADS fraction, current max speed | the player's own body, from `PlayerState` |
| `action` (16) | intended velocity fwd/right (u/s), yaw/pitch rate (deg/s), sprint, crouch, prone, attack, ads, hold_breath, jump, reload, melee, use, frag, smoke | physical units, named |
| `game_frame` | the step's `FrameIndex` | traces every row back to the raw session |
| `task` | text | for now one per session, e.g. "Play free-for-all against bots on mp_boneyard." |

**Action aggregation over a step:**
* **rates:** total turn ÷ step time. This is exact: it integrates to the same
  angle.
* **intended velocity, sprint, stance and held buttons** (attack, ADS): the
  value at the end of the step.
* **momentary buttons** (jump, reload, melee, use, frag, smoke): pressed at any
  command in the step, so a 1-frame press is not lost.
* **hold_breath:** only while aiming. The sprint key sets the same bit, which
  made the raw bit 6 % noise.

## Evidence: first conversion

`play-2` → `context/datasets/play-2-lerobot`, without images:
* **8 episodes (lives), 4,945 steps at 20 Hz (247 s).**
* Reloaded with `LeRobotDataset(repo_id, root=…)`; LeRobot wrote `stats.json`.

Action ranges:
* intended fwd −165 … 330 u/s; intended right −335 … 273 u/s;
* yaw −841 … 1,219 deg/s; pitch −179 … 186 deg/s;
* sprint 29 %, ADS 26 %, attack 12 %.

Turning check: the dataset's yaw totals 9,493° against 9,516° in the raw
commands (−0.24 %). The gap is back-and-forth turns cancelling inside a 50 ms
step, plus the trimmed ends of each life.

## Decisions for the user

1. **fps of the dataset.** 20 Hz (the sim tick, 3 game frames per step), 30, or
   60. The raw data allows any of them later.
2. **Episode = life?** Alternatives: one match, or fixed-length chunks.
3. **Image.** Resolution (models typically use 224–256 px; the capture is the
   window size), aspect (16:9 letterbox or crop), and HUD on or off. The HUD
   carries ammo, minimap and hit markers.
4. **Task text.** One fixed text per session, or richer language annotations
   later (LeRobot 0.7 has language columns).
5. **Where datasets live.** Local (`context/datasets/`) or a private Hugging
   Face Hub repo.

## What was not done

* **Images are not in the dataset yet.** Video capture has now landed with a
  measured offset of 0 frames: `frames.csv` row N shows `movement.csv` row N.
  Wiring it into the converter is next. `--no-video` is required until then.
* The converter is a prototype script: no tests, one session per run.
  Merging sessions into one dataset (LeRobot's `aggregate`) is not tried.
* Stance and sprint labels come from state flags, as decided. Momentary
  buttons use OR-in-step.
* Neither a training run nor a training-side loader was tried.
* Scaffolding: none in `crates/`.
