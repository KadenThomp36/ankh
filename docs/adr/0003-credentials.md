# ADR 0003 — Store only the AnkiWeb sync key, in the OS keyring

**Status:** accepted (2026-08-26)

`ankh login` exchanges username+password for AnkiWeb's host key (`hkey`) and
stores `{username, hkey, endpoint}` as JSON in the OS keyring
(`service=ankh`, `user=<profile>`). The password is dropped immediately and
never written anywhere. `ANKH_SYNC_KEY` / `ANKH_SYNC_ENDPOINT` /
`ANKH_SYNC_USER` override the keyring for CI and containers.

Rotating the AnkiWeb password does not invalidate an issued hkey; `ankh logout`
does.
