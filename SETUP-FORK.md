# Setting up IW4L from our fork (agent runbook)

Read this file top to bottom and run the steps on a Linux machine. The fork is
`grubertj-tng/iw4L`; `master` here is plain IW4L, identical to upstream
`vladtrc/iw4L`. The Unitree G1 robot work lives on the `g1-robot` branch (last
section), not on `master`.

## 1. System packages

```bash
# Debian / Ubuntu
sudo apt install g++ pkg-config libx11-dev libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev make git
# Fedora: gcc-c++ pkgconf-pkg-config libX11-devel alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel
# Arch:   gcc pkgconf libx11 alsa-lib systemd wayland libxkbcommon libxcursor libxrandr libxi
```

A working Vulkan GPU driver is also required. See `docs/BUILD.md` for details.

## 2. Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

`rust-toolchain.toml` selects stable with rustfmt and clippy; nothing else to pick.

## 3. Repository

If you are reading this you have probably already cloned the fork. If not:

```bash
git clone git@github.com:grubertj-tng/iw4L.git     # or the https URL if there is no SSH key
cd iw4L
```

Then, once:

```bash
git remote add upstream https://github.com/vladtrc/iw4L.git
git remote set-url --push upstream DISABLED        # nothing may be pushed to the original project
git config user.name "Jakob Grubert"
git config user.email "jakob.grubert@tngtech.com"
```

## 4. Game data

You need your own copy of Call of Duty: Modern Warfare 2 (2009) multiplayer
data; IW4L ships none. Never commit, upload or link game files.

* Steam, Windows depot: `steamcmd +@sSteamCmdForcePlatformType windows +force_install_dir ~/Games/iw4 +login <user> +app_update 10190 +quit`
* Or copy an existing install. The folder must contain `main/` and `zone/`.

The games root is the folder that *contains* the game folder: with the
install above it is `~/Games`. If the game data is not on the machine, ask the
user for it. Do not look for it elsewhere.

## 5. Configure

```bash
cp .env.example .env
```

Edit `.env` and set `IW4L_GAMES` to the games root (absolute path). Leave the
`IW4L_DEPLOY_*` and `IW4L_RELEASE_KEY` lines alone; they are only for
deployments. `.env` is gitignored.

## 6. Build and run

```bash
make map mp_boneyard CMDS='wait world; spawn 0; force_match_start; bot add 3'
```

The first build compiles the whole workspace and takes several minutes. It
needs a display (`DISPLAY=:0` if the session has none). Caches, logs and
screenshots go to `iw4l-artifacts/`.

Headless smoke test that must exit 0:

```bash
make map mp_boneyard CMDS='wait world; finish_run'
```

It worked if `iw4l-artifacts/logs/latest.log` shows the map loaded and the
process ended with `process_exit … code=0`.

Optional checks: `make publish-check` and `cargo clippy --workspace --all-targets`.
The repo forbids unit tests in tracked files, so there is no `cargo test` step.

## 7. The G1 robot (branch `g1-robot`, optional)

It is not on `master`. To use it:

```bash
git checkout g1-robot
git clone --depth 1 --filter=blob:none --sparse https://github.com/unitreerobotics/unitree_ros.git context/externals/unitree_ros
git -C context/externals/unitree_ros sparse-checkout set robots/g1_description
```

Add to `.env` (absolute path):

```bash
IW4L_ROBOT_URDF=/…/iw4L/context/externals/unitree_ros/robots/g1_description/g1_29dof_rev_1_0.urdf
IW4L_ROBOT_BODY=all      # optional: all | allies | axis, those soldiers are robots
IW4L_ROBOT_PROP=off      # optional: no standing robot in front of the spawn
```

Run the same `make map` command. `latest.log` then has `urdf robot …` lines.
`docs/ROBOT.md` explains the rest. The meshes live under `context/`, which git
ignores, so every machine fetches them itself.

## Traps

* The console command `move` stalls the whole script if its target is blocked
  or the yaw is outside -180…180. Append `&` to it in `--cmds`.
* For screenshot or test runs, point `IW4L_SETTINGS_PATH`,
  `IW4L_ACCOUNT_PATH` and `IW4L_PROFILE_PATH` at scratch files. Otherwise
  commands like `thirdperson 1` are saved into `~/.config/iw4l/settings.cfg`.
* Every machine in a multiplayer match must run the same build and the same
  `IW4L_ROBOT_*` settings.
* Pull upstream changes with `git fetch upstream && git merge --ff-only upstream/master`
  on `master`; never push to `upstream`.
