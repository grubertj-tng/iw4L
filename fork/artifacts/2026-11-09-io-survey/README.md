---
status: active
updated: 2026-11-09
branch: rfm-io-adapter
next: user — decide per-command vs time-based replay and the model I/O for the next action source; video keyed by FrameIndex is in the parallel video clone
---

# Survey of the action, vision and sound paths

Where player input enters the game and where video and sound leave it, and the seams where a recorder or a model could attach.

Current slice: [6-INFERENCE-ADAPTER-READY](6-INFERENCE-ADAPTER-READY.md) — `IW4L_RFM_DRIVE` drives the player from actions; the recorder sees the injected command 100 %, intended velocity bit-exact wherever reachable.

Earlier: [5-BOT-MATCH-RECORDING-READY](5-BOT-MATCH-RECORDING-READY.md) (online = offline on human play; sprint button ≠ sprint state), [4-ROUNDTRIP-NORMALIZATION-READY](4-ROUNDTRIP-NORMALIZATION-READY.md) (round trip, normalisation), [3-MOVEMENT-RECORDING-READY](3-MOVEMENT-RECORDING-READY.md) (the recorder and the first recording), [2-ACTION-SPACE-READY](2-ACTION-SPACE-READY.md) (how the sim moves a player), [1-IO-SURVEY-READY](1-IO-SURVEY-READY.md) (input/video/sound seams).
