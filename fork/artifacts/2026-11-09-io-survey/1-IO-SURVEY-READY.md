# 1 — Survey of the action, vision and sound paths

Read-only survey of the code at `dab22d6` (upstream `acc57a5` plus fork docs).
Nothing was built, run or changed. Every claim comes from reading code;
claims marked *(inferred)* were reasoned rather than read and are unverified.
Line numbers drift; the function and type names are the stable reference.

## The loop as it exists today

```text
devices ─► publish_client_action_input ─► KbuttonSet ─► sample_client_input/build_usercmd
 (Bevy PreUpdate, per render frame)        (binds)       ─► ClientCmdTemplate.cmd          ◄── seam A1
                                                          ─► predict_local_move ─► PendingClientSends ◄── seam A2
                                                          ─► send_pending_commands (≤60/s)
                                                               │ listen host: direct push
                                                               ▼
bots: think_bots ─► cmds.push ─────────────────────────► ClientCommandInbox           ◄── seam A3
                                                          ─► gather_authority_input ─► TickInput
                                                          ─► sim::try_step (20 Hz, 50 ms)
                                                          ─► Snapshot ─► PresentedSnapshot (tick)
                                                               │
             window ◄── Bevy render (+ UI camera) ◄────────────┘                         ◄── seams V*
             speakers ◄── audio render_core::render_for (48 kHz, 128-frame blocks)        ◄── seam S1
```

## Action (INPUT)

**What a command is.** `UserCmd` (`crates/playerstate_iw4/src/usercmd.rs`):
`server_time` (ms), `buttons` (bitmask: attack, sprint, use, reload, prone,
crouch, jump, ADS, frag, smoke, change-zoom, …), `angles: [i32;3]` (absolute,
65536/360 units, relative to `ps.delta_angles`), `forwardmove`/`rightmove`
(i8, ±127), `weapon`, `weapon_mapped`, `off_hand_index`, `gun_angle_offset`,
melee charge, killstreak map cursor, remote-control axes.

**Rates.** One `UserCmd` per *render frame* on the client
(`sample_client_input` in `Update`, `ClientSet::Input`). The authority steps
at 20 Hz (`AUTHORITY_HZ`, `crates/net/src/authority/inbox.rs`) and consumes
every command whose `server_time` falls in the tick — at 60 fps about 2–3 per
human per tick *(inferred)*. Bots produce one command per tick.

**Device path.** `publish_client_action_input` (`crates/console/src/plugin/mod.rs`)
reads keys, mouse buttons, mouse motion, wheel and gamepad sticks once per
frame; `KeyBinds` map them to command ids (`+attack`, …); `input_iw4::key_event`
→ `KbuttonSet`; `build_usercmd` (`crates/net/src/client/input.rs`) turns
buttons, axes and mouse (× sensitivity × FOV scale × 0.022) into a `UserCmd`;
`sample_client_input` then patches weapon select, offhand, location cursor,
melee charge and gun offset into `ClientCmdTemplate`.

**Not in `UserCmd`.** `ClientAction`s travel separately: action slots /
killstreak activation, menu responses, class select, chat. Menus, console and
chat read input on their own paths.

**Console verbs.** `hold/press/release/+x/-x` and `mousemove` go through the
*same* Kbutton → `build_usercmd` path as real devices, but are skipped while a
menu/console/chat has focus and while the window is unfocused. `look`, `move`,
`tp` are cheat teleports that bypass the builder.

**Seams.**

| seam | where | gives | misses / cost |
|---|---|---|---|
| A1 | `ClientCmdTemplate.cmd`, a new system after `sample_client_input`, before `predict_local_move` | read **or replace** the local player's command; prediction and networking stay consistent | `ClientAction`s; replacing must also update `LookState` or angles drift. New system only |
| A2 | `PendingClientSends` after Predict (`(CmdSeq, UserCmd, …)`) | exactly the commands that were simulated and sent, with `server_time` | read-only. Lowest effort; the best "what the human did" log |
| A3 | `ClientCommandInbox` on the host, bot-style `push(id, None, cmd, None)` | a model-driven seat at 20 Hz that leaves every human path alone | that seat has **no rendered view** — vision would need a second client process |
| A4 | console verbs over stdin (`IW4L_CONSOLE_STDIN=1`) | drive keys/mouse with **zero code change**, today | coarse, frame-rate dependent, focus/menu gated; no record of what landed |

## Vision (OUTPUT)

**No continuous capture exists.** `screenshot` / `IW4L_SCREENSHOT`
(`crates/render/src/diag/capture.rs`) spawn Bevy's `Screenshot::primary_window()`:
the whole window (world, HUD, Bevy UI), async GPU readback, a fresh texture and
buffer per shot, 30-frame settle, PNG encode. Built for single shots.

**Frame end.** The world camera (`Camera3d`) draws world, post-FX and IW HUD;
a `Camera2d` (order 200) draws Bevy UI (reticle, names, menus, console). In
windowed mode both go straight to the window. Fullscreen below native height
renders the world into an offscreen `Rgba8UnormSrgb` image and stretches it
(`crates/render/src/resolution.rs`) — an existing offscreen path, world + HUD
without Bevy UI *(inferred)*.

**No window, no picture.** `serve` (dedicated) has no window and renders
nothing; there is no headless rendering mode.

**Which game time a frame shows.** Rendering runs at display rate (vsync by
default, no frame cap); the sim at 20 Hz. Remote entities are interpolated
between snapshots; the local player is **predicted**, so a frame is a mix of
states, not one tick. `PresentedSnapshot::tick()` / `interpolation_pair()`
exist on the main world, but no tick or game time reaches the render world or
the pixels. Pipelined rendering (default) puts the render one frame behind.

**Pacing trap.** An unfocused window updates only at 60 Hz (Bevy
`WinitSettings::game()`); scripted mouse input is dropped when unfocused.

**Seams.**

| seam | gives | cost / invasiveness |
|---|---|---|
| V1 | per-frame `Screenshot::primary_window()` from a new system after Present, tagged with presented tick and client time when queued | full window incl. UI; new texture+blit per frame (~3.7 MB at 720p); new file, no game change |
| V2 | Bevy `Readback` on a fixed-size offscreen image (extend the `resolution.rs` path to windowed) | fixed resolution, buffer pool, world + HUD only; small change to `resolution.rs` |
| V3 | a render-world node copying the view target into a staging ring, tick written at extract | most precise, cheapest at runtime; most invasive (`render_gpu`) |
| V4 | re-render a recorded demo offline (see Replay) | not bound to real time; not usable for inference |

## Sound (OUTPUT)

`RenderShared::render_for` (`crates/audio/src/render_core.rs`) produces each
final 48 kHz stereo f32 block of 128 frames and advances the audio clock
`frame`. CPAL, the null fallback and the offline renderer share it. Nothing
records it. Nothing links audio frames to sim ticks or video frames.
`IW4L_SOUND=off` also silences cue producers, so a "silent" run is not a
capture source.

| seam | gives | cost |
|---|---|---|
| S1 | tap after the limiter in `render_for`: lock-free SPSC push + writer thread | exact sample index per block; ~10 lines in one real-time-critical file |
| S2 | a Bevy system logging `(tick, render frame, audio_frame(), Instant)` | the join between audio and the rest; additive |
| S3 | PipeWire/Pulse monitor | no code; wall time only, includes device latency |

## Recording, replay and control

* **Demos store output, not input.** `.iw4ldemo` (`crates/replay/src/file.rs`)
  is one snapshot delta per tick; `acks` and `UserCmd`s are not stored; no
  wall-clock time. Playback interpolates snapshots without re-simulating and
  without prediction — it shows authoritative state, not what the player saw.
  Actions cannot be recovered from a demo beyond view angles and state.
* **Re-rendering is possible.** Demo playback has pause, rate 0.05–16× and
  single-step; stepping lands frames on tick boundaries. Pixel-identical re-runs
  are unverified.
* **Time is real time.** No live pause, step, timescale or lockstep of render to
  sim; `Time<Virtual>` speed is overwritten each frame by the GSC slow-motion
  plan. A model that is slower than real time sees a moving world.
* **Control channels.** `--cmds` / `IW4L_CMDS` at launch; `IW4L_CONSOLE_STDIN=1`
  for a live one-way console. No socket, RPC or shared memory into the process.
* **Seats.** The listen-host human is `ClientId(0)`. Several instances run side
  by side with separate `IW4L_ARTIFACTS_DIR` / settings / account paths
  (`master_duo_chaos`). Remote joins go through `iw4l-master`.
* **Batch tooling to reuse.** approved_tests launch the binary with `--cmds`
  and parse stdout marks; xtask reads Perfetto `player_tick` rows
  (`IW4L_PERF=1`), which carry per-tick buttons, origin and angles for every
  client.

## What this means for the interfaces

1. **The action space is a choice.** `UserCmd` is game-native, compact and
   exactly what the sim consumes (A1/A2). Raw device input (keys, mouse deltas)
   is closer to what a human does, and is what a model would emit if it plays
   "like a person". Recording both at A2 + device level is cheap.
2. **For a VLA that sees its own screen, inference belongs on the local player**
   (A1 replace + V*), not on a bot seat (A3 has no picture).
3. **No common clock exists.** The interfaces need one record per render frame
   joining: frame index, wall time, client time, presented tick, `CmdSeq` of the
   command sent, audio frame. This ledger is also what makes the interfaces
   verifiable — every sample can be traced to the command and tick around it.
4. **Most of it fits in one new fork-only crate** (a Bevy plugin with systems
   ordered against the existing `ClientSet`s, gated by env), plus a one-line
   plugin registration. The known exceptions: S1 (~10 lines in `render_core.rs`)
   and possibly V2 (`resolution.rs`).
5. **Fastest path to a first look, no code:** stdin console verbs for actions,
   OS-level screen and audio capture, Perfetto `player_tick` for state.
   Unsynchronised and lossy, but enough to look at.

## Open decisions for the user

* Action space: `UserCmd`, raw devices, or both recorded.
* What the model sees: full window (menus, console, UI) or world + HUD; resolution.
* Rates: capture every frame or tick-aligned (20 Hz); model control rate.
* Real time only, or allow pausing/stepping the sim for slow models (touches game code).
* Live capture, or record live + re-render from demo for video.
* Sound now or later.
* Transport to the model: separate process (socket / shared memory) or in-process.
* Dataset format (e.g. LeRobot, RLDS) — can wait.

## What was not done

* Nothing was run. No claim here was checked against a live process; in
  particular the fullscreen offscreen image contents, the per-frame screenshot
  cost, deterministic demo re-render and headless GPU needs are unverified.
* Spot-checked directly: `UserCmd` fields, `AUTHORITY_HZ = 20`,
  `ClientCmdTemplate`, `PendingClientSends`, `IW4L_CONSOLE_STDIN`, the
  screenshot spawn, the end of `render_for`, empty `acks` in demo frames, and
  the names of the input functions. The rest comes from four read-only sub-surveys.
* Gamepad, menu and chat paths were only located, not traced.
* Scaffolding: none. The slice added no code or probes to the tree.

Next: the user's decisions above. Then an agent can prototype the frame ledger
and seam A2 (read-only) as the first verifiable piece.
