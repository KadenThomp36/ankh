# CLI contract

## Exit codes

| code | meaning |
|-----:|---------|
| 0 | ok |
| 1 | generic error |
| 3 | not logged in |
| 4 | full sync required (`--download` / `--upload` to resolve) |
| 5 | collection busy |
| 6 | sync/network error |
| 7 | keyring error |

## Output

`--format table` (default, human), `--format json` (one object, with
`schema_version`), `--format jsonl` (one object per line, each with
`schema_version`). Errors go to stderr as `{"schema_version","error","code"}`
in JSON modes.

`schema_version` is bumped on any breaking change to shapes.
