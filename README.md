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
- [x] **2 · review** — the study screen: question/answer, four ratings with FSRS intervals, undo, bury/suspend/flag/mark, audio autoplay (mpv), `ankh next` / `ankh answer`
- [x] **3 · rendering** — notetype CSS (classes, alignment, size→weight), Unicode/CJK wrapping, `<ruby>`, inline images (Kitty/Sixel/iTerm2/half-blocks), hints (`H`), type-answer and MathJax placeholders, theme-safe colours
- [x] **4 · browser** — Anki search syntax, sortable table, question/answer preview, card info + review log, visual-mode bulk ops (suspend/bury/flag/mark/tag/move/due/forget/delete), `ankh search` / `ankh card` / `ankh bulk`
- [x] **5 · editor** — notes as Markdown + frontmatter in `$EDITOR` (lossless: un-round-trippable HTML stays raw), batch add, `ankh export`/`import` for git-able decks, nvim ftplugin
- [x] **6 · Lua** — embedded Lua 5.4: `init.lua`, `ankh.setup`, keymaps to actions or functions, events, `:Commands`, `:lua`, read/act API, plugins on `package.path`, six themes
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

Inside the TUI: `j`/`k` move, `h`/`l` fold, `Enter` studies a deck, `S` sync,
`<Space>` opens which-key, `:` for the command line, `?` for the current
view's keymap, `q` goes back / syncs and quits.

Reviewing: `Space` shows the answer and then rates *good*; `1`–`4` (or
`a`/`h`/`g`/`e`) rate; `u` undo; `-` bury; `!` suspend; `*` mark; `r` replay
audio; `H` reveal hints; `i` card info; `<Space>1`…`7` flag.

Browsing (`/` on a deck, or `b`): `/` edits the search (Anki syntax), `Tab`
flips the preview, `I` card info, `v` starts a visual range, then `!` `-` `*`
`t` `T` `m` `d` `D` `F` act on it, `o`/`O` change the sort.

```sh
ankh search 'deck:Korean is:due' --sort due --format jsonl | jq .sort_field
ankh card 1633756077720
ankh bulk 'tag:leech prop:lapses>10' --suspend
ankh add -d Inbox "front" "back"          # or just `ankh add` to open $EDITOR
ankh edit 1633756077719                   # note as Markdown in $EDITOR
ankh export 'deck:Korean::Mining' -o mining.md && git add mining.md
ankh import mining.md                     # ids update, new entries add
```

Editing: `e` in the browser or `<Space>e` while reviewing opens the note in
`$EDITOR`; `a` adds a note to the current deck. See `docs/adr/0005-note-files.md`
for the file format and `contrib/nvim` for the ftplugin.

## Configure

`~/.config/ankh/init.lua` (see `ankh config --defaults` for every default and
`docs/lua.md` for the API):

```lua
local ankh = require("ankh")
ankh.setup({ theme = "catppuccin", sync = { on_quit = false } })
ankh.keymap.set("review", "<leader>d", function()
  local c = ankh.card.current()
  os.execute(("xdg-open 'https://en.dict.naver.com/#/search?query=%s' &"):format(c.question))
end, { desc = "look up in Naver" })
ankh.on("card_answered", function(card) print(card.id, card.rating) end)
ankh.command("Leeches", function() ankh.browse('tag:leech deck:"' .. ankh.deck.current() .. '"') end)
```

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
