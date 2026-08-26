# Changelog

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
