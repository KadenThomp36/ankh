# ADR 0005 — Notes are edited as Markdown files with frontmatter

**Status:** accepted (2026-08-26)

## Decision

ankh never embeds a text editor. Editing and adding notes hands a
*note file* to `$VISUAL`/`$EDITOR`:

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

- `## Heading` lines that name a field of the notetype delimit fields; any
  other heading is content.
- A file may hold many notes; each starts with a `---` frontmatter block and
  inherits `notetype`, `deck`, `tags` from the previous one when omitted.
  Notes with a `note:` id are updated, the rest are added. This makes
  `ankh export QUERY -o deck.md` + `git` + `ankh import deck.md` a real
  workflow.

## Markdown ⇄ HTML

Anki fields are HTML fragments. `ankh-core::markdown`:

- Markdown → HTML emits Anki-style fragments: no `<p>` wrappers, paragraphs
  joined by `<br><br>`, newlines as `<br>`, `<img>` without `/>`.
- HTML → Markdown (via `htmd`) is verified by converting back and comparing
  normalised HTML. If the round trip is lossy (inline styles, spans, tables
  with attributes…) the field is emitted as raw HTML behind a
  `<!-- html -->` marker and saved verbatim. **A note is never corrupted by
  editing it.**
- Brackets are unescaped so `[sound:x.mp3]` and `[grammar]` read naturally;
  the round-trip check catches the rare case where that would form a link.
- Local image paths in `<img src>` / `![](path)` are copied into the media
  folder on save and the reference rewritten.

## Not supported

Changing a note's notetype, and notetype/template editing (v1.1).
