# Setting up IW4L from our fork (agent runbook)

Read this file top to bottom and run the steps on a Linux machine. The fork is
`grubertj-tng/iw4L`; `master` is upstream `vladtrc/iw4L` plus our own work. How
agents work in this repository is in `AGENTS.md`.

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

`context/` is machine-local and starts empty on a new machine; the task
journals in `fork/artifacts/` come with the clone.

## Traps

* The console command `move` stalls the whole script if its target is blocked
  or the yaw is outside -180…180. Append `&` to it in `--cmds`.
* For screenshot or test runs, point `IW4L_SETTINGS_PATH`,
  `IW4L_ACCOUNT_PATH` and `IW4L_PROFILE_PATH` at scratch files. Otherwise
  commands like `thirdperson 1` are saved into `~/.config/iw4l/settings.cfg`.
* Every machine in a multiplayer match must run the same build.
* Pull upstream changes with `git fetch upstream && git merge upstream/master`
  on `master`; never push to `upstream`.
