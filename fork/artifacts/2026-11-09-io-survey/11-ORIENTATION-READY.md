# 11 — Orientation: yaw as a rate, pitch absolute, heading as (sin, cos)

## Decisions (user, after discussing the movement pipeline)

The robotics framing: the sim's command interface (`UserCmd`) is the cmd_vel
topic. `pmove`'s friction and acceleration are the base controller.

* **Linear motion:** the action stays the **intended velocity** (the base
  controller's output on empty flat ground). The model's action enters the
  game **through the setpoint, by inversion** (option (i)): the adapter finds
  the `UserCmd` that makes the game's own controller produce it.
  * No game code changes.
  * The same physics as for humans.
  * Exact whenever the target is physically reachable.

  Overwriting the controller output inside `pmove` (option (ii)) was rejected:
  it changes the movement core and the wire protocol, and it removes the
  acceleration limits, so it would amount to a speed hack.
* **Actual velocity** (fwd, right, up) stays in the observation.
* **Yaw:** a **rate** (deg/s). The game keeps yaw as an unwrapped command
  counter (65,536 per turn) plus a server offset, and wraps the view to
  [−180°, 180°). Yaw wraps around, is unbounded and cannot be seen in the
  image, so it is relative.
* **Pitch:** **absolute** (deg). It is bounded (the sim clamps ±85°),
  gravity-referenced (visible in the image as the horizon) and drift-free, and
  the sim commands it absolutely anyway.
* **Heading in the observation:** the world yaw as `(sin, cos)`. Continuous
  around the ring and already in [−1, 1].

## Changes (`crates/rfm_io`, `fork/tools`)

* **`drive::Action`:** `rate: [f32; 2]` → `yaw_rate`, `pitch`.
* **`ActionSource::current`** returns `Planned { action, remaining_ms }`.
  `remaining_ms` is the game time from the command's step start to the end of
  the row.
* **Adapter pitch** (`pitch_toward`): the target is turned into command units
  through `delta_angles`. Each command covers `msec / remaining_ms` of the
  remaining difference, at most 240 deg/s, then the existing ±85° guard
  applies. The command that ends a row lands on the target.
* **Action file:** column `pitch` (absolute) replaces `pitch_rate`.
  * `rfm_actions` writes the recorded view pitch after each command.
  * Idle rows keep the last pitch.
* **`rfm_roundtrip`:**
  * yaw is checked by integrating the rate;
  * pitch by turning the recorded view pitch back into the command angle;
  * **only commands under control are compared** (not dead, not spectating,
    `input_captured = 0`).
* **`rfm_drive_check`:** yaw rate as before; pitch target reached / held by
  the rate limit / held at ±85° / still on its way.
* **`to_lerobot.py`:**
  * action `pitch_deg` = view pitch at the end of the step (replaces
    `pitch_rate_dps`);
  * `observation.state` gains `heading_sin`, `heading_cos` (12 values).

## Evidence

**Round trip, under control:**
* `menu-check`, which has the capture flags: **0 mismatches**.
* `play-2` and `play-3` predate those flags and show 4 and 6 short runs of
  mismatches. **Every one starts exactly at a respawn** (0.0 s after it) and
  lasts 0.1–2.1 s: the class-selection menu, during which the view does not
  follow the command's angles. In `menu-check` the same stretch is flagged as
  `script_menu` and excluded.
* Before restricting the comparison, dead frames (pm_type 8) were also
  compared; that caused the bulk of the first mismatch count.
* Movement stays bit-exact on all three.

**In-game drive** with the `play-3` action files (scratch account, class 4,
uncapped at about 200 fps, 40 s):

| file | commands | yaw: recorded = commanded | pitch at the end of a row = target |
|---|---|---|---|
| one row per original command (17 ms rows) | 8,579 | 100 % | 99.91 % of 2,316 rows |
| 20 Hz (50 ms rows, made from it) | 9,207 | 100 % | 99.88 % of 824 rows |

* The 3 missed rows: 2 are each run's final row, cut by `finish_run`; one
  ended after only 2 of its commands (not investigated further).
* No row hit the 240 deg/s pitch limit.
* `rfm_drive_check`'s per-command pitch numbers (72 % / 51 % "reached") count
  commands in the middle of a row, where pitch is still on its way by design.
  The per-row check above is the meaningful one.

**Converter** (`menu-check` → `context/datasets/menu-check-lerobot`):
* `pitch_deg` −1.4 … 15.5°;
* heading sin/cos cover −1 … 1;
* 4 episodes, as before.

## Noticed, not changed

* **Intended velocity at 5 ms commands.** `rfm_drive_check` reports the
  inverse's local search short of the exhaustive best on 16–17 % of commands,
  by p50 0.1 u/s, max 2.4 u/s. The earlier 17 ms replays showed 1.5 %. The
  velocity target of a row is aimed at from the row's first command on (not
  spread like pitch). The search around the analytic solution is tuned for
  ~17 ms steps. Small, but worth a look.
* `normalization_study.py` still evaluates the pitch *rate*. Absolute pitch
  normalises trivially (÷85), but its uniform-bin resolution (0.66° per bin at
  256 bins) is coarse; see the discussion in the chat.

## What was not done

* No new human recording. The relabelling works offline on the existing raw
  sessions.
* The velocity search at very short steps (above).
* Scaffolding: none. The per-row pitch check and the 20 Hz file were inline
  scripts; the 20 Hz file is under `context/recordings/play-3/`.
