# 4 — Round trip of the action interface, and how the actions normalise

User request (made while away from the keyboard): a round-trip check on the
recording, and an evaluation of how well the recorded action distributions
can be normalised.

## What changed in the code (branch `rfm-io`)

* **`action.rs` refactor.**
  * `MoveFrame` holds the state a command moved with and offers `advance`
    (forward), `command_for` (inverse), `wish_speed` and `full_speed`.
  * `angles_after_rate` inverts `view_rate`.
* **Definition of intended velocity refined.** Jump landings, hard landings and
  melee lunges are now excluded (`EXCLUDED_FLAGS`), together with ladders and
  mantles.
  * **What the previous slice got wrong:** its recorder let jump-landing
    friction leak into the intended velocity. That makes the movement action
    dip after every jump with unchanged keys. It also made offline replay
    depend on the jump timer `pm_time`, which the first recording did not log.
  * `play-1`'s `intended_*` columns use the old definition. The round trip
    therefore recomputes intended velocity offline and uses that as its
    reference.
* **The recorder logs the full movement state:** `pm_time`, `damage_timer`,
  `leanf`, `weapon_pos_frac`, view-height target and lerp,
  `melee_charge_time`, and `full_speed`.
* **New tools:**
  * `crates/rfm_io/src/bin/rfm_roundtrip.rs`;
  * `fork/tools/normalization_study.py`.
* **Definitions and CSV columns** are documented in `crates/rfm_io/README.md`.

## Round trip — evidence

Run with
`target/release/rfm_roundtrip <movement.csv> <roundtrip.csv>`.

| check | `play-1` (human, old recorder) | `scripted-3` (scripted, full state) |
|---|---|---|
| commands while alive | 5,491 | 631 |
| offline forward = in-game recorder | not comparable (old definition); before the first jump: 321 commands incl. 150 sprinting agree within 6×10⁻⁵ u/s | **100 % bit-exact** |
| inverse: intended → command → intended | **100 % bit-exact** | **100 % bit-exact** |
| reconstructed command = recorded command | 99.69 % (17 differ) | 100 % |
| rotation: rates → integrated angles = recorded | **0 mismatches** (1 teleport re-anchored) | 0 mismatches |
| limits ±1200 / ±180 deg/s | clips 2 yaw commands, 6.8° in total | nothing |

**The 17 differing commands are keys that had no effect.** The velocity along
the key's direction was already above that key's wish speed. Examples: still
holding W while decelerating from sprint to run, or strafing while turning
fast. Pressing or not pressing gives the identical velocity, so the
difference is invisible to the sim. The inverse prefers the previous command
on such ties; without that tie-break 73 commands differed.

**Since reconstructed commands give bit-identical intended velocities, and
the original commands in all but 17 no-op cases, re-simulating them through
the real sim would reproduce the recorded trajectory.** This is argued from
determinism; it was not run through the full sim with collision.

**Coverage.**
* `scripted-3` exercised walking, strafing, diagonals, turning, crouch (88
  commands), jumps (109), ADS (65) and a stance transition (49).
* It did not exercise sprint, prone, damage or hard landing. The scripted
  `press +breath_sprint` did not engage sprint.
* Sprint is covered by `play-1`, up to its first jump.

**Scripted runs.**
* They use a copy of the user's account (`context/scratch2/`). The user's own
  account and settings were not touched.
* `spawn 0` is still refused ("Unlocks at level 25") for the first custom
  class. The game spawned the player anyway (class selection fallback), so
  movement was recorded.

## Normalisation — evidence

Data: `play-1` only (the human recording), 5,491 commands while alive, about
70 s. Full tables: `context/recordings/play-1/normalization_table.csv`. Plot:
`context/recordings/play-1/normalization.png`.

Metrics per scheme:
* **share outside [−1, 1]** (clipped);
* **256-bin tokenisation** (RT-2/OpenVLA style): bins used, effective bins
  (exp entropy), share in the top bin, de-quantisation error in original units;
* **skew and excess kurtosis** after normalisation.

### Intended velocity (forward, right)

* **Multimodal, not heavy-tailed.**
  * Forward has plateaus at 0, about +203 (run), about +305 (sprint) and about
    −142 (back). The top bin holds about 24 %.
  * Excess kurtosis is −0.5 (forward) and +0.2 (right); skew −0.65 (forward).
  * Physically bounded: |v| ≤ 329 u/s observed. The sprint wish speed is 305;
    turning while moving can exceed it slightly.
* **No normalisation removes the plateaus.** They are what keyboard play
  contains, and they carry information: run vs sprint.
* **Min-max to the data range or to a fixed physical bound behaves best.**
  Nothing is clipped, and the de-quantisation error is ≤ 1.1 u/s p99 for a
  ±350-ish range.
  * Quantile q01/q99 clips 2 %. z-score is not bounded (49 % beyond ±1σ for
    forward).
  * Effective bins: forward ~33 of 256, right ~94.
* **Dividing by the current max speed is worse for forward.** Run and sprint
  plateaus collapse onto 1.0 (top bin 46 %). The deceleration after a sprint
  overshoots to 1.5 (14–19 % clipped). For right it is slightly better than
  min-max (top bin 17 %, effective 104).

### Yaw and pitch rate

* **Zero-inflated and heavy-tailed at the per-frame rate.**
  * Yaw is exactly 0 on 47 % of commands, pitch on 60 %. Yaw excess kurtosis
    22, pitch 19.
  * Linear scaling to ±1200/±180 puts 52–60 % in one bin and uses 13 (yaw) /
    6 (pitch) effective bins.
* **Rates are a lattice.** At one command per frame a rate is an integer
  number of mouse counts × a constant ÷ `msec`. This is the comb in the pitch
  histograms.
* **A companding transform fixes the tails.** μ-law (μ = 255) or asinh, each
  with the limit as the end of the range:
  * Yaw: skew 0.1, excess kurtosis 0.36; effective bins 28 at 60 Hz.
  * Errors: de-quantisation p50 0.1 deg/s, p99 7.4 deg/s for yaw. Fine
    resolution where aiming happens, coarse in flicks.
* **Lower control rates help the most.** Averaging the rate over the window is
  exact for total rotation (mean rate × window time = the same turn).
  * Yaw μ-law effective bins: 28 (60 Hz) → 45 (20 Hz) → 64 (10 Hz). Top bin:
    47 % → 39 % → 31 %.
  * Excess kurtosis falls to about 0 at 20 Hz and below.

### Buttons

Share of commands with the button held:

| attack | sprint | breath | ADS | jump | reload | frag |
|---|---|---|---|---|---|---|
| 9.7 % | 21.1 % | 21.1 % | 11.7 % | 5.9 % | 4.4 % | 0.09 % |

* Also present: `0x80000` (throw).
* Sprint and breath are always set together: one key, `+breath_sprint`.
  Decoding should merge them, or the dataset carries a duplicate.
* Rare buttons (frag 0.09 %) need class weighting or will be ignored.

### Recommendation (for the user to decide)

* **Velocity:** a linear map with a **fixed physical bound** (e.g. ±350 u/s →
  [−1, 1]). Keep it stable across datasets rather than data-quantile based.
  Accept the multimodality.
* **Rates:** **μ-law (or asinh) companding** with the limits ±1200 / ±180
  deg/s as the range ends.
* **Control rate:** a **lower control rate (20 Hz, or chunks)** with mean
  rates. Rotation stays exact and the zero-inflation drops.
* **Buttons:** binary; merge sprint/breath; weight the rare ones.

## What was not done

* **No full-sim re-simulation with collision.** "Same commands → same
  trajectory" rests on determinism and on bit-identical commands.
* **Full-state online = offline is verified on a scripted run only.** That run
  covers crouch, jumps, ADS and stance transitions; it does not cover sprint,
  prone, damage, hard landing or shellshock. A new human recording would close
  this.
* **Normalisation statistics come from one 70-second human session**, one
  player, one mouse sensitivity and one weapon. They indicate shape, not final
  constants.
* **The 20 Hz and 10 Hz figures are offline averages over consecutive
  commands.** They do not come from an adapter running at that rate.
* **The inverse is not running in the game.** The inference-side adapter
  (substituting `ClientCmdTemplate.cmd`) is not written.
* **Scaffolding:** none added to the runtime. The round-trip binary and the
  two Python scripts are kept on purpose as verification tools.

Next: the user decides the normalisation and control rate. Then:
* a new human recording with the full state, including sprint and prone, to
  close the coverage gap;
* the in-game inference adapter, or video capture keyed by `FrameIndex`.
