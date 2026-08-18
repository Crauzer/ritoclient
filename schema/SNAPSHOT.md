# Schema snapshot

Taken with `cargo xtask ritoclient-snapshot` against a live client. Do not edit the
`.json` files by hand - re-take the snapshot instead and read the diff; that diff is
the version-drift check. `overrides.toml` is the opposite: hand-authored, never
overwritten by the snapshot.

| Provenance | |
| --- | --- |
| Date | 2026-07-29 |
| Riot Client build | 135.0.7.4760 |
| Region / locale | EUW / en_US |

| Contents | |
| --- | --- |
| `help.filtered.json` | 157 of 1261 functions, 124 of 3966 types (in-scope namespaces + transitive type closure) |
| `openapi.filtered.json` | 38 paths - swagger covers only a few in-scope namespaces, which is why `/help` is the index |
| `probes.json` | 157 derived paths, parameterless ones GET-probed |

The surface depends on client state: a tray-idle client serves almost nothing, so the
snapshot wakes it first, and entitlements can hide routes per account and region -
which is why the region is provenance and not trivia.
