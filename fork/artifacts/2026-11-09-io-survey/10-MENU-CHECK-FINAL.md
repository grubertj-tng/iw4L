# 10 — Menu check: the pause menu, console and chat are detected and cut

This closes the open item of slice 9. Recording `context/recordings/menu-check/`
(local): the user played focused at 1920×1080 against 2 bots. They opened the
Escape menu for about 7 s, then the console/chat, then quit.

## Flags over time (`movement.csv`)

| t | what | flag |
|---|---|---|
| 9.0–12.6 s | class choice at spawn | `script_menu` |
| 39.9–47.0 s | **Escape (pause) menu** | `script_menu` |
| 51.4–53.0 s | console / chat | `console_or_chat` |
| 53.6–53.9, 54.2–55.0, 67.7–68.2 s | short menu openings | `script_menu` |
| from 91.5 s | quitting | `script_menu` |

* `window_focused` was 1 throughout the play.
* `ui_overlay` never fired. It belongs to Theater and the main-menu browser.
* While a menu is open the client still sends commands, but empty
  (`forwardmove = 0`). That is why these frames must not be training data.

## Converter (`fork/tools/to_lerobot.py`)

`menu-check` → `context/datasets/menu-check-lerobot`: 4 episodes, 1,330
steps at 20 Hz, with images.

| episode | steps | time | captured frames inside |
|---|---|---|---|
| 0 | 544 | 12.7–39.8 s | 0 |
| 1 | 88 | 47.0–51.3 s | 0 |
| 2 | 253 | 55.0–67.5 s | 0 |
| 3 | 445 | 68.2–90.4 s | 0 |

* The fragments 53.0–53.6 s and 53.9–54.2 s were dropped (`--min-seconds 1`).
* Round trip on this recording: movement and rotation exact; 99.98 % of
  commands reconstructed exactly.

## What was not done

* Nothing open for this item.
* Scaffolding: none. The timeline was an inline script.
