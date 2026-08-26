# Lua API

ankh embeds Lua 5.4. On start it runs the built-in `defaults.lua` (see it
with `ankh config --defaults`) and then `~/.config/ankh/init.lua`. A broken
`init.lua` never locks you out: the app starts with defaults and shows the
error on the message line.

```lua
local ankh = require("ankh")   -- also available as the global `ankh`
```

## Options

```lua
ankh.setup({
  theme = "tokyonight",        -- tokyonight · catppuccin · gruvbox · rose-pine · nord · dracula
  leader = "<Space>",
  timeoutlen = 1000,           -- ms to wait for the rest of a key sequence
  sync = { on_launch = true, on_quit = true, media = true },
  audio = { autoplay = true },
  review = { show_timer = true },
  my = { anything = "goes" },  -- read back with ankh.get("my.anything")
})
```

## Keymaps

```lua
ankh.keymap.set(view, lhs, rhs, { desc = "shown in which-key and :help" })
ankh.keymap.del(view, lhs)
```

- `view`: `"global"` (every view), `"decks"`, `"review"`, `"browser"`.
- `lhs`: neovim notation — `j`, `gg`, `<C-d>`, `<S-Tab>`, `<CR>`, `<leader>s`.
- `rhs`: an **action name** (`ankh.actions()` lists them; e.g. `"sync"`,
  `"rate good"`, `"flag 3"`) or a **Lua function**.

In the review view digits are ratings, so counts (`5j`) are disabled there.

## Events

```lua
ankh.on("card_shown", function(card) ... end)
```

| event | payload |
|-------|---------|
| `startup` | – |
| `card_shown` | card table |
| `card_answered` | card table + `rating` |
| `review_done` | – |
| `sync_done` | – |
| `quit` | – |

## Commands

```lua
ankh.command("Due", function(args) ... end, { desc = "count due" })
-- then :Due anything
```

## Doing things

| function | effect |
|----------|--------|
| `ankh.notify(msg)` / `ankh.error(msg)` | message line |
| `ankh.action(name)` | run any action, e.g. `ankh.action("sync")` |
| `ankh.cmd(":browse deck:Korean")` | run a `:` command |
| `ankh.sync()`, `ankh.quit()` | |
| `ankh.browse(query)` | open the browser on a search |
| `ankh.review.show_answer()`, `.answer("good")`, `.undo()`, `.bury()`, `.suspend()`, `.flag(n)`, `.replay()` | review actions |

## Reading things

| function | returns |
|----------|---------|
| `ankh.card.current()` | `{ id, note_id, deck, notetype, kind, flag, tags, question, answer, question_html, answer_html, buttons, counts, answer_shown }` or nil |
| `ankh.card.info(id)` | stats, FSRS state, review log |
| `ankh.deck.list()` | flat list of `{ id, name, full_name, level, new, learn, review, total }` |
| `ankh.deck.current()` | name of the deck under the cursor / being studied |
| `ankh.note.get(id)` | `{ id, notetype, deck, tags, fields = { Front = "…" }, card_ids }` |
| `ankh.note.add({ notetype=, deck=, tags=, fields= })` | new note id (fields are HTML) |
| `ankh.search(query)` | list of card ids |
| `ankh.render(html)` | plain text |
| `ankh.view()`, `ankh.mode()` | `"decks"`/`"review"`/`"browser"`, `"normal"`/… |
| `ankh.get(key)` | an option |

Anything else Lua can do, it can do: `os.execute`, `io.open`, `require` of
files on `package.path`. There is no sandbox — same stance as neovim.

## Examples

```lua
-- Look the current word up in a dictionary with <leader>d while reviewing.
ankh.keymap.set("review", "<leader>d", function()
  local c = ankh.card.current()
  if c then os.execute(("xdg-open 'https://en.dict.naver.com/#/search?query=%s' >/dev/null 2>&1 &"):format(c.question)) end
end, { desc = "look up in Naver" })

-- Log every answer to a file.
ankh.on("card_answered", function(card)
  local f = io.open(os.getenv("HOME") .. "/.ankh-log", "a")
  f:write(os.date("%F %T"), "\t", card.id, "\t", card.rating, "\n")
  f:close()
end)

-- A :Leeches command that opens the browser on leeches in the current deck.
ankh.command("Leeches", function()
  ankh.browse(('deck:"%s" tag:leech'):format(ankh.deck.current() or ""))
end, { desc = "browse leeches here" })
```

## Plugins

`~/.config/ankh/plugins/?.lua` and `?/init.lua` are on `package.path`, so
`require("myplugin")` works from `init.lua`.
