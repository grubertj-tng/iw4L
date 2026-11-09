# 6 — Inference adapter: actions in physical units drive the local player

User request (made while away): build the in-game adapter that turns an
action in the interface's units into the local player's `UserCmd`, with a
replay source, and show with numbers that the recorder sees exactly what was
injected. Branch `rfm-io-adapter` (clone `context/mrs/adapter`).

## What was built

* `crates/rfm_io/src/drive.rs` — the adapter. `IW4L_RFM_DRIVE=<actions.csv>`
  enables it, `IW4L_RFM_DRIVE_LOG=<csv>` adds a per-command log.
  * `drive_command` runs in `ClientSet::Predict`, after `sample_client_input`
    and before `predict_local_move`, and overwrites `ClientCmdTemplate.cmd`;
    `commit_command` runs right after the predictor.
  * Step length: `cg time − command_time` of the prediction world's local
    player, clamped to 200 — the value `ClientPrediction::predict` computes.
  * Rotation: clamp to ±1200 / ±240 deg/s, `angles_after_rate` from the
    previous command's angles, pitch kept inside the sim's ±85° clamp,
    `LookState.angles` set to the command's angles.
  * Movement: target `(fwd, right)` → world with the yaw the command produces
    → `MoveFrame::command_for_bounded` on the adapter's own intended velocity.
  * The adapter's intended velocity is stepped after the predictor ran the
    command, with the recorder's code (`IntendedTracker`, now shared by both),
    the predicted state after the command and the predictor's `msec`.
  * Sprint desired, stance desired, buttons with `retrigger`, weapon and
    offhand index: rules in `crates/rfm_io/README.md`.
* `ActionSource` trait (`current(&DriveTick)`, `advance`, `cursor`) — the
  seam for a model. `ActionFile` replays a CSV, one row per command.
* `action.rs`: `IntendedTracker` (recorder and adapter), `moves`,
  `command_for_bounded` (a lower bound on `forwardmove`), and a grid search
  that runs only when the local search does not land exactly. `command_for`
  is `command_for_bounded(.., -127)`; the round trip on `play-2` is unchanged
  (100 % bit-exact forward and inverse, 99.75 % same command, as in slice 5).
* `record.rs` uses `IntendedTracker`; its output is unchanged.
* `table.rs`: reading `movement.csv` back (moved out of `rfm_roundtrip`).
* Tools: `rfm_actions` (recording → action file) and `rfm_drive_check`
  (recording + drive log + action file → verification).

## Evidence

Runs from the clone, `context/scratch-adapter` settings/account/profile
(machine-local), 60 s of play after `spawn 4; force_match_start`. Outputs in
`context/recordings/adapter-*` (machine-local): `movement.csv`,
`drive_log.csv`, `actions.csv`, `check.txt`, `check.csv`.

* **`adapter-1-v2`**: `play-2` replayed, 3,667 driven commands, `msec` p50 17.
* **`adapter-2-v2`**: synthetic file (`adapter-2/make_synthetic.py`): yaw
  2000 and pitch 400 deg/s (beyond the limits), pitch into the view clamp,
  run, sprint, sprint not desired while forward held, sprint again, ADS,
  crouch, prone, crouch, stand, jump with `retrigger` every command, strafe
  circle, back-left. 1,139 commands; this run had `msec` p50 4 (≈ 250 fps).

| check (`rfm_drive_check`) | play-2 replay | synthetic |
|---|---|---|
| command the predictor ran = command built | 100 % | 100 % |
| `msec` planned = run = recorded | 100 % | 100 % |
| logged action = action file row | 100 % | 100 % |
| intended velocity bit-exact where any command reaches the target | **100 % of 430** | **100 % of 456** |
| intended velocity bit-exact overall | 11.7 % | 40.0 % |
| miss, no `i8` command reaches the target (exhaustive search) | 86.0 %, p50 0.33 u/s | 53.9 % |
| miss, the inverse's search short of the exhaustive best | 1.5 %, ≤ 1.07 u/s | 5.7 %, ≤ 0.32 u/s |
| miss, state guess off (sprint/stance changed in this command) | 0.8 % (27 sprint, 2 stance) | 0.35 % |
| recorded rate = rate the command angles carry | 100 % | 100 % |
| that rate within ½ angle unit of the action's clamped rate | 100 % | 95 % — the other 57 are at the ±85° pitch clamp |
| stance state = desired | 100 % | 87.5 % (see below) |
| sprint state = desired | 93.3 % | 99.9 % |
| action buttons in the command | 100 % | 97.3 % — the 31 released jump commands of `retrigger` |

**Why most targets are unreachable.** The targets are the intended velocities
the original commands produced at the original step lengths. With another
`msec` the acceleration and friction steps differ, so during speed changes no
`i8` command lands exactly; plateaus (rest, full run, full sprint) are reached
bit-exactly. The check proves "unreachable" by an exhaustive search over all
255 × 255 commands under the recorded post-command state.

**The one-command lag.** The intended velocity is defined with the state after
the command; the adapter only has the state before it (with the new view
angles applied). It costs exactly the commands in which a sprint or stance
changes: 29 of 3,667 and 4 of 1,139. The tracker state is not affected — the
next command starts from the true state.

**Sprint (play-2 replay, 80 % while desired).** 243 of the 247 disagreeing
commands are four runs of ~61 commands. Each begins with the sim ending a
running sprint while it is still desired: the sprint's time budget ran out,
and the next sprint waits for the budget to recover. The human's sprints sat
at other times relative to stamina. The remaining disagreement is one command
of cancel latency. Synthetic: the sprint starts on the first desired command,
a sprint not desired while forward is held is cancelled on the second command
(release, then press), and a sprint desired with a target below sprint speed
is kept (`SPRINT_FORWARD_MIN`).

**What `SPRINT_FORWARD_MIN` fixed.** The first synthetic run (`adapter-2`)
asked for sprint with 190 u/s forward. The inverse chose `forwardmove` < 106
to hold 190, which ends a sprint; the adapter re-pressed, and the sprint
toggled every third command (`S..S..`). Now the inverse searches only
commands with `forwardmove ≥ 106` while sprint is desired.

**What the grid search fixed.** On the first `play-2` replay
(`adapter-1`, local search only) the inverse fell short of the exhaustive best
on 31 % of commands (p50 1.1, max 29 u/s), always on unreachable targets.
With the grid search: 1.5 %, max 1.07 u/s.

**Stance (synthetic, 87.5 %).** The adapter's stance bits equal the desired
stance on 100 % of commands. The sim held prone, then crouch, for ~1,800 ms
after entering prone (`JUMPING` with `pm_time` 1800 set on entering prone);
at 4 ms per command that spans the following crouch and stand segments.

**Class.** The action targets come from a class with `move_speed_scale` 1.07.
A first replay with class 0 (0.95, m4) capped run speed at 180.5 instead of
203.3 and missed every run plateau by 22.8 u/s (`adapter-1-m4`). Class 4
(`akimbo_uzi`) has the same 1.07. The scratch account got `unlock all`
(`account.dat.before-unlock` kept next to it).

## Running it

```bash
cargo run --release -p rfm_io --bin rfm_actions -- <play>/movement.csv <out>/actions.csv
IW4L_PRESENT_MODE=Immediate IW4L_RFM_RECORD=<out>/movement.csv \
IW4L_RFM_DRIVE=<out>/actions.csv IW4L_RFM_DRIVE_LOG=<out>/drive_log.csv \
  target/play/iw4l map mp_boneyard --cmds 'wait world; spawn 4; force_match_start; wait 61s; finish_run'
cargo run --release -p rfm_io --bin rfm_drive_check -- <out>/movement.csv <out>/drive_log.csv <out>/actions.csv <out>/check.csv
```

(plus the scratch `IW4L_SETTINGS_PATH/ACCOUNT_PATH/PROFILE_PATH`). With the
default vsync and the window behind another game, the game ran at ~1 fps;
`IW4L_PRESENT_MODE=Immediate` avoids that.

## Upstream files

None changed.

## What was not done

* **Replay is per command, not per time.** One row per command the sim runs.
  The command rate follows the frame rate (17 ms in one run, 4 ms in another),
  so a replay plays at another speed than the recording. Rates and velocities
  are in physical units and stay right; durations (how long a turn lasts, a
  key is held, a stance is kept) scale. A time-based source would hold each
  row for its recorded `msec`, at the risk of dropping one-command button
  releases.
* **Not exercised in game:** `weapon` (selection through `WeaponSelect`),
  frag/smoke offhand index, melee (the client's melee-charge target is not
  computed for an injected melee), `use`, reload, attack, `throw`. The bits are
  verified to reach the command; their effects in the sim were not checked.
* **No model source.** The socket/shared-memory source is not written; the
  seam is `ActionSource`.
* **Sprint/stance lag** is measured, not removed. Predicting the post-command
  sprint and stance flags would remove it; it costs ~1 % of commands.
* **Frozen controls** (pre-match freeze) are not special-cased: the adapter
  consumes actions while the sim ignores movement.
* **The adapter replaces all buttons.** Location selection (airstrike) and
  remote-control (predator) modes are not driven.
* Scaffolding: none in the tree. `rfm_actions` and `rfm_drive_check` are kept
  on purpose as tools; the synthetic generator lives in the recording folder.

Next (the user): decide whether the replay should be time-based, and which
model I/O (socket, shared memory) the next source uses. Then video keyed by
`FrameIndex`, which the parallel `video` clone is on.
