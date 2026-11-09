# 9 — Episodes end at menus; the adapter replays by game time

Decisions by the user:
* episodes **end** when a menu is open (play after it starts a new episode);
* the action-file replay is a test tool, so make it **time-based**;
* storage local only; HUD decided later.

## Menus, console, chat: logged, and the converter cuts there

**What the recorder logs.** Five new columns after `presented_tick`:
* `input_captured`: OR of the four below;
* `console_or_chat`: `frame::HudInputView::console_open`, which the console
  plugin sets from the console and from chat;
* `script_menu`: `hud::MenuState` → `ScriptMenus::captures_input`;
* `ui_overlay`: `frame::UiOverlayInput`;
* `window_focused`: `frame::InputDevices::focused`.

No upstream edits. `HudInputView::script_menu_open` exists but nothing writes
it, so it is not used.

**What the converter does.** `fork/tools/to_lerobot.py` treats
`input_captured = 1` like death: the episode ends, and the next stretch of
play starts a new one. Not a pause: the match keeps running behind a menu, so
splicing would put a jump inside an episode. `--min-seconds` (default 1)
drops the fragments. Recordings without the column (play-1 … play-3) are
converted as before, with a note.

**Evidence so far.** On scripted runs the window is never focused, so
`input_captured` is 1 throughout, as defined. `script_menu` fired on 6 frames
(class choice).

**Not verified yet:** which flag the in-game pause/quit menu sets. It needs a
focused human session: open the menu for a few seconds, close it, quit.

## Time-based replay

**`ActionSource`** lost `advance`. `current(&DriveTick)` answers with the
action valid at the **start of the command's step** (`server_time - msec`).

**`ActionFile`:**
* new required column `t_ms` (game ms, non-decreasing);
* replay time starts at the first command driven;
* the source ends after the last row's duration (taken as the previous gap).

**`rfm_actions`:**
* writes `t_ms` = the recorded step start;
* recomputes rates through `RateTracker`, so zero-time commands are carried
  into the next command;
* writes an idle row at the start of each stretch without control (death).

**Evidence.** `play-3/actions.csv` (15,033 rows, 261 s), driven for 61 s:

| run | frame rate | commands | row = row valid at step start | game time / file time | \|yaw\| turn game vs file | sprint game vs file |
|---|---|---|---|---|---|---|
| `adapter-time-1` | 60 Hz (unfocused throttle) | 3,670 (17 ms) | 100 % | 61.3 s / 61.3 s | 1,436° / 1,458° | 2.0 s / 2.0 s |
| `adapter-time-2` | uncapped (`IW4L_BENCH=1`) | 12,270 (5 ms) | 100 % | 61.3 s / 61.3 s | 1,460° / 1,458° | 2.0 s / 2.0 s |

Before this change, a per-command replay at 5 ms per command would have run
the same rows 3.4× faster in game time.

**Turn error.** A command takes the rate of the row at its step start. A
command that straddles a row boundary uses the earlier row for its whole
step, so 17 ms commands drift a little (−1.5 % here) and 5 ms commands almost
nothing (+0.1 %). If exact totals matter, the adapter can integrate the rate
schedule across the boundary instead. Not done.

## What was not done

* Which flag the pause/quit menu sets (needs the user, see above).
* Exact integration of the rate schedule across row boundaries.
* `rfm_drive_check` was not re-run on the time-based runs. The check above
  covers row selection and totals, not the per-command field comparison.
* Scaffolding: none. The analysis was an inline script.
