# ritoclient

[![CI](https://github.com/Crauzer/ritoclient/actions/workflows/ci.yml/badge.svg)](https://github.com/Crauzer/ritoclient/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Rust crates for talking to the **Riot Client's local API** - the loopback HTTPS
server the Riot Client runs, whose port and password live in a lockfile only the
current user can read.

| Crate                                        | What it does                                                    |
| -------------------------------------------- | --------------------------------------------------------------- |
| [`ritoclient`](crates/ritoclient)            | **Depend on this one.** Launch and session orchestration, plus the facade over the two below |
| [`ritoclient-api`](crates/ritoclient-api)    | Typed namespaces and models - the generator's crate; nothing hand-written belongs in it |
| [`ritoclient-core`](crates/ritoclient-core)  | Transport: the client, retries, routes, and the lockfile. Mechanism, no policy |

The arrows point one way - `ritoclient` → `ritoclient-api` → `ritoclient-core` -
and Cargo refusing a cycle is what keeps launcher policy out of generated code.
See [`docs/design/layering.md`](docs/design/layering.md).

## Quick start

```rust
use ritoclient::Client;
use ritoclient::prelude::*;

let client = Client::new()?;

for product in client.product_registry().products().unwrap_or_default() {
    for patchline in product.installed_patchlines() {
        println!("{} {} -> {}", product.id, patchline.id, patchline.install_full_path);
    }
}
```

See [the crate README](crates/ritoclient/README.md) for the design in
full, and the rendered docs for everything else.

## Development

```bash
cargo test  --workspace --all-features   # unit tests and doctests
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --open
```

CI runs all of the above on Windows *and* Linux, plus an MSRV check and a
`cargo package --workspace`. Windows is the platform this actually targets;
Linux is there to keep the non-Windows fallbacks compiling.

### Examples need a live client

Everything under `crates/ritoclient/examples/` talks to a real Riot Client,
so none of it runs in CI:

```bash
cargo run -p ritoclient --example probe          # read-only survey
cargo run -p ritoclient --example hide_and_show  # hides the window, then restores it
cargo run -p ritoclient --example launch         # starts a game
```

## Documentation

Doc comments carry what was measured about individual routes. [`docs/`](docs/) carries
what spans them:

- [**The plan**](docs/plans/api-surface-codegen.md) - where the work stands and what
  is next
- [**The survey**](docs/riot-client-local-api.md) - 1261 functions probed against a
  live client; the crate's source data
- [**Layout and prior art**](docs/design/endpoint-layout.md) - why it looks like this,
  and how the endpoint shape was decided
- [**Launch protocol**](docs/launch-protocol.md) · [**Consumers**](docs/consumers.md)

## Status

Pre-1.0 and moving. The transport is stable in shape; the set of modelled
namespaces is not - only a handful of the client's 126 are wrapped so far, and
the rest are reachable through the low-level `Client`. See
[CONTRIBUTING.md](CONTRIBUTING.md) for the conventions a new namespace follows.

## Relationship to Riot Games

Not endorsed by or affiliated with Riot Games. It talks to software already
running on your own machine, with credentials that machine already holds.

## License

Apache-2.0. See [LICENSE](LICENSE).
