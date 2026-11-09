# 2 — How the sim turns a command into movement and rotation

Question from the user: instead of WASD and mouse deltas, can actions be
sim-level quantities — a movement speed instead of keys, a rotational speed
instead of mouse movement? How does the sim handle these?

Read-only, at `dab22d6`. Read directly in code (not delegated):
`crates/movement_iw4/src/{single,viewangles,cmdscale,walk,air,accelerate,friction,sprint,stance}.rs`,
`crates/sim/src/step.rs` (per-command loop, `PmoveSingleContext` values),
`crates/input_iw4/src/lib.rs` (`sample_move`, `axis_to_move`, `create_cmd`),
`crates/bots/src/motor.rs` (`drive`). Nothing was run.

## Where the sim consumes a command

`sim::step` loops over `TickInput.cmds` and runs `movement_iw4::pmove` **once
per `UserCmd`** (`step.rs`, loop over `input.cmds` → `pmove(ps, &mut cmd, …)`).
The step length is `msec = cmd.server_time − ps.command_time`, clamped to
1..200 ms (`single.rs`, `clamped_msec`). So the sim integrates per command,
not per 20 Hz tick: a human at 60 fps produces ~60 pmoves per second, a bot 20.

Order inside one pmove (`single.rs`): view angles → ADS intent → sprint →
stance → view height → ADS fraction → breath → (mantle | ladder | melee charge
| air move | walk move) → ground trace → footsteps.

## Rotation

`update_view_angles` (`viewangles.rs`):

```text
viewangles[axis] = wrap( cmd.angles[axis] · (360/65536) + ps.delta_angles[axis] )
pitch clamped to [−85°, +85°]  (the clamp is stored by rewriting delta_angles[0])
```

* **The sim has no notion of rotational speed.** A command carries an
  **absolute** orientation; the sim just sets it. Rotation speed exists only
  as the difference between consecutive commands divided by their `msec`.
* Mouse sensitivity, the FOV/zoom scale and `m_yaw/m_pitch = 0.022` are applied
  on the client before the command is built (`create_cmd`, client
  `build_usercmd`). They do not exist at sim level.
* `delta_angles` is an offset the server owns. It changes on spawn, teleport,
  script `setPlayerAngles`, spectating, the mantle yaw cap (`mantle.rs`) and
  the pitch clamp. A controller that writes absolute angles must rebase on
  `ps.delta_angles` (bots do: `angles = (pitch − delta) · ANGLE2SHORT`).
* Roll (`angles[2]`) is 0 for players.
* Resolution: 1 unit = 0.0055°.

**Consequence.** "Yaw rate, pitch rate" is a clean, lossless action: an adapter
integrates `rate × msec` onto the current view angle every command. It is
independent of the human's sensitivity and zoom, and independent of the model's
control rate. A 10 Hz model's rate is spread over every rendered frame, so the
rotation is smooth; absolute angles from a 10 Hz model would jump. Bots already
work this way: `motor.rs` slews yaw and pitch with their own velocities
(`slew_yaw`, `yaw_vel`) and writes the integrated result as absolute angles.

## Translation

**Command.** `forwardmove`, `rightmove: i8 ∈ [−127, 127]`. This is already a
continuous axis, and WASD is just a coarse source of it:

* Keys give ±127, except that a key pressed for part of a frame contributes that
  fraction (`sample_move` → `movement_key_state`).
* Gamepad sticks give analog values (`axis_to_move`).
* Bots compute it from a world-frame direction (`wish_to_move`).

**Wish direction.** The horizontal (yaw-only) forward and right vectors are
weighted by `forwardmove` and `rightmove` (`walk.rs`, `air.rs`; `forward[2] = 0`,
so pitch does not tilt movement).

**Wish speed** (`cmdscale.rs`, `cmd_scale_walk`), in units/s:

```text
f' = forwardmove · (0.7 if backward else 1)       r' = rightmove · 0.8
wishspeed = speed · max(|f'|, |r'|) / 127
          × 0.4    walking (lean / slow-walk)
          × 1.5    sprinting
          × stance (stand 1, crouch 0.65, prone 0.15, blended over 400 ms)
          × weapon move scale  or  weapon ADS move scale   (per weapon)
          × 0.4    shellshocked
          × damage-timer scale × ps.move_speed_scale_multiplier (perks/script)
speed = ps.speed = 190 by default (world.rs)
```

So the deepest *commanded* quantity is a **target velocity in the player's yaw
frame**. Its direction comes from (forwardmove, rightmove). Its magnitude is
the fraction max(|f'|,|r'|)/127 of a ceiling that depends on state. Diagonals
are not faster: the magnitude is the larger component, not the vector length.

**What actually moves the player** is `ps.velocity`. It is state, not command:

```text
on ground: friction    v ← v · (1 − 5.5 · max(|v|,100)/|v| · dt)    (×0.3 hard landing)
           accelerate  toward wishdir·wishspeed, at most accel·wishspeed·dt per step
                       accel: stand 9, crouch 12, prone 19, slippery 1
           gravity on slopes/slick, clip to ground plane, step-slide collision
in air:    friction (none airborne), accelerate with accel 1.0, gravity, slide
```

On flat ground at stand, velocity reaches the target in roughly 1/9 s
*(inferred from the constants; not measured)*. In the air the command barely
steers. Ladders, mantles and melee charge override all of this.

**Consequence.** Two "movement speed" actions are possible:

| action | what it is | invertible to a `UserCmd`? |
|---|---|---|
| **move intent** `(fwd, right) ∈ [−1,1]²`, fraction of current max speed, yaw frame | exactly what the sim consumes, without the i8 quantisation | trivially: `forwardmove = round(127·fwd)` |
| **target velocity** in units/s, yaw frame | intent × the state ceiling above | yes, when below the ceiling: divide by `cmd_scale_walk(ps, …)`; above it, it saturates |
| **actual velocity** `ps.velocity` | the outcome of acceleration, friction, collision, air | no; it needs a closed-loop controller and fails in the air. Belongs in the **observation** (proprioception), not the action |

Normalised intent is the best action. The same output means the same thing
whatever stance, weapon or perk applies; the model does not have to know a
weapon's move scale. Target velocity can be logged next to it for analysis.

## Discrete actions

These stay discrete. They are buttons whose *edges* matter, because the sim
compares them with `old_buttons`.

* **Hold-state:** attack, ADS, sprint, crouch, prone, hold breath, use.
* **Edge-triggered:** jump (needs a release between jumps), reload, melee,
  frag, smoke, stance changes. An adapter must emit a release before
  re-pressing.
* **Sprint has preconditions:** `forwardmove > 105` (≈ 83 % forward intent), and
  it is blocked by ADS, fire, reload, use and the stamina timer (`sprint.rs`). The
  sim drops an impossible sprint silently. So "sprint" is a request, and the
  observation should report whether it happened (`pm_flags::SPRINTING`).
* **Weapon** is a categorical index (`cmd.weapon`), switched by changing it.
  Better exposed to a model as "primary / secondary / …" than as a raw index.
* **Not in `UserCmd`:** killstreak activation, class choice, menus and chat go
  as `ClientAction`s.

## Proposed action vector (for discussion)

```text
continuous  move_fwd, move_right      ∈ [−1, 1]    → forwardmove/rightmove
            yaw_rate, pitch_rate      deg/s        → integrated onto view angles per command
binary      attack, ads, sprint, jump, crouch, prone, reload, melee, use, frag, smoke
categorical weapon slot
```

Matching **proprioception** for the observation side, all from `PlayerState`:
`velocity` (world or yaw frame), `viewangles`, stance, ADS fraction,
sprinting, on-ground, ammo.

An adapter does the work in both directions:

* **Inference:** model output at its own rate → a `UserCmd` per rendered frame
  (seam A1). The adapter holds the intent, integrates the rates, and
  generates button edges.
* **Teleop recording:** `UserCmd` stream (seam A2) + `PlayerState` → the same
  vector. `fwd = forwardmove/127`; rates come from consecutive view angles over
  `msec`, with `delta_angles` jumps (teleports, spawns) flagged rather than
  turned into huge rates.

Both directions use the same code. A round-trip test is then possible:
recorded human commands → action vector → commands → replay the sim → same
trajectory. That test is how the user can check the interface does what it
claims.

## Things the user should know

* **Keyboard teleop gives near-binary movement data.** Intent will be almost
  always 0 or ±1. A gamepad gives continuous intent. Mouse gives continuous
  rotation either way.
* **Human rotation rates are heavy-tailed.** Flicks reach thousands of deg/s.
  The model's rate range (and any clipping) is a decision.
* **Command rate matters to the sim only slightly.** `msec` clamps to 1..200
  and pmove is per command, so 20 Hz and 60 Hz commands give similar
  trajectories *(inferred; not measured)*.

## What was not done

* Nothing was run. The time to reach target speed, and whether 20 Hz and 60 Hz
  command streams give similar trajectories, are inferred from the constants
  and not measured.
* Not traced: jump internals (`jump.rs`), stance latching details, ADS
  transitions (`ads_frac.rs`), mantle and ladder control, the weapon state
  machine (fire, reload timing), where `move_speed_scale_multiplier` is set.
* Scaffolding: none. No code was added.

Next: the user decides the action vector (the proposal above or a variant).
Then an agent can write the adapter as a pure, testable module and verify it
by round trip on recorded commands.
