# ankh — domain vocabulary

Terms as used in code, docs and the UI. Where Anki has a word, we use Anki's.

## Anki concepts (inherited, unchanged)

- **Collection** — one SQLite file (`collection.anki2`) holding everything for
  a profile. The unit of sync.
- **Note** — the data: a set of **fields** filled in against a **notetype**.
- **Card** — what you review. Generated from a note by one of the notetype's
  **templates** (a note with a "reversed" template yields two cards).
- **Notetype** — field list + card templates (HTML) + CSS. "Basic", "Cloze".
- **Cloze** — a notetype whose templates are derived from `{{c1::…}}` markers
  in a field; each cloze number is a card.
- **Deck** — a named container of cards; nested with `::`. A **filtered deck**
  is a temporary deck built from a search.
- **Deck options** (preset) — daily limits, learning steps, FSRS params; shared
  by many decks.
- **Queue** — what a card is waiting in: new, learning, review, day-learn,
  suspended, buried. **Due** = new + learning + review counts for today.
- **Scheduler** — v3 with FSRS. Answering a card yields four **ratings**:
  again / hard / good / easy.
- **Revlog** — the review history; source of all stats.
- **Media** — files referenced by fields (`<img src>`, `[sound:x.mp3]`), stored
  beside the collection in `collection.media/`.
- **Sync** — incremental (**normal**) or **full** (one side overwrites the
  other; required after a schema change). **Media sync** is separate.
- **hkey** — the AnkiWeb host key issued on login; the only credential we keep.

## ankh concepts

- **Engine** — `ankh-core::Engine`: one open collection + AnkiWeb client.
- **Profile** — a named `{collection, media, login}` triple under
  `$XDG_DATA_HOME/ankh/<profile>/`. Default: `default`.
- **View** — a screen in the TUI (`decks`, `review`, `browser`, `editor`,
  `stats`, `options`, `help`). Analogous to a neovim buffer.
- **Mode** — how keys are interpreted inside a view: `normal`, `insert`,
  `command`, `visual`. Exactly neovim's meaning.
- **Keymap** — bindings for one (view, mode). Key notation is neovim's:
  `j`, `gg`, `<C-d>`, `<leader>s`.
- **Action** — a named, argument-free thing the UI can do. Keys map to actions;
  `:commands` and Lua call the same actions.
- **Pristine** — a collection that has never held a card. A full-sync
  conflict on a pristine collection resolves to "download" automatically.
- **Report** — the result of a sync (`SyncReport`): outcome + server message.
- **Note file** — a note serialised as Markdown + YAML frontmatter for
  editing in `$EDITOR` (milestone 5).
