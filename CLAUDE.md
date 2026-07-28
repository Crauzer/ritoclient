# CLAUDE.md

Guidance for Claude Code (claude.ai/code) when working in this repository.

## What this is

A Rust workspace with one crate, `crates/ritoclient-api`: a client for the Riot
Client's local loopback API. Apache-2.0.

## Commands

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
```

All four must be clean; CI enforces each on Windows and Linux. The docs one
matters more than it looks - broken intra-doc links are the usual way a change
here regresses.

## Editing rules

**Always read files before editing them.** Never assume file contents from memory
or prior context. When making bulk edits, read all target files first.

**Avoid trivially descriptive comments.** Only comment non-obvious business logic,
workarounds, edge cases, or "why" decisions. Do not add inline comments that
restate what the code already expresses. Strip narration comments before
committing. Document all public APIs with `///`.

Doc comments here carry things that were **measured against a live client** - a
status code that surprised someone, a route spelling that cost a debugging
session. That knowledge exists nowhere else, so preserve it verbatim when moving
code.

## Layout

| Module       | Talks to                                        |
| ------------ | ----------------------------------------------- |
| `client`     | the remoting server, at the transport level     |
| `retry`      | how a request is repeated when it does not stick |
| `namespaces` | one module per API namespace the client serves  |
| `models`     | the data the API carries, grouped by namespace  |
| `types`      | shapes no single namespace owns                 |
| `ids`        | well-known product / patchline identifiers      |
| `installs`   | `RiotClientInstalls.json` on disk               |
| `lockfile`   | the remoting lockfile on disk                   |
| `processes`  | the Windows process table                       |
| `launch`     | orchestration: pick a namespace and drive it    |
| `session`    | orchestration that outlives a request           |

## Invariants - do not break these without being asked

- **Every namespace gets a folder** (`namespaces/<ns>/`) with `mod.rs` for the
  handler and `routes.rs` for the routes, even at one route. `routes.rs` is the
  file a generator will write; `mod.rs` is the file it must never touch.
- **`routes.rs` is one `routes!` invocation.** It declares the constants and the
  namespace's `ALL` table from the same list. Namespace written once, version
  written per route - a namespace serves several versions at a time.
- **`models/` mirrors `namespaces/`.** `models::flat` is private generated
  storage; grouping modules re-export from it and own the `impl` blocks.
- **Read-only calls return `Option`, never `Result`.** Only launching returns
  `LauncherError`.
- **A status is data, not an error.** `RequestError` means no round trip
  happened. What a 404 means is the caller's call, because it differs per route.
- **Orchestration lives above `namespaces/`.** Anything that polls, spawns a
  thread, or picks *which* route to drive belongs in `launch` or `session`.
- **Handlers are named `<Namespace>Handler`**, to avoid colliding with the
  identically-named model types.

## Hard rules

- Never log the lockfile password. `Lockfile`'s `Debug` redacts it.
- `/product-session/v1/sessions` returns an RSO authorization key inside
  `launchConfiguration.arguments`; strip it before dumping a session payload.
- Do not add endpoint wrappers for `/rso-auth`, `/rso-authenticator`,
  `/player-account`, `/entitlements` or `/payments`.
- The crate names no products and spawns no game executable. See the `# Scope`
  section of `lib.rs` for why the second one is a technical fact rather than a
  policy.

## Testing

Unit tests never touch a live client. Anything that needs one goes in
`examples/`, which CI does not run.
