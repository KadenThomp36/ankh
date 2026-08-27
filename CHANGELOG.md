# Changelog

## Unreleased

- **Fixed**: with `{{FrontSide}}` on the back template, revealing the answer
  replayed the question's audio before the answer's. The question is now
  extracted first and spliced over the inlined copy, as the desktop does, so
  the reveal plays only what the answer side adds. `r` (replay) on the back
  still plays question then answer, matching the desktop's `replayq` default.
- **Fixed**: showing the answer waited `timeoutlen` (1s) because `<Space>`
  was both `continue` and the leader. Keymaps grew neovim's `nowait`
  (`ankh.keymap.set(view, lhs, rhs, { nowait = true })`), review's `<Space>`
  and `<CR>` use it, and review's leader maps moved to plain keys: `E` edit,
  `A` add, `s` stats, `f0`–`f7` flag.
- **Fixed**: sync and network failures printed only `anki: SyncError` /
  `anki: NetworkError`. `AnkiError`'s `Display` renders the variant name and
  drops the `kind` and message carried in its source, so a wrong password, an
  outdated client and a server outage were indistinguishable. They are now
  unwrapped at the `From<AnkiError>` boundary and read like `sync: AuthFailed:
  Email or password was incorrect; please try again.`
- **Fixed**: network errors exited `1`; `docs/cli.md` documents `6` for
  "sync/network error". They now exit `6`, as the sync errors already did.

## 0.1.0 — 2026-08-26

First release. Everything below happened in one day, on a real 20k-card
Korean collection.

- **Engine**: Anki's own `rslib` (26.08.1) — real v3/FSRS scheduling, real
  AnkiWeb sync (incremental, full, media), real `.anki2` files.
- **CLI**: `login`, `sync`, `status`, `decks`, `next`, `answer`, `search`,
  `card`, `bulk`, `note`, `edit`, `add`, `export`, `import`, `notetypes`,
  `deck`, `options`, `fsrs optimize`, `stats`, `config`; `--format
  table|json|jsonl` with `schema_version`; semantic exit codes.
- **TUI**: deck tree, review screen (audio autoplay, hints, images, flags,
  marks, undo), card browser (Anki search, preview, card info, visual-mode
  bulk operations), stats (calendar heatmap, forecast, counts, answers,
  hours, intervals), embedded `:help`.
- **Rendering**: notetype CSS subset, Unicode/CJK-correct wrapping, ruby,
  Kitty/Sixel/iTerm2/half-block images, theme-safe colours.
- **Editing**: notes as Markdown + frontmatter in `$EDITOR`, lossless
  (raw-HTML fallback), batch files, git-able export/import; nvim ftplugin.
- **Lua**: `init.lua`, options, keymaps (actions or functions), events,
  `:Commands`, `:lua`, plugins; six themes.
- **Management**: decks, options presets as TOML, FSRS optimise, `.apkg`
  export/import, CSV import.
