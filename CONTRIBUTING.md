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

CI runs all four on Windows and Linux, plus an MSRV check and a
`cargo publish --dry-run`. The docs job is not decorative: it fails on a broken
intra-doc link, which is the most common way a doc comment rots here.

## Conventions

### Every namespace gets a folder, and routes get their own file

```text
namespaces/<namespace>/
    mod.rs      the handler and its endpoint methods
    routes.rs   the Route declarations, and the helpers that bind them
```

This holds even for a namespace with a single route. octocrab splits only when a
module grows, which suits a crate whose namespaces are all hand-written; this one
is aimed at the client's 126, with `routes.rs` the file a generator writes and
`mod.rs` the file it must never touch. A layout that changes shape at some size
threshold cannot be that seam, so there is no threshold.

Each `routes.rs` is one `routes!` invocation, which declares the constants and
that namespace's `ALL` table from the same list - so a route cannot be added and
left out of the table:

```rust
crate::routes! {
    namespace = "riot-client-lifecycle";

    /// `POST /riot-client-lifecycle/v1/hide` - "Hide the UX."
    HIDE = 1, "hide";
}
```

The namespace is written once; the **version is written per route**, because a
namespace serves several at a time - `rnet-product-registry` is on v1 and v4
simultaneously.

`namespaces::ALL_ROUTES` merges every table and `namespaces::routes()` flattens
it. Nothing needs updating when a namespace is added beyond the one entry in
`ALL_ROUTES`.

### Everything else

- **`models/` mirrors `namespaces/`.** The types a namespace returns are always
  at the matching path under `models::`. `models::flat` is private generated
  storage under the client's own qualified names; the grouping modules re-export
  from it under ergonomic ones and own the hand-written `impl` blocks.
- **Handlers are named `<Namespace>Handler`.** The suffix looks redundant at four
  namespaces and stops looking that way at 126: the client's namespace names and
  its type names overlap heavily, and `ProductSessionHandler` next to
  `models::product_session::ProductSession` is the collision it prevents.
- **Read-only calls return `Option`, never `Result`.** Every caller has a
  fallback, and "the client didn't answer" is not a failure worth showing a user.
  Only launching returns `LauncherError`.
- **A status is data, not an error.** `RequestError` means no round trip
  happened. Deciding what a 404 means belongs to the caller, because it differs
  per route.
- **Orchestration sits above `namespaces/`, not among it.** Anything that polls,
  spawns a thread, or decides *which* route to drive for a job goes in `launch`
  or `session`.

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
cargo run -p ritoclient-api --example probe          # read-only
cargo run -p ritoclient-api --example hide_and_show  # hides the window, then restores it
cargo run -p ritoclient-api --example launch         # starts a game
```

When a probe confirms or corrects a route spelling, put the finding in the route's
doc comment. That is the only record of it.

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
