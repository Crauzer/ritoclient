//! `/riotclientapp/v1/new-args` - the duplicate-instance argv handoff.
//!
//! This is what a *second* `RiotClientServices.exe` calls to hand its argv to
//! the first one before exiting. Its **204 means "arguments queued"**, nothing
//! more: against a fully booted client the documented launch body returns 204
//! and launches nothing at all. Against a tray-idle client it wakes the window
//! and then does nothing else - the half-working symptom that makes it look
//! almost right.
//!
//! So it has exactly one use here: **waking a tray-idle client**. In that state
//! it is the only route that exists - the client's whole API surface collapses
//! to this single function until it finishes booting. To launch, use
//! [`crate::namespaces::product_launcher`].

pub mod routes;

use std::time::Duration;

use crate::LauncherError;
use crate::client::{Client, RequestError, StatusCode, WAKE_TIMEOUT};
use crate::launch::LaunchTarget;
use crate::window::allow_foreground;

use routes::NEW_ARGS;

/// The client may be mid-startup when we ask, so a refusal is worth a couple of
/// retries before it counts as unreachable.
const ATTEMPTS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(250);

/// The `/riotclientapp/v1` namespace. Obtained from [`Client::app_args`].
pub struct AppArgsHandler<'a> {
    client: &'a Client,
}

impl<'a> AppArgsHandler<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Wake a tray-idle client by handing it launch arguments.
    ///
    /// Do not mistake its 204 for a launch. See the module docs.
    ///
    /// **This is the call that moves the port.** Waking the client restarts its
    /// remoting listener on a *new port* under the same pid, so from the second
    /// attempt onwards any cached port names something nothing is listening on -
    /// the retries would all fail against a client that had in fact just woken
    /// up. [`Client`] re-reads the lockfile per attempt, which is what makes
    /// retrying this particular call meaningful at all.
    ///
    /// The loop is hand-rolled rather than a [`crate::retry::RetryPolicy`]
    /// because [`allow_foreground`] has to be re-asserted before *each* attempt:
    /// the grant is consumed when a window takes the foreground, so issuing it
    /// once ahead of a retry sequence would leave the later attempts unable to
    /// raise the window.
    pub fn wake_with_launch_args(&self, target: &LaunchTarget) -> Result<(), LauncherError> {
        // Both elements are required: the client only takes its launch branch
        // when `--launch-product` and `--launch-patchline` are both present.
        let args = [
            format!("--launch-product={}", target.product_id),
            format!("--launch-patchline={}", target.patchline_id),
        ];

        let mut last_reason = String::from("no attempt was made");

        for attempt in 1..=ATTEMPTS {
            allow_foreground();

            match self
                .client
                .post(NEW_ARGS)
                .json(&args)
                .timeout(WAKE_TIMEOUT)
                .send()
            {
                Ok(response) if response.status() == StatusCode::NO_CONTENT => {
                    tracing::debug!("Woke the Riot Client with new-args");
                    return Ok(());
                }
                Ok(response) => last_reason = format!("HTTP {}", response.status()),
                Err(RequestError::NoClient) => {
                    last_reason = "the Riot Client is no longer running".to_string();
                }
                Err(e) => last_reason = e.to_string(),
            }

            tracing::debug!("new-args attempt {attempt}/{ATTEMPTS} failed: {last_reason}");
            if attempt < ATTEMPTS {
                std::thread::sleep(RETRY_DELAY);
            }
        }

        Err(LauncherError::RiotClientUnreachable {
            reason: last_reason,
        })
    }
}
