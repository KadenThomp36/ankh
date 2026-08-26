# ADR 0004 — Render card HTML semantically, not faithfully

**Status:** accepted (2026-08-26)

A terminal is not a browser. `ankh-render` extracts *meaning* from card
HTML — block structure, emphasis, colour, ruby, images, rules — and
ignores layout CSS. The honoured subset is documented in the crate root.

Choices worth knowing:

- **`.card` colours are ignored.** Card CSS is written for Anki's white (or
  black) page; on the user's terminal theme, black text disappears. Only
  emphasis and alignment are inherited from `.card`. Inner elements keep
  their colours, but a near-black/near-white foreground with no background
  is dropped at draw time (`tui/doc.rs`).
- **Font size maps to weight**: ≥24px/130% → bold, ≤14px/85% → dim.
- **Selectors**: `.class`, `tag`, `tag.class`, `#id`; combinators match the
  last compound selector only; pseudo-classes and attribute selectors are
  skipped.
- **Images** go through `ratatui-image`. The protocol is chosen from the
  environment (ghostty/kitty/wezterm → Kitty; iTerm2; foot/konsole → Sixel;
  tmux → half-blocks) and the cell size from `TIOCGWINSZ`. The
  escape-sequence query is opt-in (`ANKH_QUERY_TERMINAL=1`) because a
  terminal that never answers leaves a reader on stdin that eats keystrokes.
- **Not rendered**: LaTeX (needs a TeX install; shown as code), TTS, JS.
- **RTL/bidi** is a v2 item.
