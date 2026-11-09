# 3 — First movement recording: intended velocity and view rates

Decisions taken by the user before this slice:
* Movement action is the **intended velocity**.
* Rotation action is **yaw and pitch rate in deg/s**, with limits to be set.
* Actions must be in exact sync with frames.

## What was built (branch `rfm-io`)

* New crate `crates/rfm_io`, a Bevy plugin that does nothing unless
  `IW4L_RFM_RECORD=<file.csv>` is set.
  * `action.rs` is pure logic:
    * `IntendedVelocity` runs the sim's own `friction`, `cmd_scale_walk` and
      `accelerate` from `movement_iw4` on a velocity of its own. That velocity
      lives on virtual flat ground: no collision, no slopes, no gravity, no
      ladders or mantles. Inputs are the player state after the command
      (stance, sprint, yaw) and the weapon's move scales.
    * `view_rate` is the angle difference between consecutive commands divided
      by the command's `msec`.
  * `record.rs` writes one CSV row per `Update` pass, in `ClientSet::Diag`
    (after Input → Predict → Send → Present). It reads `ClientPrediction`
    (`last_cmd`, `last_predict_msec`, `predicted_local`) and
    `PresentedSnapshot::tick`. A row is keyed by our own `FrameIndex`, which
    counts up in `First`, and also carries `perf::frames::open_index()`.
* Upstream edits:
  * `sim::SimWorld::scales_for` changed from `pub(crate)` to `pub`;
  * one plugin line in `bootstrap/src/plugins.rs`;
  * the workspace and bootstrap `Cargo.toml` entries.
* `fork/tools/plot_movement.py` prints summary numbers and writes
  `timeseries.png`, `distributions.png` and `sync.png`. Python comes from the
  `context/venv` virtualenv (uv: matplotlib, numpy, pandas).

## Evidence

Recording `context/recordings/play-1/` (local to this machine): the user
playing on mp_boneyard, keyboard and mouse, 167 s, 10,045 frames, about 70 s
alive (the first ~75 s are menu, class choice and spectating).

**Sync**
* Frame index: no gaps.
* Alive frames: 5,491, and **every one has exactly one new command**.
* Command `msec`: median 17. It equals the frame's wall delta to within
  0.29 ms (median).
* Frame time: median 16.66 ms (vsync, 60 Hz), p99 21.4 ms. There are spikes of
  78–218 ms during loading and menus.
* The presented tick advances linearly at 20 Hz.

**Speed plateaus**
* `speed` is 190 and `move_scale_mult` is 1.07 (a perk); the weapon move scale
  was 1.0.
* So running forward gives 203 u/s and sprinting gives 305 u/s. Both show as
  plateaus of `wish_speed`.

**Movement data**
* Raw `forwardmove`: 100 % of commands are −127, 0 or 127.
* Intended forward velocity has continuous values between the plateaus, but
  large peaks remain at 0, ≈203 and ≈305.
  * On the yaw-frame scatter the plateaus form an octagon (8 key directions ×
    speed classes).
  * Diagonal lines are the acceleration ramps between key states.
  * Turning while moving fills the space in between.
* Intended and actual velocity agree closely on the ground. They differ in the
  air and on collisions (actual dips to −280 u/s, intended does not).
* Intended forward velocity reaches 327 u/s, above the 305 u/s wish speed. This
  is the sim's own acceleration rule: it caps only the component along the wish
  direction, so turning while moving can raise the magnitude. It is real sim
  behaviour, not a recorder bug *(reasoned from `accelerate.rs`, not isolated
  in a probe)*.

**Rotation rates (non-zero commands)**

| | p50 | p90 | p99 | p99.9 | max |
|---|---|---|---|---|---|
| \|yaw rate\| deg/s | 47 | 260 | 649 | 1164 | 1487 |
| \|pitch rate\| deg/s | 8 | 27 | 58 | 99 | 132 |

## What was not done

* **No round trip yet.** The inverse (intended velocity → `forwardmove/rightmove`
  for inference) is not written. The claim "intended velocity can be tracked by
  the sim" is therefore unverified.
* **Shellshock** is assumed to affect movement. The sim's per-client shellshock
  parameters are private, so the recorder uses the `SHELLSHOCKED` flag alone.
* **Buttons** are recorded raw (`buttons` bitmask). They are not decoded into
  the discrete action vector yet.
* **Video is not captured yet.** For the frame/action pairing two things are
  still open:
  * Pipelined rendering draws main-world frame N during frame N+1.
  * The human produced command N after seeing frame N−1 (and display latency).
  The video capture must key its frames by the same `FrameIndex`.
* **One short session from one player** with one sensitivity setting. The rate
  percentiles are an indication, not a basis for final limits.
* **The scripted test (`--cmds`) never moved.** The scratch account had the
  class locked ("Unlocks at level 25"), and the retry with `unlock all` was
  stopped by the user. The recorder was verified on human play instead.
* **No tests.** The repository allows tests only in `crates/approved_tests`.
* Scaffolding: none in `crates/`. The plot script is a fork tool and is kept
  on purpose.

Next: the user reads the plots and sets the rate limits. Then:
* the inverse adapter and a round-trip check on this recording;
* video capture keyed by `FrameIndex`.
