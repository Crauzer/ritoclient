//! `/riotclientapp/v1/new-args` - the duplicate-instance argv handoff.
//!
//! This is what a *second* `RiotClientServices.exe` calls to hand its argv to
//! the first one before exiting. Its **204 means "arguments accepted"** and
//! nothing else. What the client then does with them is a build- and
//! cohort-dependent question, which is the whole reason this is never the call
//! to launch with. Both halves of that were measured:
//!
//! - On Riot Client 135, posting `--launch-product` + `--launch-patchline` to a
//!   fully booted client answered 204 and launched nothing at all.
//! - On 136.0.3.4787 the same body launched, ~8.4 s later. `App_OnNewArgs`
//!   publishes the raw array before it filters anything, the lifecycle
//!   launch-args object subscribes to that resource and re-parses every switch
//!   in it, and the launch that follows walks the client's entire startup
//!   middleware chain - the one the direct-launch gate sits on. No session id
//!   comes back.
//!
//! Neither measurement was wrong. The install that launched is cohorted into
//! the lifecycle rewrite (`install-settings/cohorts` reports
//! `RC_15.new_lifecycle: "globalEnable"`), so the answer varies by build *and*
//! by user. Treat it as unanswerable and never send a launch pair here.
//!
//! That leaves exactly one use in this workspace: **waking a tray-idle client**,
//! posting an empty array so there is nothing for any build to act on. In that
//! state it is the only route that exists - the client's whole API surface
//! collapses to this single function until it finishes booting. To launch, use
//! [`crate::namespaces::product_launcher`], which is the Play button's own
//! route and reaches none of that middleware.

pub mod endpoints;
pub mod routes;

use ritoclient_core::client::{Client, RequestError, Response};

/// The `/riotclientapp/v1` namespace. Obtained from
/// [`ClientExt::app_args`](crate::ClientExt::app_args).
pub struct AppArgsHandler<'a> {
    client: &'a Client,
}

impl<'a> AppArgsHandler<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// `POST /riotclientapp/v1/new-args` - queue argv for the running client.
    ///
    /// The 204 is an acknowledgement, not a launch report, and on 136 a launch
    /// pair sent here does launch. Wake with `[]`. See the module docs.
    ///
    /// The client's argument convention applies: the body is the bare array
    /// (`["--flag"]`), not an object wrapping it.
    pub fn new_args(&self, args: &[String]) -> Result<Response, RequestError> {
        self.client.endpoint(&endpoints::NewArgs { args }).send()
    }
}
