-- ankh defaults. This file is embedded in the binary and runs before your
-- ~/.config/ankh/init.lua. Copy it with `ankh config --defaults`.
--
-- Views:  decks · review · browser   (plus "global" = every view)
-- Keys:   neovim notation — j, gg, <C-d>, <S-Tab>, <CR>, <leader>s
-- Rhs:    an action name (see `ankh.actions()`) or a Lua function

local native = ...

local ankh = { keymap = {}, review = {}, card = {}, deck = {}, note = {}, browser = {} }

--- Options ------------------------------------------------------------------

local function flatten(prefix, tbl, out)
  for k, v in pairs(tbl) do
    local key = prefix == "" and k or (prefix .. "." .. k)
    if type(v) == "table" then flatten(key, v, out) else out[key] = v end
  end
  return out
end

--- ankh.setup({ theme = "tokyonight", sync = { on_launch = true }, ... })
function ankh.setup(opts)
  for k, v in pairs(flatten("", opts or {}, {})) do native.set_option(k, v) end
end

--- ankh.get("sync.on_quit")
function ankh.get(key) return native.get_option(key) end

--- Keymaps ------------------------------------------------------------------

--- ankh.keymap.set(view, lhs, rhs, { desc = "..." })
function ankh.keymap.set(view, lhs, rhs, opts)
  native.keymap_set(view, lhs, rhs, opts and opts.desc or nil)
end

--- ankh.keymap.del(view, lhs)
function ankh.keymap.del(view, lhs) native.keymap_del(view, lhs) end

--- Events & commands --------------------------------------------------------

--- ankh.on("card_shown", function(card) ... end)
--- events: startup · card_shown · card_answered · review_done · sync_done ·
---         view_changed · note_saved · quit
function ankh.on(event, fn) native.on(event, fn) end

--- ankh.command("Hello", function(args) ankh.notify("hi " .. args) end, { desc = "..." })
--- then :Hello world
function ankh.command(name, fn, opts) native.command(name, fn, opts and opts.desc or nil) end

--- Doing things ---------------------------------------------------------------

function ankh.notify(msg) native.notify(tostring(msg)) end
function ankh.error(msg) native.error(tostring(msg)) end
--- run any action by name, e.g. ankh.action("sync")
function ankh.action(name) native.action(name) end
--- run a :command, e.g. ankh.cmd("browse deck:Korean")
function ankh.cmd(line) native.cmd(line) end
function ankh.sync() native.action("sync") end
function ankh.quit() native.action("quit") end
function ankh.actions() return native.actions() end
--- ankh.browse("deck:Korean is:due")
function ankh.browse(query) native.browse(query or "") end
--- ankh.search("tag:leech") -> { card ids }
function ankh.search(query) return native.search(query) end
--- ankh.render(html) -> plain text
function ankh.render(html) return native.render(html) end

function ankh.review.show_answer() native.action("show_answer") end
--- ankh.review.answer("good")   (again | hard | good | easy)
function ankh.review.answer(rating) native.action("rate " .. rating) end
function ankh.review.undo() native.action("undo") end
function ankh.review.bury() native.action("bury") end
function ankh.review.suspend() native.action("suspend") end
function ankh.review.flag(n) native.action("flag " .. tostring(n)) end
function ankh.review.replay() native.action("replay") end

--- the card on screen (or nil): { id, note_id, deck, kind, flag, tags,
--- question, answer, question_html, answer_html, buttons }
function ankh.card.current() return native.card_current() end
--- ankh.card.info(id) -> stats + review log
function ankh.card.info(id) return native.card_info(id) end

--- ankh.deck.list() -> flat list of { id, name, full_name, new, learn, review, level }
function ankh.deck.list() return native.decks() end
function ankh.deck.current() return native.deck_current() end

--- ankh.note.get(id) -> { id, notetype, deck, tags, fields = { Front = "...", ... } }
function ankh.note.get(id) return native.note_get(id) end
--- ankh.note.add({ notetype = "Basic", deck = "Inbox", tags = {"x"}, fields = { Front = "...", Back = "..." } }) -> id
function ankh.note.add(t) return native.note_add(t) end

function ankh.view() return native.view() end
function ankh.mode() return native.mode() end

package.loaded["ankh"] = ankh
_G.ankh = ankh

--- Default options ------------------------------------------------------------

ankh.setup({
  theme = "tokyonight",          -- tokyonight · catppuccin · gruvbox · rose-pine · nord · dracula
  leader = "<Space>",
  timeoutlen = 1000,             -- ms to wait for the rest of a key sequence
  sync = { on_launch = true, on_quit = true, media = true },
  audio = { autoplay = true },
  review = { show_timer = true },
})

--- Default keymaps ------------------------------------------------------------

local map = ankh.keymap.set

-- every view
map("global", "j", "down", { desc = "down" })
map("global", "<Down>", "down", { desc = "down" })
map("global", "k", "up", { desc = "up" })
map("global", "<Up>", "up", { desc = "up" })
map("global", "gg", "top", { desc = "top" })
map("global", "G", "bottom", { desc = "bottom" })
map("global", "S", "sync", { desc = "sync" })
map("global", "<leader>ss", "sync", { desc = "sync" })
map("global", "<leader>sd", "sync_download", { desc = "sync: full download" })
map("global", "<leader>su", "sync_upload", { desc = "sync: full upload" })
map("global", "u", "undo", { desc = "undo" })
map("global", ":", "command_mode", { desc = "command line" })
map("global", "?", "help", { desc = "help" })
map("global", "q", "quit", { desc = "quit / back" })
map("global", "ZZ", "quit", { desc = "quit (syncs first)" })
map("global", "ZQ", "force_quit", { desc = "quit without syncing" })
map("global", "<Esc>", "clear_message", { desc = "clear message" })

-- decks
map("decks", "<C-d>", "half_down", { desc = "half page down" })
map("decks", "<C-u>", "half_up", { desc = "half page up" })
map("decks", "l", "expand", { desc = "expand" })
map("decks", "h", "collapse", { desc = "collapse / parent" })
map("decks", "za", "toggle_fold", { desc = "toggle fold" })
map("decks", "<CR>", "open", { desc = "study deck" })
map("decks", "R", "refresh", { desc = "refresh" })
map("decks", "/", "browse_deck", { desc = "browse this deck" })
map("decks", "b", "open_browser", { desc = "browser" })
map("decks", "a", "add_note", { desc = "add note to deck" })

-- review
map("review", "<Space>", "continue", { desc = "show answer / good" })
map("review", "<CR>", "continue", { desc = "show answer / good" })
map("review", "l", "show_answer", { desc = "show answer" })
map("review", "1", "rate again", { desc = "again" })
map("review", "2", "rate hard", { desc = "hard" })
map("review", "3", "rate good", { desc = "good" })
map("review", "4", "rate easy", { desc = "easy" })
map("review", "a", "rate again", { desc = "again" })
map("review", "h", "rate hard", { desc = "hard" })
map("review", "g", "rate good", { desc = "good" })
map("review", "e", "rate easy", { desc = "easy" })
map("review", "-", "bury", { desc = "bury card" })
map("review", "!", "suspend", { desc = "suspend card" })
map("review", "*", "toggle_mark", { desc = "mark / unmark note" })
map("review", "r", "replay", { desc = "replay audio" })
map("review", "H", "toggle_hints", { desc = "reveal / hide hints" })
map("review", "i", "card_info", { desc = "card info" })
map("review", "U", "unbury", { desc = "unbury deck" })
map("review", "<BS>", "back", { desc = "back to decks" })
map("review", "<C-d>", "scroll_down", { desc = "scroll down" })
map("review", "<C-u>", "scroll_up", { desc = "scroll up" })
map("review", "/", "browse_deck", { desc = "browse this deck" })
map("review", "<leader>e", "edit_note", { desc = "edit note in $EDITOR" })
map("review", "<leader>a", "add_note", { desc = "add note to deck" })
for n = 0, 7 do
  local names = { "clear flag", "flag red", "flag orange", "flag green", "flag blue", "flag pink", "flag turquoise", "flag purple" }
  map("review", "<leader>" .. n, "flag " .. n, { desc = names[n + 1] })
end
-- `g` is "good" in review, so no `gg` there.
ankh.keymap.del("review", "gg")

-- browser
map("browser", "/", "insert_mode", { desc = "edit search" })
map("browser", "i", "insert_mode", { desc = "edit search" })
map("browser", "<C-l>", "clear_search", { desc = "new search" })
map("browser", "v", "visual_mode", { desc = "visual select" })
map("browser", "V", "visual_mode", { desc = "visual select" })
map("browser", "<C-d>", "half_down", { desc = "half page down" })
map("browser", "<C-u>", "half_up", { desc = "half page up" })
map("browser", "p", "preview", { desc = "toggle preview" })
map("browser", "<Tab>", "flip_preview", { desc = "preview question / answer" })
map("browser", "<CR>", "study_card", { desc = "study this card's deck" })
map("browser", "I", "card_info", { desc = "card info" })
map("browser", "e", "edit_note", { desc = "edit note in $EDITOR" })
map("browser", "a", "add_note", { desc = "add note" })
map("browser", "!", "toggle_suspend", { desc = "suspend / unsuspend" })
map("browser", "-", "bulk_bury", { desc = "bury" })
map("browser", "*", "bulk_mark", { desc = "mark / unmark" })
map("browser", "t", "prompt_tag", { desc = "add tags" })
map("browser", "T", "prompt_untag", { desc = "remove tags" })
map("browser", "m", "prompt_move", { desc = "move to deck" })
map("browser", "d", "prompt_due", { desc = "set due date" })
map("browser", "D", "confirm_delete", { desc = "delete notes" })
map("browser", "F", "confirm_forget", { desc = "forget (reset to new)" })
map("browser", "o", "cycle_sort", { desc = "next sort column" })
map("browser", "O", "reverse_sort", { desc = "reverse sort" })
map("browser", "<BS>", "back", { desc = "back to decks" })
for n = 0, 7 do
  local names = { "clear flag", "flag red", "flag orange", "flag green", "flag blue", "flag pink", "flag turquoise", "flag purple" }
  map("browser", "<leader>" .. n, "bulk_flag " .. n, { desc = names[n + 1] })
end
