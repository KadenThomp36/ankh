# Editing notes

`e` (browser) or `<Space>e` (review) opens the note in `$VISUAL`/`$EDITOR`
as a Markdown file; `a` adds one to the current deck. Headless:
`ankh edit ID`, `ankh add`, `ankh note ID`.

```markdown
---
note: 1633756077719
notetype: Korean Vocab
deck: Korean::Vocabulary
tags: leech TTMIK-1.24
---

## Korean

조사

## English

[grammar] particle
```

- `## Heading` lines that name a field delimit fields; other headings are content.
- Markdown becomes Anki HTML: `**bold**`, `*italic*`, lists, `![](image.png)`
  (local paths are copied into the media folder), newlines become `<br>`.
- Fields whose HTML can't be expressed as Markdown appear as raw HTML after a
  `<!-- html -->` marker and are saved back verbatim. Nothing is ever lost.
- Cloze: `{{c1::Seoul}}` works as-is. `[sound:x.mp3]` too.
- One file can hold many notes: repeat the `---` block. `notetype`, `deck`
  and `tags` carry over when omitted. `ankh export QUERY -o deck.md` and
  `ankh import deck.md` round-trip a whole deck through git.

Changing a note's notetype isn't supported here — use Anki desktop.
