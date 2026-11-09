"""Plot a movement recording written by the game with IW4L_RFM_RECORD=<file.csv>.

usage: python fork/tools/plot_movement.py <recording.csv> [out_dir]

Writes PNGs next to the recording (or into out_dir) and prints summary numbers.
"""

import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd


def load(path: Path) -> pd.DataFrame:
    df = pd.read_csv(path)
    df = df[df["ps_valid"] == 1].copy()
    for col in ["pm_type", "pm_flags", "on_ground"]:
        df[col] = df[col].astype(int)
    df["t"] = df["real_s"] - df["real_s"].iloc[0]
    df["alive"] = df["pm_type"] < 8
    return df


def summary(df: pd.DataFrame) -> None:
    frames = len(df)
    new = df[df["cmd_new"] == 1]
    dt = df["real_s"].diff().dropna() * 1000.0
    print(f"frames with a player: {frames}")
    print(f"frames with a new command: {len(new)} ({100.0 * len(new) / max(frames, 1):.1f} %)")
    print(f"frame time ms: median {dt.median():.2f}  p99 {dt.quantile(0.99):.2f}  max {dt.max():.2f}")
    print(f"command msec: median {new['cmd_msec'].median():.0f}  min {new['cmd_msec'].min():.0f}  max {new['cmd_msec'].max():.0f}")
    gaps = (df["frame"].diff().dropna() != 1).sum()
    print(f"gaps in the frame index: {gaps}")
    moving = new[new["alive"]]
    for col in ["yaw_rate_dps", "pitch_rate_dps"]:
        a = moving[col].abs()
        turning = a[a > 0]
        if len(turning):
            q = turning.quantile([0.5, 0.9, 0.99, 0.999]).values
            print(f"|{col}| while turning: p50 {q[0]:.0f}  p90 {q[1]:.0f}  p99 {q[2]:.0f}  p99.9 {q[3]:.0f}  max {turning.max():.0f}")
    for col in ["forwardmove", "rightmove"]:
        vals = moving[col]
        share = vals.isin([-127, 0, 127]).mean() * 100.0
        print(f"{col}: {share:.1f} % of commands are -127, 0 or 127")
    for col in ["intended_fwd", "intended_right"]:
        vals = moving[col]
        top = vals.abs().max()
        plateau = (vals.abs() > 0.98 * top).mean() * 100.0 if top > 0 else 0.0
        still = (vals.abs() < 1.0).mean() * 100.0
        print(f"{col}: max {top:.1f} u/s, {still:.1f} % near zero, {plateau:.1f} % within 2 % of max")


def plot_timeseries(df: pd.DataFrame, out: Path) -> None:
    fig, ax = plt.subplots(5, 1, figsize=(14, 13), sharex=True)
    ax[0].step(df["t"], df["forwardmove"], where="post", label="forwardmove (key)")
    ax[0].step(df["t"], df["rightmove"], where="post", label="rightmove (key)")
    ax[0].set_ylabel("command [-127,127]")
    ax[0].legend(loc="upper right")

    ax[1].plot(df["t"], df["intended_fwd"], label="intended fwd")
    ax[1].plot(df["t"], df["actual_fwd"], label="actual fwd", alpha=0.6)
    ax[1].plot(df["t"], df["wish_speed"], label="wish speed", alpha=0.4, ls="--")
    ax[1].set_ylabel("u/s, forward")
    ax[1].legend(loc="upper right")

    ax[2].plot(df["t"], df["intended_right"], label="intended right")
    ax[2].plot(df["t"], df["actual_right"], label="actual right", alpha=0.6)
    ax[2].set_ylabel("u/s, right")
    ax[2].legend(loc="upper right")

    ax[3].plot(df["t"], df["yaw_rate_dps"], label="yaw rate")
    ax[3].plot(df["t"], df["pitch_rate_dps"], label="pitch rate", alpha=0.7)
    ax[3].set_ylabel("deg/s")
    ax[3].legend(loc="upper right")

    ax[4].step(df["t"], df["on_ground"], where="post", label="on ground")
    ax[4].step(df["t"], ((df["pm_flags"] & 0x4000) != 0).astype(int) + 1.2, where="post", label="sprinting")
    ax[4].step(df["t"], ((df["pm_flags"] & 0x3) != 0).astype(int) + 2.4, where="post", label="crouch/prone")
    ax[4].set_yticks([])
    ax[4].legend(loc="upper right")
    ax[4].set_xlabel("seconds")
    fig.tight_layout()
    fig.savefig(out / "timeseries.png", dpi=110)
    plt.close(fig)


def plot_distributions(df: pd.DataFrame, out: Path) -> None:
    moving = df[(df["cmd_new"] == 1) & df["alive"]]
    fig, ax = plt.subplots(2, 3, figsize=(16, 9))
    ax[0, 0].hist(moving["forwardmove"], bins=64)
    ax[0, 0].set_title("forwardmove (raw command)")
    ax[0, 1].hist(moving["intended_fwd"], bins=64)
    ax[0, 1].set_title("intended velocity, forward [u/s]")
    ax[0, 2].hist(moving["intended_right"], bins=64)
    ax[0, 2].set_title("intended velocity, right [u/s]")
    for a in ax[0]:
        a.set_yscale("log")

    for i, col in enumerate(["yaw_rate_dps", "pitch_rate_dps"]):
        vals = moving[col]
        vals = vals[vals != 0]
        if len(vals):
            lim = max(abs(vals.quantile(0.001)), abs(vals.quantile(0.999)))
            ax[1, i].hist(vals.clip(-lim, lim), bins=120)
        ax[1, i].set_yscale("log")
        ax[1, i].set_title(f"{col} (non-zero, clipped to p0.1–p99.9)")

    ax[1, 2].scatter(moving["intended_fwd"], moving["intended_right"], s=2, alpha=0.3)
    ax[1, 2].set_aspect("equal")
    ax[1, 2].set_title("intended velocity in the yaw frame [u/s]")
    ax[1, 2].set_xlabel("forward")
    ax[1, 2].set_ylabel("right")
    fig.tight_layout()
    fig.savefig(out / "distributions.png", dpi=110)
    plt.close(fig)


def plot_sync(df: pd.DataFrame, out: Path) -> None:
    fig, ax = plt.subplots(1, 3, figsize=(16, 4))
    dt = df["real_s"].diff() * 1000.0
    ax[0].plot(df["t"], dt)
    ax[0].set_title("frame time [ms]")
    new = df[df["cmd_new"] == 1]
    ax[1].hist(new["cmd_msec"], bins=range(0, 60))
    ax[1].set_title("command step msec")
    ax[2].plot(df["t"], df["presented_tick"] - df["presented_tick"].min())
    ax[2].set_title("presented sim tick (20 Hz)")
    fig.tight_layout()
    fig.savefig(out / "sync.png", dpi=110)
    plt.close(fig)


def main() -> None:
    path = Path(sys.argv[1])
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else path.parent
    out.mkdir(parents=True, exist_ok=True)
    df = load(path)
    summary(df)
    plot_timeseries(df, out)
    plot_distributions(df, out)
    plot_sync(df, out)
    print(f"plots written to {out}")


if __name__ == "__main__":
    main()
