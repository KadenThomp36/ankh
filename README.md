<h1 align="center">ankh</h1>
<p align="center"><em>A neovim-flavoured Anki client for the terminal.</em></p>

```
                  _      _
   __ _   _ __   | | __ | |__
  / _` | | '_ \  | |/ / | '_ \
 | (_| | | | | | |   <  | | | |
  \__,_| |_| |_| |_|\_\ |_| |_|
```

`ankh` runs Anki's **real** engine — the same Rust library (`rslib`) that powers
Anki desktop — so scheduling (v3 + FSRS), the collection format, and AnkiWeb
sync are exactly what Anki does, not a reimplementation. On top of that sits a
modal, keyboard-driven interface that feels like neovim, and a headless CLI
that makes every action scriptable.

> Unofficial. Not affiliated with or endorsed by Ankitects / Anki.

## Status

Early. Milestones, in order:

- [x] **0 · spike** — rslib as a dependency; login, full sync, media, scheduler
- [x] **1 · skeleton** — CLI (`login`, `sync`, `decks`, `status`, `--format json`), TUI deck tree, keymaps, which-key, sync overlay
- [ ] **2 · review** — the study screen
- [ ] **3 · rendering** — Unicode/CJK, `<ruby>`, images (Kitty/sixel), audio
- [ ] **4 · browser** — Anki search syntax, previews, bulk ops
- [ ] **5 · editor** — notes as Markdown in `$EDITOR`, batch add, git-able deck export
- [ ] **6 · Lua** — `init.lua`, keymaps, events, plugins
- [ ] **7 · management** — deck options/FSRS, stats, import/export
- [ ] **8 · polish** — `:help`, themes, release binaries

## Install

Until there are release binaries:

```sh
# needs: rust (stable), protoc, pkg-config, libdbus (linux)
cargo install --git https://github.com/KadenThomp36/ankh ankh
```

## Use

```sh
ankh login          # AnkiWeb credentials → sync key in your OS keyring
ankh sync           # incremental; a brand-new profile downloads everything
ankh                # the TUI
ankh decks --format json | jq '.data[] | select(.review > 0) | .name'
```

Inside the TUI: `j`/`k` move, `h`/`l` fold, `S` sync, `<Space>` opens
which-key, `:` for the command line, `?` for the full keymap, `q` syncs and quits.

## Design

- One engine, two faces. `ankh-core` wraps `rslib`; the CLI and TUI are both
  thin clients of it. Nothing is TUI-only.
- Credentials: the password is exchanged for AnkiWeb's sync key and only the key
  is stored (OS keyring). `ANKH_SYNC_KEY` overrides for CI.
- Own collection at `$XDG_DATA_HOME/ankh/<profile>/`; `--collection` can point
  at a desktop Anki profile instead.
- See `CONTEXT.md` for vocabulary and `docs/adr/` for the decisions.

## License

AGPL-3.0-or-later — required by linking `rslib`. See `LICENSE`.
