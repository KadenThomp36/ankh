# ankh nvim support

Copy (or symlink) this directory into your runtimepath, e.g.
`~/.config/nvim/pack/ankh/start/ankh/`, or with lazy.nvim:

```lua
{ dir = "~/src/ankh/contrib/nvim" }
```

You get: filetype detection for `*.ankh.md`, Markdown highlighting with the
frontmatter, `## Field` headings, clozes and `[sound:]` tags coloured, `]]`/`[[`
to jump between fields, and `<C-c>` in visual mode to wrap a cloze.
