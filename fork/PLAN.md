# Plan

The living map of the fork's work: what is in flight, where, and whose move it
is. It describes the present; how we got here is in the journals under
[`artifacts/`](artifacts/) and in git. Agents and the user both update it.

Why we do any of this: [`GOALS.md`](GOALS.md).

## In flight

| task | status | branch | journal | whose move | next |
|---|---|---|---|---|---|
| Action and vision interfaces (survey → recorder → inference adapter → video → LeRobot) | active | `rfm-io` | [2026-11-09-io-survey](artifacts/2026-11-09-io-survey/README.md) | user | review the action/observation interface (slice 11); open: model input size, HUD |
| Video capture keyed by `FrameIndex` | active | `rfm-io` | [2026-11-09-video-capture](artifacts/2026-11-09-video-capture/README.md) | user | watch a recording; decide capture size and frame cap for dataset runs |

`status`: planned | active | blocked | done | dropped.
`whose move`: agent | user, matching the suffix of the journal's newest
iteration (`-PART` → agent, `-READY` → user, `-FINAL` → nobody).

## Parked

| task | branch | note |
|---|---|---|
| Unitree G1 robot as a player body | `g1-robot` | An experiment, not a goal. Its docs are `docs/ROBOT.md` on the branch; the setup steps (URDF fetch, `IW4L_ROBOT_*`) are in `SETUP-FORK.md` at commit `02af3d7`. |
