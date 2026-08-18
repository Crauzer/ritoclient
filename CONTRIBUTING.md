# Contributing

## Getting set up

Stable Rust, edition 2024 (MSRV is the `rust-version` in the workspace
`Cargo.toml`). Nothing else - the crate has no build script and no external
tooling.

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
```

CI runs all four on Windows and Linux, plus an MSRV check, a
`cargo package --workspace`, and a codegen drift check (regenerate, then
`git diff --exit-code`). The docs job is not decorative: it fails on a
broken intra-doc link, which is the most common way a doc comment rots here.

Two workspace tasks, run through the `cargo xtask` alias:

```bash
cargo xtask ritoclient-snapshot   # re-take schema/ from a live Riot Client
cargo xtask ritoclient-codegen    # regenerate crates/ritoclient-api/src, offline
```

## The three crates

```text
crates/ritoclient/        orchestration and the facade. Depend on this one
crates/ritoclient-api/    typed namespaces and models. The generator's crate
crates/ritoclient-core/   transport: client, retry, route, lockfile, processes
```

Dependencies point downward only, and Cargo refusing a cycle is the boundary's
enforcement. The practical rule: **anything that loops, sleeps, calls the OS, or
decides what a status means goes in `ritoclient`; nothing hand-written goes in
`ritoclient-api` beyond what a generator would write** - see
[`docs/design/layering.md`](docs/design/layering.md). Behaviour for generated
model types is an extension trait in `ritoclient` (`PatchlineExt`, `ProductExt`
in `models_ext.rs`), never an inherent `impl`.

## Background

This file is the *how*. The *why*, and the measurements behind it, are in [`docs/`](docs/) -
start with [`docs/README.md`](docs/README.md). Before adding a namespace, read its entry in
[the survey](docs/riot-client-local-api.md); before changing how endpoints are shaped, read
[the layout doc](docs/design/endpoint-layout.md), which records how the current shape was decided.

## Conventions

### Every namespace gets a folder, and three files inside it

```text
namespaces/<namespace>/
    mod.rs         the handler and its endpoint methods
    routes.rs      the Route declarations
    endpoints.rs   the endpoint types and the namespace's EndpointMeta table
```

This holds even for a namespace with a single route. The crate is aimed at the
client's 126 namespaces and **is written whole by the generator**: `cargo xtask
ritoclient-codegen` wipes `src/` and rewrites it from `schema/`, and CI fails
on any drift, so hand-editing a generated file cannot stick. A layout that
changes shape at some size threshold cannot be a generator target, so there is
no threshold. To add or change a namespace, edit `schema/overrides.toml`
(names, spellings, doc prose - data, never code) and regenerate.

Each `routes.rs` is one `routes!` invocation, which declares the constants and
that namespace's `ALL` table from the same list - so a route cannot be added and
left out of the table:

```rust
ritoclient_core::routes! {
    namespace = "riot-client-lifecycle";

    /// `POST /riot-client-lifecycle/v1/hide` - "Hide the UX."
    HIDE = 1, "hide";
}
```

The namespace is written once; the **version is written per route**, because a
namespace serves several at a time - `rnet-product-registry` is on v1 and v4
simultaneously.

`namespaces::ALL_ROUTES` merges every table and `namespaces::routes()` flattens
it; `ALL_ENDPOINTS` / `endpoints()` do the same for the endpoint tables. Nothing
needs updating when a namespace is added beyond those two entries.

### Endpoints are types; handlers are sugar

Each operation is a struct implementing `ritoclient_core::Endpoint` - the verb
and route as associated consts, path parameters as borrowed fields, the output
as an associated type. The handler method constructs the endpoint and picks a
finisher (`send()`, `json()`, `ok()`, `ignore()`); nothing else belongs in it.
`endpoints.rs` also declares the namespace's `ALL: &[EndpointMeta]` row per
endpoint, and a test in `namespaces/mod.rs` asserts the tables stay in step
with the impls and the route tables.

The tables say what the crate declares, never what a client serves: existence
varies per client build and boot state, so it is asked at runtime with
`Client::probe`, not recorded as metadata.

### Everything else

- **`models/` mirrors `namespaces/`.** The types a namespace returns are always
  at the matching path under `models::`. `models::flat` is private generated
  storage under the client's own qualified names; the grouping modules re-export
  from it under ergonomic ones. Behaviour for those types lives in
  `ritoclient`'s extension traits, never in `ritoclient-api`.
- **Handlers are named `<Namespace>Handler`.** The suffix looks redundant at four
  namespaces and stops looking that way at 126: the client's namespace names and
  its type names overlap heavily, and `ProductSessionHandler` next to
  `models::product_session::ProductSession` is the collision it prevents.
- **Handlers return `Option<T>` or `Result<Response, RequestError>`, nothing
  else.** Read-only calls answer `Option`, because every caller has a fallback
  and "the client didn't answer" is not a failure worth showing a user. Only the
  orchestration in `ritoclient` returns `LauncherError`.
- **A status is data, not an error.** `RequestError` means no round trip
  happened. Deciding what a 404 means belongs to the caller, because it differs
  per route.
- **Orchestration lives in `ritoclient`, not in `namespaces/`.** Anything that
  polls, spawns a thread, or decides *which* route to drive for a job goes in
  `launch` or `session`.

### Doc comments

Public APIs get `///`. Record what was *measured* against a live client - a
status code that surprised you, a spelling that cost a debugging session - and
skip anything the code already says. Avoid comments that narrate the next line.

## Scope and secrets

Two rules that are not style preferences:

- **Never log the lockfile password.** `Lockfile`'s `Debug` redacts it; keep it
  that way.
- **`/product-session/v1/sessions` returns an RSO authorization key** inside
  `launchConfiguration.arguments`. Anything dumping a session payload for
  diagnostics strips that field first.

The endpoint modules stay off `/rso-auth`, `/rso-authenticator`,
`/player-account`, `/entitlements` and `/payments`. This is a scoping decision
about what we have business modelling, not an access control boundary - `Client`
reaches them like anything else on loopback. Please do not add wrappers for them.

## Testing

Unit tests run without a client and cover parsing, route binding, and the tables.
Anything needing a live client belongs in `examples/`, which CI never runs:

```bash
cargo run -p ritoclient --example probe          # read-only
cargo run -p ritoclient --example hide_and_show  # hides the window, then restores it
cargo run -p ritoclient --example launch         # starts a game
```

When a probe confirms or corrects a route spelling, record the finding in
`schema/overrides.toml` and regenerate - measured knowledge single-homes there
and the generator emits it, because a regenerated crate keeps nothing typed
into it by hand. `schema/probes.json` is the snapshot's own ledger of what a
live client answered per derived path.

## Commits and releases

[Conventional commits](https://www.conventionalcommits.org) - `cliff.toml` builds
the changelog from them, so `feat:`, `fix:`, `perf:`, `refactor:` and `docs:`
each land in their own section and anything else falls under "Other".

Releases are tag-driven: push `vX.Y.Z` and `.github/workflows/release.yml` runs
the tests, drafts notes with git-cliff, and publishes to crates.io. That last
step needs a `CARGO_REGISTRY_TOKEN` repository secret.

## Known follow-ups

- **`missing_docs` is not enabled.** It currently fires ~90 times, mostly on
  struct fields and constants. Worth turning on in the workspace lints once
  that is paid down.
- **`clippy::undocumented_unsafe_blocks`** fires twice. Both are in the Windows
  process-table walker and want a `// SAFETY:` line.
