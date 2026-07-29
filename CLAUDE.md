# CLAUDE.md

Guidance for Claude Code (claude.ai/code) when working in this repository.

## What this is

A Rust workspace of three published crates - a client for the Riot Client's
local loopback API. Apache-2.0. Dependencies point downward only:

```text
crates/ritoclient/        orchestration + facade. What downstream depends on
crates/ritoclient-api/    typed namespaces and models. The generator's crate
crates/ritoclient-core/   transport: client, retry, route, lockfile, processes
```

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

**Never use em dashes (or en dashes).** Not in prose, doc comments, docs, or
commit messages. Use a plain hyphen with spaces (` - `) or restructure the
sentence so no dash is needed; numeric ranges use a bare hyphen (`1-5`).

**Never use section signs.** Write `section 1.5`, not `§1.5` - easier for
humans to write and read.

**Avoid trivially descriptive comments.** Only comment non-obvious business logic,
workarounds, edge cases, or "why" decisions. Do not add inline comments that
restate what the code already expresses. Strip narration comments before
committing. Document all public APIs with `///`.

Doc comments here carry things that were **measured against a live client** - a
status code that surprised someone, a route spelling that cost a debugging
session. That knowledge exists nowhere else, so preserve it verbatim when moving
code.

## Layout

| Module       | Crate | Talks to                                        |
| ------------ | ----- | ----------------------------------------------- |
| `client`     | core  | the remoting server, at the transport level. Owns `Method`/`StatusCode`; reqwest appears nowhere else |
| `endpoint`   | core  | operations as values: the `Endpoint` trait, `EndpointBuilder`, `EndpointMeta` |
| `retry`      | core  | how a request is repeated when it does not stick |
| `route`      | core  | versioned routes as structured data, and the `routes!` macro |
| `lockfile`   | core  | the remoting lockfile on disk                   |
| `processes`  | core  | the Windows process table (lockfile liveness needs it) |
| `types`      | core  | shapes no single namespace owns                 |
| `namespaces` | api   | one module per API namespace, plus `ClientExt`  |
| `models`     | api   | the data the API carries, grouped by namespace  |
| `ids`        | facade | well-known product / patchline identifiers     |
| `installs`   | facade | `RiotClientInstalls.json` on disk              |
| `launch`     | facade | orchestration: pick a namespace and drive it   |
| `session`    | facade | orchestration that outlives a request          |
| `models_ext` | facade | `ProductExt`/`PatchlineExt` - behaviour for generated model types |

## Invariants - do not break these without being asked

- **Dependencies point downward only** - facade → api → core. Cargo refusing a
  cycle is the enforcement; do not work around it with a re-export or a feature.
- **Nothing hand-written belongs in `ritoclient-api`** beyond what a generator
  would write. It must not loop, sleep, call the OS, name a launcher type, or
  decide what a status means. Its `Cargo.toml` is the allowlist.
- **Every namespace gets a folder** (`namespaces/<ns>/`) with `mod.rs` for the
  handler, `routes.rs` for the routes, and `endpoints.rs` for the endpoint types
  and their `EndpointMeta` table, even at one route.
- **`routes.rs` is one `routes!` invocation.** It declares the constants and the
  namespace's `ALL` table from the same list. Namespace written once, version
  written per route - a namespace serves several versions at a time.
- **Handlers construct an endpoint and pick a finisher, nothing else.** The
  operation itself is an `Endpoint` impl in `endpoints.rs`; cross-cutting
  behaviour is a combinator on core's `EndpointBuilder`, written once.
- **`models/` mirrors `namespaces/`.** `models::flat` is private generated
  storage; grouping modules re-export from it. Behaviour goes on the facade's
  extension traits (`models_ext.rs`), never as an inherent `impl`.
- **Handlers return `Option<T>` or `Result<Response, RequestError>`, nothing
  else.** Read-only calls answer `Option`. Only the facade's orchestration
  returns `LauncherError`.
- **A status is data, not an error.** `RequestError` means no round trip
  happened. What a 404 means is the caller's call, because it differs per route.
- **Orchestration lives in the facade.** Anything that polls, spawns a
  thread, or picks *which* route to drive belongs in `launch` or `session`.
- **Handlers are named `<Namespace>Handler`**, to avoid colliding with the
  identically-named model types.

## Hard rules

- Never log the lockfile password. `Lockfile`'s `Debug` redacts it.
- `/product-session/v1/sessions` returns an RSO authorization key inside
  `launchConfiguration.arguments`; strip it before dumping a session payload.
- Do not add endpoint wrappers for `/rso-auth`, `/rso-authenticator`,
  `/player-account`, `/entitlements` or `/payments`.
- The crates name no products and spawn no game executable. See the `# Scope`
  section of `crates/ritoclient/src/lib.rs` for why the second one is a
  technical fact rather than a policy.

## Testing

Unit tests never touch a live client. Anything that needs one goes in
`examples/`, which CI does not run.
