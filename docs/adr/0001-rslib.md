# ADR 0001 — Use Anki's `rslib` as the engine

**Status:** accepted (2026-08-26)

## Context

Every existing Anki CLI either wraps AnkiConnect (needs the desktop app
running) or re-implements a scheduler and sync protocol. Both routes produce
tools that are subtly wrong: FSRS, learning steps, burying, sibling handling
and the sync protocol are large, versioned and undocumented.

## Decision

Depend on `anki` (the `rslib` crate) directly as a git dependency pinned to a
release tag (`26.08.1`). It is not published on crates.io and is marked
`publish = false`; cargo handles its workspace + submodules fine. Confine every
use of it to `ankh-core`.

A spike proved the path: login → shard redirect → full download → media sync
→ deck tree → scheduler answers → card render, ~50 s cold build.

## Consequences

- **License:** rslib is AGPL-3.0; ankh is therefore AGPL-3.0-or-later.
- **API stability:** none promised. Bumps are deliberate, one-crate changes,
  smoke-tested against a real AnkiWeb account.
- **Private modules:** `anki::progress` is private, so `ProgressState` cannot
  be named. `ProgressLink` in `engine.rs` captures it in type-inferred
  closures and parses its `Debug` output. Candidate for an upstream PR.
- **Endpoint persistence:** the first sync returns `new_endpoint`
  (`syncN.ankiweb.net`); it must be stored with the credentials or full sync
  fails with `400 missing original size`.
- Build needs `protoc`, a C compiler, and (Linux) `libdbus` for the keyring.
