"""How well can the recorded actions be normalised?

usage: python fork/tools/normalization_study.py <roundtrip.csv> [out_dir]

Reads the per-command output of `rfm_roundtrip` (intended velocity under the
current definition, rates, buttons) and compares normalisation schemes for
each continuous action dimension, at the recorded control rate and averaged
down to 20 Hz and 10 Hz.
"""

import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

BUTTONS = {
    "attack": 0x1, "sprint": 0x2, "melee": 0x4, "use": 0x8, "reload": 0x10,
    "use_reload": 0x20, "prone": 0x100, "crouch": 0x200, "jump": 0x400,
    "ads": 0x800, "stance_held": 0x1000, "breath": 0x2000, "frag": 0x4000,
    "smoke": 0x8000, "change_zoom": 1 << 26,
}
LIMITS = {"yaw_rate": 1200.0, "pitch_rate": 240.0}  # YAW/PITCH_RATE_LIMIT_DPS in crates/rfm_io/src/action.rs
BINS = 256


def kurtosis(x):
    x = np.asarray(x, float)
    s = x.std()
    return float(((x - x.mean()) ** 4).mean() / s**4 - 3.0) if s > 0 else float("nan")


def skew(x):
    x = np.asarray(x, float)
    s = x.std()
    return float(((x - x.mean()) ** 3).mean() / s**3) if s > 0 else float("nan")


def schemes(name, x, full_speed):
    """Each scheme: (forward map to roughly [-1,1], inverse map back)."""
    q01, q99 = np.quantile(x, 0.01), np.quantile(x, 0.99)
    mu, sd = x.mean(), x.std()
    lo, hi = x.min(), x.max()
    out = {
        "min-max (data range)": (
            lambda v: 2 * (v - lo) / (hi - lo) - 1,
            lambda y: (y + 1) / 2 * (hi - lo) + lo,
        ),
        "z-score": (lambda v: (v - mu) / sd, lambda y: y * sd + mu),
        "quantile q01/q99": (
            lambda v: 2 * (v - q01) / (q99 - q01) - 1,
            lambda y: (y + 1) / 2 * (q99 - q01) + q01,
        ),
    }
    if name in LIMITS:
        lim = LIMITS[name]
        out["fixed limit ±%g" % lim] = (lambda v: v / lim, lambda y: y * lim)
        scale = max(np.quantile(np.abs(x[x != 0]), 0.5), 1.0)
        norm = np.arcsinh(lim / scale)
        out["asinh (s=p50), limit ±%g" % lim] = (
            lambda v: np.arcsinh(v / scale) / norm,
            lambda y: np.sinh(y * norm) * scale,
        )
        mu_law = 255.0
        out["mu-law 255, limit ±%g" % lim] = (
            lambda v: np.sign(v) * np.log1p(mu_law * np.minimum(np.abs(v) / lim, 1)) / np.log1p(mu_law),
            lambda y: np.sign(y) * ((1 + mu_law) ** np.abs(y) - 1) / mu_law * lim,
        )
    else:
        ceiling = np.quantile(full_speed[full_speed > 0], 0.99) * 1.5
        out["fixed ±%.0f u/s" % ceiling] = (lambda v: v / ceiling, lambda y: y * ceiling)
        fs = np.where(full_speed > 0, full_speed, 1.0)
        out["÷ current max speed"] = (lambda v: v / fs, lambda y: y * fs)
    return out


def evaluate(x, fwd, inv):
    y = fwd(x)
    clipped = float(np.mean(np.abs(y) > 1.0))
    yc = np.clip(y, -1, 1)
    idx = np.clip(np.floor((yc + 1) / 2 * BINS), 0, BINS - 1).astype(int)
    centres = (idx + 0.5) / BINS * 2 - 1
    recon = inv(centres)
    err = np.abs(recon - x)
    counts = np.bincount(idx, minlength=BINS)
    p = counts[counts > 0] / counts.sum()
    return {
        "clipped %": 100 * clipped,
        "bins used": int((counts > 0).sum()),
        "eff. bins": float(np.exp(-(p * np.log(p)).sum())),
        "top bin %": 100 * counts.max() / counts.sum(),
        "q-err p50": float(np.median(err)),
        "q-err p99": float(np.quantile(err, 0.99)),
        "skew": skew(y),
        "ex.kurt": kurtosis(y),
    }


def windowed(df, n):
    if n == 1:
        return df
    g = np.arange(len(df)) // n
    return df.groupby(g).mean(numeric_only=True)


def main():
    path = Path(sys.argv[1])
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else path.parent
    df = pd.read_csv(path)
    df = df[df["moving"] == 1].reset_index(drop=True)
    df["buttons"] = df["buttons"].astype(int)
    print(f"commands while alive: {len(df)}  (~{df['cmd_msec'].median():.0f} ms each)\n")

    print("buttons held (share of commands):")
    for name, bit in BUTTONS.items():
        share = ((df["buttons"] & bit) != 0).mean() * 100
        if share > 0:
            print(f"  {name:12s} {share:6.2f} %")
    other = df["buttons"] & ~sum(BUTTONS.values())
    if (other != 0).any():
        print(f"  other bits present: {sorted(set(int(b) for b in other.unique()) - {0})}")
    print()

    dims = {
        "intended_fwd": "intended velocity forward [u/s]",
        "intended_right": "intended velocity right [u/s]",
        "yaw_rate": "yaw rate [deg/s]",
        "pitch_rate": "pitch rate [deg/s]",
    }
    rates = {"60 Hz (per command)": 1, "20 Hz (mean of 3)": 3, "10 Hz (mean of 6)": 6}
    rows = []
    for rate_name, n in rates.items():
        w = windowed(df, n)
        for dim in dims:
            x = w[dim].to_numpy(float)
            for scheme, (fwd, inv) in schemes(dim, x, w["full_speed"].to_numpy(float)).items():
                r = evaluate(x, fwd, inv)
                rows.append({"rate": rate_name, "action": dim, "scheme": scheme, **r})
    table = pd.DataFrame(rows)
    pd.set_option("display.width", 200)
    pd.set_option("display.max_rows", 200)
    for rate_name in rates:
        print(f"=== {rate_name} ===")
        t = table[table.rate == rate_name].drop(columns="rate")
        print(t.to_string(index=False, float_format=lambda v: f"{v:.3g}"))
        print()
    table.to_csv(out / "normalization_table.csv", index=False)

    for dim in ["yaw_rate", "pitch_rate", "intended_fwd", "intended_right"]:
        x = df[dim]
        print(f"{dim}: exactly zero {100 * (x == 0).mean():.1f} %,  "
              f"|x| p50/p90/p99/max = {np.quantile(np.abs(x), [0.5, 0.9, 0.99]).round(1).tolist()} / {np.abs(x).max():.1f}")

    fig, ax = plt.subplots(4, 4, figsize=(20, 14))
    for row, dim in enumerate(dims):
        x = df[dim].to_numpy(float)
        sch = schemes(dim, x, df["full_speed"].to_numpy(float))
        picks = list(sch.items())[-4:] if dim in LIMITS else [list(sch.items())[i] for i in (0, 1, 2, 4)]
        for col, (scheme, (fwd, _)) in enumerate(picks):
            y = np.clip(fwd(x), -1.5, 1.5)
            ax[row, col].hist(y, bins=128, range=(-1.5, 1.5))
            ax[row, col].axvline(-1, color="k", lw=0.5)
            ax[row, col].axvline(1, color="k", lw=0.5)
            ax[row, col].set_yscale("log")
            ax[row, col].set_title(f"{dim}: {scheme}", fontsize=9)
    fig.tight_layout()
    fig.savefig(out / "normalization.png", dpi=100)
    plt.close(fig)
    print(f"\nwritten: {out / 'normalization_table.csv'}, {out / 'normalization.png'}")


if __name__ == "__main__":
    main()
