"""Convert a raw recording session into a LeRobot v3 dataset (prototype).

usage: python fork/tools/to_lerobot.py <session_dir> <out_dir> [--fps 20] [--repo-id local/iw4l]
       [--task "..."] [--no-video]

<session_dir> holds what the game wrote with IW4L_RFM_RECORD (movement.csv) and
what `rfm_roundtrip` wrote from it (roundtrip.csv: actions under the current
definition). With video, it also holds the capture (frames.csv + mp4, keyed by
FrameIndex) -- not wired yet: --no-video is required until video capture lands.

Runs with the lerobot venv: context/venv-lerobot/bin/python.

Sampling. LeRobot assumes a constant rate (timestamp = frame_index / fps), the
game does not. Step k of an episode is the last game frame at or before
t0 + k/fps (by wall time). Its action covers the commands from that frame up to
the next step's frame:
  * yaw/pitch rate: total turn over the step ÷ step time (exact: integrates to
    the same angle);
  * intended velocity, sprint, stance, held buttons: value at the end of the step
    (hold_breath only counts while aiming: the sprint key sets the same bit);
  * momentary buttons (jump, reload, melee, frag, smoke, use): pressed at any
    command in the step.
Episodes are lives: contiguous runs of commands while the player moves, split
at respawns (a change of delta_angles) and deaths.
"""

import argparse
import shutil
from pathlib import Path

import numpy as np
import pandas as pd

HELD = {"attack": 0x1, "ads": 0x800, "hold_breath": 0x2000}
MOMENTARY = {"jump": 0x400, "reload": 0x10, "melee": 0x4, "use": 0x8, "frag": 0x4000, "smoke": 0x8000}
PM_SPRINTING, PM_PRONE, PM_CROUCH = 0x4000, 0x1, 0x2

ACTION_NAMES = (
    ["intended_fwd_ups", "intended_right_ups", "yaw_rate_dps", "pitch_rate_dps", "sprint", "crouch", "prone"]
    + list(HELD)
    + list(MOMENTARY)
)
STATE_NAMES = [
    "vel_fwd_ups", "vel_right_ups", "vel_up_ups", "view_pitch_deg", "on_ground",
    "sprinting", "crouched", "prone", "ads_frac", "max_speed_ups",
]


def load(session: Path) -> pd.DataFrame:
    mv = pd.read_csv(session / "movement.csv")
    rt = pd.read_csv(session / "roundtrip.csv")
    cmds = mv[(mv.cmd_new == 1) & (mv.ps_valid == 1)].copy()
    cmds = cmds.merge(
        rt[["frame", "moving", "intended_fwd", "intended_right", "yaw_rate", "pitch_rate", "full_speed"]],
        on="frame",
        suffixes=("_rec", ""),
    )
    for col in ["pm_flags", "pm_type", "buttons", "on_ground"]:
        cmds[col] = cmds[col].astype(int)
    return cmds.reset_index(drop=True)


def episodes(cmds: pd.DataFrame):
    alive = cmds.moving == 1
    respawn = (cmds.delta_yaw != cmds.delta_yaw.shift()) | (cmds.delta_pitch != cmds.delta_pitch.shift())
    breaks = (~alive) | respawn
    seg = breaks.cumsum()
    for _, part in cmds[alive].groupby(seg[alive]):
        if len(part) > 1:
            yield part.reset_index(drop=True)


def resample(ep: pd.DataFrame, fps: int):
    t = ep.real_s.to_numpy()
    grid = np.arange(t[0], t[-1], 1.0 / fps)
    idx = np.searchsorted(t, grid, side="right") - 1
    idx = np.unique(idx)
    steps = []
    for a, b in zip(idx[:-1], idx[1:]):
        window = ep.iloc[a + 1 : b + 1]
        end = ep.iloc[b]
        seconds = window.cmd_msec.sum() / 1000.0
        yaw = (window.yaw_rate * window.cmd_msec / 1000.0).sum() / seconds if seconds > 0 else 0.0
        pitch = (window.pitch_rate * window.cmd_msec / 1000.0).sum() / seconds if seconds > 0 else 0.0
        flags = int(end.pm_flags)
        action = [
            end.intended_fwd, end.intended_right, yaw, pitch,
            float(flags & PM_SPRINTING != 0), float(flags & PM_CROUCH != 0), float(flags & PM_PRONE != 0),
        ]
        held = int(end.buttons)
        if held & HELD["ads"] == 0:
            held &= ~HELD["hold_breath"]
        action += [float(held & bit != 0) for bit in HELD.values()]
        action += [float((window.buttons & bit).any()) for bit in MOMENTARY.values()]
        obs = ep.iloc[a]
        state = [
            obs.actual_fwd, obs.actual_right, obs.vel_z, obs.view_pitch, float(obs.on_ground),
            float(int(obs.pm_flags) & PM_SPRINTING != 0), float(int(obs.pm_flags) & PM_CROUCH != 0),
            float(int(obs.pm_flags) & PM_PRONE != 0), obs.weapon_pos_frac, obs.full_speed,
        ]
        steps.append((int(obs.frame), np.asarray(state, np.float32), np.asarray(action, np.float32)))
    return steps


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("session", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--fps", type=int, default=20)
    ap.add_argument("--repo-id", default="local/iw4l")
    ap.add_argument("--task", default="Play free-for-all against bots on mp_boneyard.")
    ap.add_argument("--no-video", action="store_true")
    args = ap.parse_args()
    if not args.no_video:
        raise SystemExit("video capture is not wired into the converter yet; pass --no-video")

    from lerobot.datasets import LeRobotDataset

    features = {
        "observation.state": {"dtype": "float32", "shape": (len(STATE_NAMES),), "names": STATE_NAMES},
        "action": {"dtype": "float32", "shape": (len(ACTION_NAMES),), "names": ACTION_NAMES},
        "game_frame": {"dtype": "int64", "shape": (1,), "names": None},
    }
    if args.out.exists():
        shutil.rmtree(args.out)
    ds = LeRobotDataset.create(
        repo_id=args.repo_id, fps=args.fps, features=features, root=args.out,
        robot_type="iw4l_player", use_videos=False,
    )
    cmds = load(args.session)
    n_eps = n_steps = 0
    for ep in episodes(cmds):
        steps = resample(ep, args.fps)
        if len(steps) < 2:
            continue
        for frame, state, action in steps:
            ds.add_frame({
                "observation.state": state,
                "action": action,
                "game_frame": np.asarray([frame], np.int64),
                "task": args.task,
            })
        ds.save_episode()
        n_eps += 1
        n_steps += len(steps)
    ds.finalize()
    print(f"episodes {n_eps}, steps {n_steps} at {args.fps} Hz -> {args.out}")

    check = LeRobotDataset(args.repo_id, root=args.out)
    print("reloaded:", check.num_episodes, "episodes,", check.num_frames, "frames; features:", list(check.features))
    sample = check[0]
    print("sample 0:", {k: (v.tolist() if hasattr(v, "tolist") else v) for k, v in sample.items()})
    stats = check.meta.stats["action"]
    for i, name in enumerate(ACTION_NAMES):
        print(f"  {name:20s} min {stats['min'][i]:9.2f}  max {stats['max'][i]:9.2f}  mean {stats['mean'][i]:8.3f}  std {stats['std'][i]:8.3f}")


if __name__ == "__main__":
    main()
