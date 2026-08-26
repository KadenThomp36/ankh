# Keys

ankh is modal. In **normal** mode keys are commands; `:` opens the
**command** line; the browser's search box is **insert** mode; `v` in the
browser starts a **visual** range. `<Esc>` always backs out.

Counts work like neovim: `5j` moves five decks. Sequences wait `timeoutlen`
for their next key (`<Space>` alone shows the answer; `<Space>e` edits).

Press `?` in any view for that view's live keymap — it reflects your
`init.lua`. Everything below is the default.

## Every view

| key | action |
|-----|--------|
| `j` `k` `gg` `G` | move / scroll |
| `S` `<Space>ss` | sync |
| `<Space>sd` `<Space>su` | full download / upload |
| `u` | undo |
| `:` | command line |
| `?` | keys for this view |
| `q` | back (or quit from decks: syncs first) |
| `ZZ` `ZQ` | quit with / without sync |

## Decks

`<CR>` study · `h` `l` `za` fold · `/` browse this deck · `b` browser ·
`a` add note · `s` stats · `<Space>S` stats for everything · `o` options ·
`n` `r` `D` new / rename / delete deck · `<Space>f` optimise FSRS · `R` refresh

## Review

`<Space>` `<CR>` show answer, then *good* · `l` show answer ·
`1` `2` `3` `4` or `a` `h` `g` `e` rate · `-` bury · `!` suspend ·
`*` mark · `r` replay audio · `H` reveal hints · `i` card info ·
`<Space>0`–`7` flag · `<Space>e` edit note · `<Space>a` add note ·
`<Space>s` stats · `<C-d>` `<C-u>` scroll · `<BS>` back · `U` unbury

## Browser

`/` `i` edit search · `<C-l>` new search · `<Tab>` flip preview · `p` hide preview ·
`I` card info · `<CR>` study this card's deck · `e` edit · `a` add ·
`v` `V` visual range · `!` suspend/unsuspend · `-` bury · `*` mark ·
`t` `T` add / remove tags · `m` move to deck · `d` set due · `D` delete notes ·
`F` forget · `<Space>0`–`7` flag · `o` `O` sort column / direction · `<BS>` back

## Command line

`:q` `:q!` `:sync [download|upload]` `:refresh` `:undo` `:help [topic]`
`:browse QUERY` `:sort COL` `:tag T` `:untag T` `:move DECK` `:due N`
`:flag N` `:bury` `:suspend` `:unbury` `:delete` `:forget` `:info`
`:edit` `:add` `:stats [all]` `:options` `:fsrs optimize|on|off`
`:deck create|rename|delete` `:theme NAME` `:audio on|off` `:lua CODE`
plus any `ankh.command(...)` you defined.
