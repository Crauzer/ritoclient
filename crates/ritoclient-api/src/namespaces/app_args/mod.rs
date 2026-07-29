//! `/riotclientapp/v1/new-args` - the duplicate-instance argv handoff.
//!
//! This is what a *second* `RiotClientServices.exe` calls to hand its argv to
//! the first one before exiting. Its **204 means "arguments queued"**, nothing
//! more: against a fully booted client the documented launch body returns 204
//! and launches nothing at all. Against a tray-idle client it wakes the window
//! and then does nothing else - the half-working symptom that makes it look
//! almost right.
//!
//! So it has exactly one use in this workspace: **waking a tray-idle client**,
//! which is the `ritoclient` crate's launch orchestration. In that state it is
//! the only route that exists - the client's whole API surface collapses to
//! this single function until it finishes booting. To launch, use
//! [`crate::namespaces::product_launcher`].

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
    /// Do not mistake its 204 for a launch. See the module docs.
    ///
    /// The client's argument convention applies: the body is the bare array
    /// (`["--flag"]`), not an object wrapping it.
    pub fn new_args(&self, args: &[String]) -> Result<Response, RequestError> {
        self.client.endpoint(&endpoints::NewArgs { args }).send()
    }
}
