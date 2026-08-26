# Sync

`ankh login` exchanges your AnkiWeb password for a sync key and stores only
the key in your OS keyring. Your password is never written anywhere.

ankh syncs when it starts, when you quit (`q`/`ZZ`; `ZQ` skips it), and on
`S` / `:sync`. It never syncs on a timer. Media syncs after each successful
collection sync (`sync.media = false` turns that off).

## Full sync

Some changes (notetype edits, imports of certain packages, "check
database" in Anki desktop) make the two copies unmergeable. ankh then asks
which side wins:

- **download** — replace this collection with AnkiWeb's
- **upload** — replace AnkiWeb's with this one

Headless: `ankh sync --download` / `ankh sync --upload`. A brand-new empty
profile downloads automatically.

## Sharing a collection with Anki desktop

Don't open the same `collection.anki2` from both at once. Either use ankh's
own profile (default; both sync through AnkiWeb), or point ankh at the
desktop file with `--collection PATH` *while the desktop app is closed*.

## Where things live

`ankh config` prints the paths. Each profile has its own collection and
login: `ankh --profile work login`.
