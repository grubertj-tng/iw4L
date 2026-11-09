---
status: active
updated: 2026-11-09
branch: rfm-io
next: decided — sprint and stance are desired state; normalisation and control rate belong to the training side. Rate limits set: yaw ±1200, pitch ±240 deg/s. Then in-game adapter or video keyed by FrameIndex
---

# Survey of the action, vision and sound paths

Where player input enters the game and where video and sound leave it, and the seams where a recorder or a model could attach.

Current slice: [5-BOT-MATCH-RECORDING-READY](5-BOT-MATCH-RECORDING-READY.md) — 5 min against bots: online = offline bit-exact incl. sprint, jumps, ADS, damage; sprint button ≠ sprint state.

Earlier: [4-ROUNDTRIP-NORMALIZATION-READY](4-ROUNDTRIP-NORMALIZATION-READY.md) (round trip, normalisation), [3-MOVEMENT-RECORDING-READY](3-MOVEMENT-RECORDING-READY.md) (the recorder and the first recording), [2-ACTION-SPACE-READY](2-ACTION-SPACE-READY.md) (how the sim moves a player), [1-IO-SURVEY-READY](1-IO-SURVEY-READY.md) (input/video/sound seams).
