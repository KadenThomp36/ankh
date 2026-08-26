# Testing

- Unit tests: `cargo test --workspace` (key notation, progress parsing, …).
- Sync smoke test (needs a real account):
  `ankh --profile test login && ankh --profile test sync && ankh --profile test decks`.
- TUI smoke test: spawn `ankh` in a pty with `TERM=xterm-256color`, send keys,
  strip ANSI, inspect the frame. `scripts/tui-smoke.py` does this.
