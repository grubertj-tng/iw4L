# 5 — Bot-match recording: full-state verification on human play

Recording `context/recordings/play-2/` (local to this machine): the user,
keyboard and mouse, mp_boneyard free-for-all against 5 bots
(`--cmds 'wait world; bot add 5'`), with the current recorder (full movement
state, slice-4 definition).
* 300 s in total, of which 249 s alive.
* 15,244 commands, 14,884 of them while alive.
* 12 respawns.

## Round trip

`target/release/rfm_roundtrip` on this recording:

* **Offline forward = in-game recorder: 100 % bit-exact on all 14,884
  commands.** This closes the gap left open in slice 4 (sprint and damage were
  unverified). Coverage:

  | state | commands |
  |---|---|
  | sprinting | 4,456 |
  | jumping | 1,657 |
  | ADS | 3,768 |
  | damage slow-down (`damage_timer > 0`) | 780 |
  | crouch | 13 |

  Damage shows as `full_speed` values of 180.5 and 193.1 u/s, against 203.3
  for a normal run.
* **Inverse round trip:** 100 % bit-exact velocities.
* **Reconstructed command = recorded command:** 99.75 %. The 37 others are
  no-op keys, as in slice 4.
* **Rotation:** 0 mismatches; re-anchored on 12 teleports (the respawns).
* **Limits ±1200 / ±180 deg/s:**
  * yaw: 2 commands clipped, 12.7° in total;
  * pitch: **11 commands clipped, 4.4° in total, max 237 deg/s**;
  * pitch p99.9 is 198 deg/s, so ±180 is a bit tight for fights.
* **7 commands have `msec = 0`.** Their rate is defined as 0. Angles still
  round-trip, because no angle change happened in them.

**Still not covered:** prone (one 0.07 % button blip, no prone state), hard
landing, shellshock, lean.

## Distributions (`normalization_study.py`)

The slice-4 conclusions hold on 3.5× the data:

* **Intended velocity:** bounded (max |v| 330 / 340 u/s), light tails (excess
  kurtosis −1.0 / −0.3). Fighting spreads it more than the first session did:
  * forward: top bin 18 % (24 % before), 57 effective bins (33 before);
  * right: 78 effective bins.
* **Yaw and pitch rate:**
  * exactly zero on 49 % / 61 % of commands;
  * linear excess kurtosis 31 / 40;
  * μ-law brings it to 0.9 / 0.8 at 60 Hz and 0.4 / 0.3 at 10 Hz;
  * yaw μ-law effective bins: 26 (60 Hz) → 43 (20 Hz) → 65 (10 Hz).
* **Buttons** (share of commands held):

  | ADS | attack | sprint | reload | jump | use | frag | prone |
  |---|---|---|---|---|---|---|---|
  | 25.5 % | 11.8 % | 6.0 % | 3.2 % | 1.4 % | 0.55 % | 0.11 % | 0.07 % |

## New finding: the sprint *button* is not the sprint *state*

* The sprint button is held on 6.0 % of commands, but the player is sprinting
  on 29 % (4,456 commands).
* Sprint latches: a short press starts it, and it lasts until forward is
  released, ADS or fire interrupts it, or stamina runs out (`sprint.rs`).
* So the raw button is a rare, edge-like event. A model trained on it has to
  learn to fire a short pulse at the right moment.

The alternative is **"sprint desired"** as the action:
* teleop label: the recorded `SPRINTING` state;
* inference: the adapter presses sprint when desired and not yet sprinting,
  and releases forward intent otherwise.

Stance (crouch, prone) toggles in a similar way. This is a decision for the
user.

## What was not done

* Prone, hard landing, shellshock and lean are still unverified.
* No full-sim re-simulation (see slice 4).
* Before the first spawn (t < 17 s) the recorder shows a non-zero actual
  velocity with no commands. It reads `presented.player(local)` while the
  player is not yet in the game. This is harmless for the dataset (no
  commands there) but not investigated.
* Scaffolding: none.

Next: the user decides
* pitch limit (±180 → about ±240?);
* normalisation and control rate (slice 4);
* sprint and stance as desired state vs raw button.

After that: the in-game inference adapter, or video capture keyed by
`FrameIndex`.
