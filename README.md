# ritoclient

[![CI](https://github.com/Crauzer/ritoclient/actions/workflows/ci.yml/badge.svg)](https://github.com/Crauzer/ritoclient/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Rust crates for talking to the **Riot Client's local API** - the loopback HTTPS
server the Riot Client runs, whose port and password live in a lockfile only the
current user can read.

| Crate                                        | What it does                                                    |
| -------------------------------------------- | --------------------------------------------------------------- |
| [`ritoclient-api`](crates/ritoclient-api) | Transport, typed endpoints per namespace, and launch orchestration |

## Quick start

```rust
use ritoclient_api::Client;

let client = Client::new()?;

for product in client.product_registry().products().unwrap_or_default() {
    for patchline in product.installed_patchlines() {
        println!("{} {} -> {}", product.id, patchline.id, patchline.install_full_path);
    }
}
```

See [the crate README](crates/ritoclient-api/README.md) for the design in
full, and the rendered docs for everything else.

## Development

```bash
cargo test  --workspace --all-features   # unit tests and doctests
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --open
```

CI runs all of the above on Windows *and* Linux, plus an MSRV check and a
`cargo publish --dry-run`. Windows is the platform this actually targets; Linux
is there to keep the non-Windows fallbacks compiling.

### Examples need a live client

Everything under `crates/ritoclient-api/examples/` talks to a real Riot Client,
so none of it runs in CI:

```bash
cargo run -p ritoclient-api --example probe          # read-only survey
cargo run -p ritoclient-api --example hide_and_show  # hides the window, then restores it
cargo run -p ritoclient-api --example launch         # starts a game
```

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
