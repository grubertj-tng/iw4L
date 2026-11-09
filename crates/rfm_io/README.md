# `rfm_io` — action interface for training models on the game

Fork-only crate. It records the local player's actions frame by frame,
converts between the game's commands and the action representation used for
training, and drives the local player from actions (the inference adapter).
The game is unchanged unless `IW4L_RFM_RECORD` or `IW4L_RFM_DRIVE` is set.

## Recording

```bash
set -a; . ./.env; set +a
IW4L_RFM_RECORD=$PWD/context/recordings/<name>/movement.csv target/play/iw4l map mp_boneyard
```

The recorder writes **one row per main-world frame** (`Update` pass), in
`ClientSet::Diag`, after Input → Predict → Send → Present. `frame` is
`FrameIndex`, which counts up in `First`. Every interface keys its rows by
`FrameIndex`, so rows with the same index in different streams belong to the
same frame. `cmd_new = 1` marks the frame's freshly built command. While the
player is alive, every frame has one.

## The action representation

| action | unit | definition | code |
|---|---|---|---|
| intended velocity `(fwd, right)` | u/s, player's yaw frame | the velocity the movement commands produce on unobstructed flat ground (see below) | `MoveFrame::advance` |
| yaw rate, pitch rate | deg/s | command-angle change between consecutive commands ÷ the command's `msec`. Positive yaw turns left, positive pitch looks down | `view_rate`, `angles_after_rate` |
| sprint desired | bool | the recorded `SPRINTING` state | label from `pm_flags`; the adapter holds `SPRINT` while desired |
| stance desired | stand / crouch / prone | the recorded `PRONE` / `CROUCH` state | label from `pm_flags`; the adapter sets the `PRONE` / `CROUCH` bits, which the sim reads as state |
| other buttons | bits | `UserCmd.buttons` as recorded | — |

**No input devices.** The inference adapter writes `UserCmd` directly, after
the client's input path. Key bindings, key fractions, sensitivity, FOV
scaling, the client's stance toggle latch and toggle-ADS binds are not
involved. Two rules of the sim's own command format remain, because the sim
compares each command with the previous one (`old_buttons`):
* jump, reload, melee, grenades and semi-auto fire need one command without
  the bit between activations;
* a sprint is cancelled by an off→on transition of `SPRINT` (`sprint.rs`).
  Ending forward intent, ADS, fire, reload and use end it too.

**Units, not normalisation.** Actions are in physical units here. The
inference adapter clamps the rates to their limits (yaw ±1200, pitch ±240
deg/s: `YAW_RATE_LIMIT_DPS`, `PITCH_RATE_LIMIT_DPS`). Normalisation, its statistics and the control rate
belong to the training side and are stored with the dataset. Recordings stay
at the full frame rate, so actions can be recomputed and resampled offline.

**Intended velocity.** The sim's own `friction`, `cmd_scale_walk` and
`accelerate` (from `movement_iw4`) run on a velocity of their own. Collision,
slopes, gravity and knockback never touch it. It follows stance, sprint,
walking, the weapon's move scale, perks (`move_speed_scale_multiplier`), damage
slow-down and shellshock. It leaves out what belongs to other actions
(`EXCLUDED_FLAGS`):
* jump landings (`JUMPING`);
* hard landings (`TIME_HARDLANDING`);
* melee lunges (`MELEE_CHARGE`);
* ladders and mantles.

It is reset to the actual horizontal velocity whenever the player does not
move (dead, spectating).

**Inverse.** `MoveFrame::command_for(velocity, target, previous)` returns the
`forwardmove/rightmove` that moves the intended velocity from `velocity` to
`target`. It solves the acceleration step for the wish speed, then searches
the neighbouring `i8` commands. When the velocity along a key's direction is
already above the wish speed, pressing the key changes nothing; in those ties
the previous command wins. Rotation inverts by integrating the rate onto the
command angles (`angles_after_rate`).

Known simplification: shellshock is assumed to slow movement whenever the
`SHELLSHOCKED` flag is set. The sim's per-client shellshock parameters are not
public.

## Driving the player (inference adapter)

```bash
IW4L_RFM_DRIVE=<actions.csv> IW4L_RFM_DRIVE_LOG=<drive_log.csv> target/play/iw4l map mp_boneyard
```

`drive.rs` replaces the local player's `ClientCmdTemplate.cmd` every frame,
after `sample_client_input` and before `predict_local_move`, with a command
built from one action. It writes `UserCmd` directly; key bindings and console
verbs are not involved. Fields it does not set (gun angle offset, remote
control, location selection) stay as the client built them.

Per command:

* **Step length.** The predictor runs the command with
  `msec = cg time − command_time` of the prediction world's local player,
  clamped to 200. The adapter computes the same value from the same state
  before the predictor runs, and uses it for the rates and the inverse. A
  command with `msec = 0` does not run in the sim; the adapter repeats the
  previous command then and keeps the action.
* **Rotation.** The rates are clamped to the limits and integrated onto the
  previous command's angles (`angles_after_rate`). The pitch is kept inside
  the sim's ±85° view clamp, so the sim never rewrites `delta_angles`.
  `LookState.angles` follows the command, as the client's own input does.
* **Intended velocity.** The target `(fwd, right)` is turned into the world
  frame with the yaw the command produces, then `MoveFrame::command_for`
  picks `forwardmove/rightmove` from the adapter's own intended velocity.
  That velocity is stepped after the predictor ran the command, with the same
  code, state and step length as the recorder (`IntendedTracker`), so the
  two never drift apart.
* **The state the inverse conditions on.** The intended velocity is defined
  with the player state *after* the command, which does not exist until the
  sim ran it. The adapter uses the state before the command with the view
  angles of the new command applied. Where the command itself changes the
  state that scales movement — a sprint starts or ends, the stance or its
  transition changes, the damage timer runs out — the chosen command is
  off for that one command. The next command starts from the true state again.
* **Sprint desired.** Holds `SPRINT` while desired and not sprinting. While
  sprinting it keeps the previous bit, because a fresh press cancels a
  sprint. An unwanted sprint is cancelled by release, then press. After a
  cancel the sim wants the button up once, so the adapter releases it first.
* **Stance desired.** `PRONE` / `CROUCH` bits as state.
* **Buttons.** Held as given. A button in `retrigger` starts a new activation:
  if the previous command held it, it is released for one command and pressed
  on the next. Frag and smoke get the loadout's offhand index.
* **Weapon.** 0 keeps the client's selection; otherwise the adapter selects it
  in the command and in `WeaponSelect`.

When the source runs out, the client's own input drives again.

The action source is a trait (`ActionSource`: `current`, `advance`,
`cursor`). `ActionFile` replays a CSV; a model behind a socket or shared memory
implements the same trait.

### Action file

One row per command the sim runs, header by name, missing columns are 0:

| column | meaning |
|---|---|
| `src_frame` | where the row came from (not used by the adapter) |
| `fwd`, `right` | intended velocity, u/s, player's yaw frame |
| `pitch_rate`, `yaw_rate` | deg/s; positive pitch looks down, positive yaw turns left |
| `sprint` | 0/1, sprint desired |
| `stance` | `stand` / `crouch` / `prone` |
| `attack`, `ads`, `reload`, `jump`, `use`, `melee`, `frag`, `smoke`, `breath`, `throw` | 0/1 held |
| `retrigger` | `UserCmd.buttons` bits that start a new activation |
| `weapon` | weapon index, 0 keeps the client's selection |

`rfm_actions <movement.csv> <actions.csv>` makes one from a recording.

### Drive log

`IW4L_RFM_DRIVE_LOG` writes one row per command the adapter built and the sim
ran, keyed by `frame` like the recording: the action (`target_fwd`,
`target_right`, clamped `pitch_rate`/`yaw_rate`, `sprint`, `stance`,
`action_buttons`), the rate the command angles actually carry
(`pitch_rate_cmd`, `yaw_rate_cmd`), the step lengths planned and run, whether
the command run equals the one built (`as_built`), the command's fields, the
`pm_flags` before it and the inverse's own predicted miss (`guess_error`).

## Tools

* `cargo run --release -p rfm_io --bin rfm_roundtrip -- <movement.csv> <out.csv> [yaw_limit] [pitch_limit]`
  runs the round trip; it is described in the binary's header.
* `cargo run --release -p rfm_io --bin rfm_actions -- <movement.csv> <actions.csv>`
  makes an action file from a recording.
* `cargo run --release -p rfm_io --bin rfm_drive_check -- <movement.csv> <drive_log.csv> [actions.csv] [out.csv]`
  checks a driven run: the recorder against what the adapter injected.
* `fork/tools/plot_movement.py <movement.csv>`: time series, distributions,
  frame timing.
* `fork/tools/normalization_study.py <roundtrip.csv>`: normalisation schemes
  per action dimension.

Both scripts use the `context/venv` virtualenv (matplotlib, numpy, pandas).

## Columns of `movement.csv`

* **Frame and time:** `frame`, `perf_frame`, `real_s`, `cg_time_ms`,
  `presented_tick`.
* **Command:** `cmd_new`, `server_time_ms`, `cmd_msec`, `buttons`,
  `forwardmove`, `rightmove`, `cmd_pitch`, `cmd_yaw` (65536 units per turn),
  `cmd_weapon`.
* **Rates:** `pitch_rate_dps`, `yaw_rate_dps`.
* **Player state after the command:** `ps_valid`, `pm_type`, `pm_flags`,
  `on_ground`, `origin_*`, `vel_*`, `view_pitch`, `view_yaw`, `delta_pitch`,
  `delta_yaw`, `speed`, `move_scale_mult`, `weapon`, `pm_time`,
  `damage_timer`, `leanf`, `weapon_pos_frac`, `view_height_target`,
  `view_height_lerp_time`, `view_height_lerp_target`, `view_height_lerp_down`,
  `melee_charge_time`.
* **Weapon:** `weapon_move_scale`, `weapon_ads_move_scale`.
* **Intended velocity:** `wish_speed`, `full_speed` (wish speed of full
  forward), `intended_vx`, `intended_vy` (world), `intended_fwd`,
  `intended_right` (yaw frame), `actual_fwd`, `actual_right`.
