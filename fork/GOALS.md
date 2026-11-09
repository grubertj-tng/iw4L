# Goals

Owned by the user. Agents read this and change it only when asked.

> **Details are still open.** Formats, rates, synchronisation, recording and
> model I/O will be decided with the user. Do not settle them on your own; ask.

## Vision
LONG-TERM VISION:
* transferable VLA utilities for other Bevy-ported games

Infrastructure to train AI models to play the game. The focus is on robot
foundation models (RFMs), especially vision-language-action models (VLAs).

The basic structure is a loop around the game:

* **INPUT** — the controls sent by the player or by a model.
* **OUTPUT** — the game's video, and its sound.

The system has two modes of operation:

* **Data collection / teleop** — a human controls the game. INPUT actions are
  recorded in sync with the OUTPUT and collected into a dataset.
* **Inference** — a trained model controls the game. It gives INPUTs and makes
  its decisions from the OUTPUTs.

## Current priorities

1. **Action and vision interfaces.** The INPUT and OUTPUT sides that both
   modes are built on.

## Principles

* **Leave the game code as it is, as much as possible.** The interfaces attach
  to the game. They do not reshape it.
* **Interfaces must be understandable and verifiable.** It must be possible to
  see what an interface actually does and to check that it does it the way the
  user intends.

## Non-goals

<!-- Not stated yet. -->

## Constraints

* Never commit, upload or link game files.
* Nothing is sent to the upstream project.

## Done looks like

<!-- Not stated yet. -->
