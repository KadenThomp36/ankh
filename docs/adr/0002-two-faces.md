# ADR 0002 — TUI and CLI are peers over one engine

**Status:** accepted (2026-08-26)

## Decision

`ankh` with no arguments opens the TUI; every TUI action also exists as a
subcommand with `--format table|json|jsonl`. JSON output carries
`schema_version`. Exit codes are semantic (`Error::exit_code`). Neither face
holds logic the other lacks; both call `ankh-core`.

## Why

Scriptability is the differentiator over existing tools, and a thin TUI over a
tested core is far easier to keep correct than a TUI with its own state.
