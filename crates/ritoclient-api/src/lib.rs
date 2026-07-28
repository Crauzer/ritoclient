//! A client for the Riot Client's local API.
//!
//! The Riot Client runs a loopback HTTPS server whose port and password live in
//! a lockfile only the current user can read. Everything here talks to that
//! server, or to the on-disk records it maintains ([`installs`]), or to the
//! process table ([`processes`]).
//!
//! # Getting started
//!
//! One [`Client`], reused. It resolves the lockfile on every attempt rather than
//! storing a port, so a single client survives the port change that waking the
//! Riot Client causes - there is no "reconnect" step to remember.
//!
//! ```no_run
//! use ritoclient_api::Client;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Client::new()?;
//!
//! for product in client.product_registry().products().unwrap_or_default() {
//!     for patchline in product.installed_patchlines() {
//!         println!("{} {} -> {}", product.id, patchline.id, patchline.install_full_path);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Read-only calls answer `Option`, never `Result`: every caller has a fallback,
//! and "the client didn't answer" is not a failure worth showing a user. A
//! closed client, a tray-idle one, and a response shape that moved all arrive as
//! `None`.
//!
//! # Two layers
//!
//! [`namespaces`] is the high-level half - a handle per API namespace, reached
//! from the client:
//!
//! ```no_run
//! # use ritoclient_api::Client;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let client = Client::new()?;
//! client.lifecycle().hide()?;
//! let pending = client.product_launcher().is_launch_request_pending();
//! # Ok(())
//! # }
//! ```
//!
//! [`Client`] itself is the low-level half, and it can reach anything the server
//! serves. Only a handful of the client's 126 namespaces are modelled above, so
//! this is the normal way to use the rest rather than an escape hatch of last
//! resort. Declare a [`Route`] and ask for it:
//!
//! ```no_run
//! use ritoclient_api::{Client, Route};
//!
//! /// `GET /patch/v1/installs` -> `["league_of_legends.live", …]`
//! const INSTALLS: Route = Route::new("patch", 1, "installs");
//!
//! /// `GET /vanguard/v1/status`. `restartRequired` is the field that matters:
//! /// Vanguard can demand a reboot, and a launch attempted in that state fails
//! /// in a way that looks like a bug in the caller.
//! const VANGUARD_STATUS: Route = Route::new("vanguard", 1, "status");
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let client = Client::new()?;
//! let installs: Vec<String> = client.get_json(INSTALLS).unwrap_or_default();
//!
//! // Routes with path parameters are filled in by name.
//! let status = Route::new("patch", 1, "installs/{installId}/status");
//! let response = client.get(status.bind(&[("installId", "league_of_legends.live")])).send()?;
//! # Ok(())
//! # }
//! ```
//!
//! A path can also be passed as a plain string, which is what the manual probes
//! in `examples/` use when confirming a spelling. Prefer a [`Route`]: it keeps
//! the version - the part most likely to break - visible and greppable.
//!
//! # A status is an answer, not an error
//!
//! [`RequestError`] means no round trip happened: nothing was listening, the
//! connection failed, a body would not encode. Every status the client answers
//! with arrives as a [`Response`], 404 and 5xx included.
//!
//! That is not fussiness about error types. The same status means different
//! things on different routes, so the caller decides:
//!
//! ```no_run
//! # use ritoclient_api::{Client, Route};
//! # const SOME_ROUTE: Route = Route::new("patch", 1, "installs");
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let client = Client::new()?;
//! let response = client.get(SOME_ROUTE).send()?;
//!
//! if response.status().as_u16() == 404 {
//!     match response.riot_error() {
//!         // Registered, but its handler is not available yet - worth retrying.
//!         Some(error) if error.is_rpc_error() => {}
//!         // No such route: wrong spelling, or the plugin is not registered.
//!         // On a client that is still booting, even this can mean "not yet".
//!         Some(error) if error.is_resource_not_found() => {}
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Timeouts and retries
//!
//! Both are configurable on the client and overridable per request. Retrying is
//! opt-in: the default is a single attempt, because most calls here are queries
//! on paths where a user is waiting.
//!
//! ```no_run
//! use std::time::Duration;
//!
//! use ritoclient_api::{Client, RetryPolicy};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Client::builder()
//!     .retry(RetryPolicy::attempts(3))
//!     .timeout(Duration::from_secs(5))
//!     .build()?;
//!
//! let response = client
//!     .get("/patch/v1/installs")
//!     .timeout(Duration::from_millis(500))
//!     .retry(RetryPolicy::none())
//!     .send()?;
//! # Ok(())
//! # }
//! ```
//!
//! Retrying transport failures is worth more than it looks: waking the client
//! restarts its remoting listener on a *new port*, so a request issued across
//! that window is refused by a client that is perfectly healthy. Because every
//! attempt re-reads the lockfile, a retry picks up the new port.
//!
//! # Launching
//!
//! [`launch()`] is orchestration rather than a namespace: it decides between
//! handing off to a running client, waking a tray-idle one, and cold-starting,
//! and reports [`LaunchStage`]s while it does. Which product to launch and which
//! executable that produces are the caller's to supply.
//!
//! ```no_run
//! use ritoclient_api::ids::{patchlines, products};
//! use ritoclient_api::{LaunchTarget, NullObserver};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let target = LaunchTarget {
//!     product_id: products::LEAGUE_OF_LEGENDS.to_string(),
//!     patchline_id: patchlines::LIVE.to_string(),
//! };
//!
//! let outcome = ritoclient_api::launch(None, &target, "leagueclient.exe", &NullObserver)?;
//! println!("{:?} (session {:?})", outcome.route, outcome.session_id);
//! # Ok(())
//! # }
//! ```
//!
//! It returns as soon as the request is *delivered*, which is not the same as
//! the game being up: the client may still patch, or wait for a login. Implement
//! [`LaunchObserver`] to follow progress, and see `examples/launch.rs`.
//!
//! # Examples
//!
//! Runnable against a live client, in `examples/`:
//!
//! ```text
//! cargo run -p ritoclient-api --example probe          # read-only survey
//! cargo run -p ritoclient-api --example hide_and_show  # hides the window, then restores it
//! cargo run -p ritoclient-api --example launch         # starts a game
//! ```
//!
//! # Scope
//!
//! - **It never spawns a game executable.** A game's argv carries an
//!   `rso_auth.authorization-key` blob only an authenticated Riot Client can
//!   mint, so any design that starts the game directly is wrong regardless of
//!   how tempting the process tree makes it look. This one is a technical fact,
//!   not a policy.
//! - **It names no products.** Which product and patchline to launch, and which
//!   executable that produces, are all the caller's to supply;
//!   [`ids`] exists so those can be
//!   written as constants rather than string literals, and nothing here
//!   branches on them.
//! - **The endpoint modules stay off `/rso-auth`, `/rso-authenticator`,
//!   `/player-account`, `/entitlements` and `/payments`.** Those are the
//!   credential surfaces and we have no business modelling them. [`Client`] can
//!   address them like anything else on loopback; nothing here does.
//! - **Two logging rules, and they are the real constraints.** Never log the
//!   lockfile password - [`Lockfile`]'s `Debug` redacts it. And
//!   `/product-session/v1/sessions` returns an RSO authorization key inside
//!   `launchConfiguration.arguments`, so anything dumping a session payload for
//!   diagnostics strips that field first.
//!
//! # Layout
//!
//! | Module | Talks to |
//! | --- | --- |
//! | [`client`] | the remoting server, at the transport level |
//! | [`retry`] | how a request is repeated when it does not stick |
//! | [`namespaces`] | one module per API namespace the client serves |
//! | [`models`] | the data the API carries, grouped by namespace |
//! | [`types`] | shapes no single namespace owns |
//! | [`ids`] | well-known product / patchline identifiers |
//! | [`installs`] | `RiotClientInstalls.json` on disk |
//! | [`lockfile`] | the remoting lockfile on disk |
//! | [`processes`] | the Windows process table |
//! | [`mod@launch`] | orchestration: pick a namespace and drive it |
//! | [`session`] | orchestration that outlives a request |
//!
//! Launching is Windows-only; everything else compiles everywhere, and the
//! platform-specific entry points return [`LauncherError::UnsupportedPlatform`]
//! or an empty result rather than failing to build.

// Several modules explain their own privacy - `models::flat` is private on
// purpose, and the prose says so - and naming the private item is the clearest
// way to write that. Rustdoc renders these as plain code rather than links,
// which is the intended result.
#![allow(rustdoc::private_intra_doc_links)]

pub mod client;
pub mod error;
pub mod ids;
pub mod installs;
pub mod launch;
pub mod lockfile;
pub mod models;
pub mod namespaces;
pub mod processes;
pub mod progress;
pub mod retry;
pub mod route;
pub mod session;
pub mod types;

#[cfg(target_os = "windows")]
mod spawn;
mod window;

pub use client::{Client, RequestError, Response, StatusCode};
pub use error::LauncherError;
pub use installs::{RiotClientInstalls, default_installs_path, resolve_riot_client};
pub use launch::{Availability, LaunchOutcome, LaunchRoute, LaunchTarget, availability, launch};
pub use lockfile::{Lockfile, default_lockfile_path, live_lockfile};
pub use models::product_registry::{Patchline, Product};
pub use progress::{LaunchObserver, LaunchProgress, LaunchStage, NullObserver};
pub use retry::{Backoff, RetryPolicy};
pub use route::Route;
pub use session::hide_for_play_session;
pub use types::RiotError;
