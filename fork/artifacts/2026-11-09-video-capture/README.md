---
status: active
updated: 2026-11-09
branch: rfm-io-video
next: user — watch a recording, decide on capture size/downscaling and frame-rate cap for dataset runs; then the LeRobot export
---

# Per-frame video capture keyed by FrameIndex

`IW4L_RFM_VIDEO=<dir>` writes an H.264 mp4 of every main-world frame plus `frames.csv` mapping each video frame to the recorder's `FrameIndex`; attribution measured at offset 0.

Current slice: [1-VIDEO-CAPTURE-READY](1-VIDEO-CAPTURE-READY/README.md).
