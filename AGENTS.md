# Agent guide for this fork

This repository is `grubertj-tng/iw4L`, a fork of
[vladtrc/iw4L](https://github.com/vladtrc/iw4L). The runtime and upstream's
working rules are inherited; this file says what is different here. Where the
two disagree, this file wins.

## Read first

1. [`fork/GOALS.md`](fork/GOALS.md) — what we are trying to achieve. Owned by
   the user: change it only when asked. If it has no goals written yet, ask the
   user before picking work.
2. [`fork/PLAN.md`](fork/PLAN.md) — the work in flight, on which branch, and
   whose move it is.
3. Upstream's contract — `AGENT.md` and `CONTEXT.md`, imported at the end of
   this file.
4. [`docs/INDEX.md`](docs/INDEX.md) — the map of the runtime; open the file for
   the area you touch.
5. [`SETUP-FORK.md`](SETUP-FORK.md) — only if this machine is not set up yet.

## What differs from upstream

* **Nothing goes upstream.** The `upstream` remote is fetch-only. The "Making a
  public push" pass in `CONTEXT.md` (squash to one commit, strip, `make
  publish-check`) is upstream's release ritual and is not required here.
  Ordinary commits with messages that state the effect, pushed to `origin`.
* **Task journals are tracked**, in `fork/artifacts/` instead of
  `context/artifacts/`, so work can continue on another machine. The format is
  the one `CONTEXT.md` describes: one folder per task, the README header,
  iterations named `<N>-<NAME>-<FINAL|PART|READY>`, and the journal is not
  rewritten. Keep it to text: large logs, traces, dumps and anything derived
  from game data stay in `context/` and are referenced by path, noting that
  they exist on one machine only.
* **`fork/PLAN.md` is the index of tasks.** When you open, hand over or close a
  task, update its row there as well as the artifact README.
* `context/` keeps its other roles: `externals/`, the `mrs/` agent clones and
  bulky evidence. It is still never committed.

## What still holds

* Upstream's code rules: no retail addresses or decompiler placeholder names,
  comments only where the code cannot show a hazard, IW4L-domain names (see
  "Comments" and "Names" in `CONTEXT.md`). Our code sits next to upstream's and
  should read the same.
* Tests only in `crates/approved_tests`; probes and debug leftovers come out
  before a slice lands ("Before shipping" in `CONTEXT.md`).
* Parallel agents use `make mr new <name>` / `make mr ship <name>`. Another
  agent's `context/mrs/*` is not touched without being asked.
* Never commit, upload or link game files.
* For code changes: rustfmt the touched files (`make mr fmt FILES='…'`, not
  `cargo fmt --all`) and `cargo clippy --workspace --all-targets`.

## Branches and merging upstream

* `master` is upstream plus our work. A larger experiment gets its own branch
  and a row in `fork/PLAN.md`.
* Prefer new files over edits to upstream files: every edited upstream file is
  a potential conflict on the next sync. Fork material lives in `fork/`, this
  file, `CLAUDE.md` and `SETUP-FORK.md`.
* Sync: `git fetch upstream && git merge upstream/master` on `master`.

## Upstream's contract

@AGENT.md

@CONTEXT.md
