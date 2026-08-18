//! A client for the Riot Client's local API.
//!
//! The Riot Client runs a loopback HTTPS server whose port and password live in
//! a lockfile only the current user can read. Everything here talks to that
//! server, or to the on-disk records it maintains ([`installs`]), or to the
//! process table ([`processes`]).
//!
//! This crate is the facade over a three-crate workspace - `ritoclient-core`
//! (the transport), `ritoclient-api` (the typed namespaces, a generator's
//! crate) - plus the hand-written orchestration that lives here: launching,
//! sessions, and the Win32 the client's window needs. Depend on this one.
//!
//! # Getting started
//!
//! One [`Client`], reused. It resolves the lockfile on every attempt rather than
//! storing a port, so a single client survives the port change that waking the
//! Riot Client causes - there is no "reconnect" step to remember.
//!
//! The namespace handlers and the model conveniences hang off extension traits,
//! so bring the prelude in with the client:
//!
//! ```no_run
//! use ritoclient::Client;
//! use ritoclient::prelude::*;
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
//! # use ritoclient::Client;
//! # use ritoclient::prelude::*;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let client = Client::new()?;
//! client.lifecycle().hide()?;
//! let pending = client.product_launcher().is_launch_request_pending();
//! # Ok(())
//! # }
//! ```
//!
//! Between the handlers and the transport sits the endpoint layer: every
//! operation a handler wraps is also a value implementing [`Endpoint`], driven
//! as `client.endpoint(&op)` with the finisher of your choice, and enumerable
//! as plain data through [`namespaces::endpoints`]. The handlers are thin
//! sugar over it - reach past them when you want a different timeout, retry
//! policy, or finisher than the handler picked.
//!
//! [`Client`] itself is the low-level half, and it can reach anything the server
//! serves. Only a handful of the client's 126 namespaces are modelled above, so
//! this is the normal way to use the rest rather than an escape hatch of last
//! resort. Declare a [`Route`] and ask for it:
//!
//! ```no_run
//! use ritoclient::{Client, Route};
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
//! # use ritoclient::{Client, Route};
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
//! When the question is only "does this client serve that route at all?",
//! [`Client::probe`] asks it without invoking anything - a plain `GET` whose
//! answer is read into a [`Presence`]. Whether an endpoint exists is a fact
//! about the client being talked to, not about this crate: it changes per
//! build and per boot state (a tray-idle client serves almost nothing), which
//! is why the endpoint tables do not claim to know it.
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
//! use ritoclient::{Client, RetryPolicy};
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
//! [`Launcher`] is orchestration rather than a namespace: it decides between
//! handing off to a running client, waking a tray-idle one, and cold-starting,
//! and reports [`LaunchStage`]s while it does. Which product to launch and which
//! executable that produces are the caller's to supply, so they are what it is
//! built from.
//!
//! ```no_run
//! use ritoclient::ids::{patchlines, products};
//! use ritoclient::{LaunchTarget, Launcher};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let launcher = Launcher::builder(
//!     LaunchTarget::new(products::LEAGUE_OF_LEGENDS, patchlines::LIVE),
//!     "leagueclient.exe",
//! )
//! .on_progress(|progress| println!("{:?}", progress.stage))
//! .build()?;
//!
//! let outcome = launcher.launch()?;
//! println!("{:?} (session {:?})", outcome.route, outcome.session_id);
//! # Ok(())
//! # }
//! ```
//!
//! It returns as soon as the request is *delivered*, which is not the same as
//! the game being up: the client may still patch, or wait for a login. The
//! session id is the handle for that; see `examples/launch.rs`.
//!
//! # Examples
//!
//! Runnable against a live client, in `examples/`:
//!
//! ```text
//! cargo run -p ritoclient --example probe          # read-only survey
//! cargo run -p ritoclient --example hide_and_show  # hides the window, then restores it
//! cargo run -p ritoclient --example launch         # starts a game
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
//! | Module | Crate | Talks to |
//! | --- | --- | --- |
//! | [`client`] | core | the remoting server, at the transport level |
//! | [`retry`] | core | how a request is repeated when it does not stick |
//! | [`route`] | core | versioned routes as structured data |
//! | [`lockfile`] | core | the remoting lockfile on disk |
//! | [`processes`] | core | the Windows process table |
//! | [`types`] | core | shapes no single namespace owns |
//! | [`namespaces`] | api | one module per API namespace the client serves |
//! | [`models`] | api | the data the API carries, grouped by namespace |
//! | [`ids`] | this | well-known product / patchline identifiers |
//! | [`installs`] | this | `RiotClientInstalls.json` on disk |
//! | [`mod@launch`] | this | orchestration: pick a namespace and drive it |
//! | [`session`] | this | orchestration that outlives a request |
//!
//! Launching is Windows-only; everything else compiles everywhere, and the
//! platform-specific entry points return [`LauncherError::UnsupportedPlatform`]
//! or an empty result rather than failing to build. The `launcher` feature
//! (default on) carries the launch/session orchestration and the only
//! `windows-sys` this crate itself pulls in; without it, this crate is the typed
//! API surface plus the on-disk records.

pub mod error;
pub mod ids;
pub mod installs;
#[cfg(feature = "launcher")]
pub mod launch;
#[cfg(feature = "launcher")]
pub mod progress;
#[cfg(feature = "launcher")]
pub mod session;

mod models_ext;
#[cfg(all(feature = "launcher", target_os = "windows"))]
mod spawn;
#[cfg(feature = "launcher")]
mod window;

pub use ritoclient_api::{ClientExt, models, namespaces};
pub use ritoclient_core::{client, endpoint, lockfile, processes, retry, route, routes, types};

pub use client::{Client, Method, Presence, RequestError, Response, StatusCode};
pub use endpoint::{Endpoint, EndpointBuilder, EndpointMeta};
pub use error::LauncherError;
pub use installs::{RiotClientInstalls, default_installs_path, resolve_riot_client};
#[cfg(feature = "launcher")]
pub use launch::{
    Availability, LaunchOutcome, LaunchRoute, LaunchTarget, Launcher, LauncherBuilder,
    availability, close,
};
pub use lockfile::{Lockfile, default_lockfile_path, live_lockfile};
pub use models::product_registry::{Patchline, Product};
pub use models::product_session::Session;
pub use models_ext::{PatchlineExt, ProductExt, SessionExt, SessionPhase, TerminationReason};
#[cfg(feature = "launcher")]
pub use progress::{LaunchObserver, LaunchProgress, LaunchStage, NullObserver};
pub use retry::{Backoff, RetryPolicy};
pub use route::Route;
#[cfg(feature = "launcher")]
pub use session::{SessionWatch, hide_for_play_session};
pub use types::RiotError;

/// The extension traits, in one `use`.
///
/// [`ClientExt`] hangs the namespace handlers off a [`Client`];
/// [`ProductExt`], [`PatchlineExt`] and [`SessionExt`] put behaviour on the
/// generated model types. All of them exist because an inherent `impl` must
/// live in the crate defining the type, and those types live in the crates
/// below this one.
pub mod prelude {
    pub use ritoclient_api::ClientExt;

    pub use crate::models_ext::{PatchlineExt, ProductExt, SessionExt};
}
