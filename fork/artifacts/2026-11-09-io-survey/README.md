---
status: active
updated: 2026-11-09
branch: rfm-io
next: user — review the interface (intended velocity · yaw rate · absolute pitch · modes/buttons; observation incl. heading sin/cos). Open: model input size, HUD; velocity search at short steps
---

# Survey of the action, vision and sound paths

Where player input enters the game and where video and sound leave it, and the seams where a recorder or a model could attach.

Current slice: [11-ORIENTATION-READY](11-ORIENTATION-READY.md) — yaw as rate, pitch absolute, heading (sin, cos); cmd_vel framing and the choice of inversion through the setpoint.

Earlier: [10-MENU-CHECK-FINAL](10-MENU-CHECK-FINAL.md) (pause menu, console, chat cut), [9-MENU-CUT-TIME-REPLAY-READY](9-MENU-CUT-TIME-REPLAY-READY.md) (input-capture cut, replay by game time), [8-IMAGES-IN-DATASET-READY](8-IMAGES-IN-DATASET-READY.md) (1080p session, frame-aligned images, zero-time fix), [7-LEROBOT-DESIGN-READY](7-LEROBOT-DESIGN-READY.md) (raw sessions → converter → LeRobot v3), [6-INFERENCE-ADAPTER-READY](6-INFERENCE-ADAPTER-READY.md) (`IW4L_RFM_DRIVE`: recorder sees the injected command 100 %, intended velocity bit-exact wherever reachable), [5-BOT-MATCH-RECORDING-READY](5-BOT-MATCH-RECORDING-READY.md) (online = offline on human play; sprint button ≠ sprint state), [4-ROUNDTRIP-NORMALIZATION-READY](4-ROUNDTRIP-NORMALIZATION-READY.md) (round trip, normalisation), [3-MOVEMENT-RECORDING-READY](3-MOVEMENT-RECORDING-READY.md) (the recorder and the first recording), [2-ACTION-SPACE-READY](2-ACTION-SPACE-READY.md) (how the sim moves a player), [1-IO-SURVEY-READY](1-IO-SURVEY-READY.md) (input/video/sound seams).
