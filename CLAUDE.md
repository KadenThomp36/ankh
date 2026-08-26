# ankh — working notes for agents

Read `CONTEXT.md` (vocabulary) and `docs/adr/` (decisions) before changing
anything non-trivial.

## Layout

- `crates/ankh-core` — the only crate that imports `anki` (rslib). Domain API:
  `Engine`, `Paths`, `AuthStore`, `DeckTree`, sync types. Keep rslib types
  from leaking out of this crate.
- `crates/ankh-render` — HTML → `Document` (blocks/spans with a CSS subset,
  ruby, images) → width-aware wrapped `Line`s. Pure, heavily unit-tested; no
  terminal types. Snapshot-friendly.
- `ankh-core::markdown` / `notefile` / `notes` — note file format (ADR 0005).
  `markdown::html_to_md` must stay lossless-or-raw; add a test for every new
  HTML construct you decide to support.
- `crates/ankh` — the binary. `editor.rs` is the `$EDITOR` handoff. `cli/` (headless commands, `--format`), `tui/`
  (ratatui app: `app.rs` loop, `keys.rs` notation + trie, `theme.rs`,
  `banner.rs`, `views/`).

## Rules

- Every TUI action has a headless equivalent; every command supports
  `--format json|jsonl` and includes `schema_version`.
- Exit codes are semantic: see `Error::exit_code`.
- Never persist the AnkiWeb password. Never log credentials.
- rslib is pinned by tag in the workspace `Cargo.toml`; bump deliberately and
  re-run the sync smoke test against a real account.
- rslib's `progress` module is private — see `ProgressLink` in
  `engine.rs` for the workaround; don't try to name `ProgressState`.
- Sync policy: on launch, on quit, on demand. Never periodic. A full-sync
  conflict is always a prompt, except on a pristine (empty) collection where
  download is automatic.

## Testing against real data

`~/.local/share/ankh/dev` is a copy of the `default` profile with **no
keyring entry**, so `ankh --profile dev` can never sync test answers to
AnkiWeb. Recreate it with `rm -rf ~/.local/share/ankh/dev && cp -r
~/.local/share/ankh/default ~/.local/share/ankh/dev`. Never answer cards in
the `default` profile from tests.

## Commands

```sh
cargo build                     # ~1 min cold (rslib), seconds warm
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
ANKH_LOG=debug cargo run        # logs to $XDG_STATE_HOME/ankh/<profile>/ankh.log
```

Manual TUI testing: drive it through a pty (`python3 -c 'import pty…'`) with
`TERM=xterm-256color`; see the smoke test in `docs/testing.md`.

## Commits

Conventional commits (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`), one
milestone-sized commit per logical step, straight to `main`.
