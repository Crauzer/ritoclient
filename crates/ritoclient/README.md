# ritoclient

[![CI](https://github.com/Crauzer/ritoclient/actions/workflows/ci.yml/badge.svg)](https://github.com/Crauzer/ritoclient/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE-APACHE)

A client for the Riot Client's local API - product discovery, launching, and the
product registry.

The Riot Client runs a loopback HTTPS server whose port and password live in a
lockfile only the current user can read. This crate talks to that server, to the
on-disk records it maintains, and to the process table.

```toml
[dependencies]
ritoclient = "0.1"
```

## One client, reused

It resolves the lockfile on **every attempt** rather than storing a port, so a
single client survives the port change that waking the Riot Client causes. There
is no reconnect step to remember - which is the bug this design exists to
prevent.

The namespace handlers and model conveniences hang off extension traits, so
bring the prelude in with the client:

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

Read-only calls answer `Option`, never `Result`: every caller has a fallback, and
"the client didn't answer" is not a failure worth showing a user. A closed
client, a tray-idle one, and a response shape that moved all arrive as `None`.

## Two layers, and the seam between them

`namespaces` is the high-level half - a handle per API namespace:

```rust
client.lifecycle().hide()?;
let pending = client.product_launcher().is_launch_request_pending();
```

Underneath, every operation is a value implementing `Endpoint` - verb, route,
parameters and output type as data - and the handlers are thin sugar that
construct one and pick a finisher. Drive an endpoint yourself when you want a
different timeout or finisher than the handler picked:

```rust
use ritoclient::namespaces::product_registry::endpoints::Products;

let products = client.endpoint(&Products).ok();
```

`Client` is the low-level half, and it reaches anything the server serves. Only a
handful of the client's 126 namespaces are modelled above, so this is the normal
way to use the rest rather than an escape hatch of last resort:

```rust
use ritoclient::Route;

/// `GET /patch/v1/installs` -> `["league_of_legends.live", …]`
const INSTALLS: Route = Route::new("patch", 1, "installs");

let installs: Vec<String> = client.get_json(INSTALLS).unwrap_or_default();

// Routes with path parameters are filled in by name.
let status = Route::new("patch", 1, "installs/{installId}/status");
let response = client.get(status.bind(&[("installId", "league_of_legends.live")])).send()?;
```

## A status is an answer, not an error

`RequestError` means no round trip happened: nothing was listening, the
connection failed, a body would not encode. Every status the client answers with
- 404 and 5xx included - arrives as a `Response`.

That is not fussiness about error types. The same status means different things
on different routes, so the caller decides: a 404 from `product-launcher` on a
tray-idle client means "wait", while the same 404 from a read query means "give
up".

## Launching

`launch()` is orchestration rather than a namespace: it decides between handing
off to a running client, waking a tray-idle one, and cold-starting, and reports
`LaunchStage`s while it does.

```rust
use ritoclient::ids::{patchlines, products};
use ritoclient::{LaunchTarget, NullObserver};

let target = LaunchTarget {
    product_id: products::LEAGUE_OF_LEGENDS.to_string(),
    patchline_id: patchlines::LIVE.to_string(),
};

let outcome = ritoclient::launch(None, &target, "leagueclient.exe", &NullObserver)?;
```

It returns as soon as the request is *delivered*, which is not the same as the
game being up: the client may still patch, or wait for a login. Implement
`LaunchObserver` to follow progress.

## Scope

- **It never spawns a game executable.** A game's argv carries an
  `rso_auth.authorization-key` blob only an authenticated Riot Client can mint,
  so any design that starts the game directly is wrong regardless of how tempting
  the process tree makes it look. This one is a technical fact, not a policy.
- **It names no products.** Which product and patchline to launch, and which
  executable that produces, are the caller's to supply. `ids` exists so those can
  be written as constants rather than string literals, and nothing here branches
  on them.
- **The endpoint modules stay off the credential surfaces** - `/rso-auth`,
  `/rso-authenticator`, `/player-account`, `/entitlements`, `/payments`. `Client`
  can address them like anything else on loopback; nothing here does. See
  [SECURITY.md](../../SECURITY.md).

Launching is Windows-only. Everything else compiles everywhere, and the
platform-specific entry points return `LauncherError::UnsupportedPlatform` or an
empty result rather than failing to build.

## Feature flags

| Feature    | Default | Effect                                                  |
| ---------- | ------- | -------------------------------------------------------- |
| `launcher` | **on**  | The launch/session orchestration and its Win32 half. Off, this crate is the typed API surface plus the on-disk records |

## License

Apache-2.0. See [LICENSE-APACHE](LICENSE-APACHE).
