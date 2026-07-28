//! `/product-launcher/v1` - the route that actually starts a product.
//!
//! This is what the Riot Client's own Play button calls. The distinction from
//! [`crate::namespaces::app_args`] cost a debugging session and is worth stating plainly:
//! that one *queues arguments* and answers 204 without launching anything.
//!
//! A tray-idle client registers only the argv handoff; this plugin appears once
//! it finishes booting, which is why [`crate::launch()`] needs a two-step.

pub mod routes;

use crate::LauncherError;
use crate::client::{Client, LAUNCH_TIMEOUT, RequestError, Response, StatusCode};
use crate::launch::LaunchTarget;

use routes::{ELIGIBILITY, IS_LAUNCH_REQUEST_PENDING, PATCHLINE, target_params};

/// What a launch request found at the other end.
///
/// This is the classification the transport deliberately does not make: a status
/// means different things on different routes, and these are what it means on
/// *this* one.
#[derive(Debug)]
pub enum LaunchAttempt {
    /// The client launched the product. Carries the session id it minted, which
    /// is the key into `/product-session/v1/external-sessions`.
    Launched { session_id: Option<String> },
    /// The client is alive but cannot take the request *yet*. Four shapes, one
    /// meaning:
    ///
    /// - **404** - the product-launcher plugin is not registered. The client is
    ///   still booting, or idling in the tray with a minimal API surface.
    /// - **5xx** - registered, but not far enough along to serve the call.
    /// - **the connection failed** - its remoting listener is restarting, which
    ///   is exactly what waking it does, on a *new port* under the same pid.
    /// - **no live lockfile** - it went away between one attempt and the next.
    ///
    /// The connection failure used to be a hard error, which is why a launch
    /// would fail while the client it was aimed at was merely mid-restart. Every
    /// one of these is a "not yet" that the wait in [`crate::launch()`] recovers
    /// from.
    NotReady { reason: String },
}

/// Turn a refused launch into an error that names its cause.
///
/// Falls back to [`LauncherError::RiotClientUnreachable`] with the raw body when
/// the payload is not the client's error shape - a refusal we cannot explain is
/// still better reported verbatim than swallowed.
fn refusal(response: &Response) -> LauncherError {
    let Some(error) = response.riot_error() else {
        return LauncherError::RiotClientUnreachable {
            reason: format!("HTTP {}: {}", response.status(), response.body().trim()),
        };
    };

    let message = error.prose().to_string();
    tracing::warn!(
        "Riot Client refused the launch: {} ({})",
        message,
        error.error_code
    );
    LauncherError::LaunchRefused {
        riot_error_code: error.error_code,
        message,
    }
}

/// The `/product-launcher/v1` namespace. Obtained from
/// [`Client::product_launcher`].
pub struct ProductLauncherHandler<'a> {
    client: &'a Client,
}

impl<'a> ProductLauncherHandler<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Whether the client considers this product/patchline launchable.
    ///
    /// Advisory: a `false` is worth surfacing, but an unreachable eligibility
    /// check must never block a launch attempt - the endpoint is absent on older
    /// builds.
    ///
    /// **This is an entitlement check, not an install check.** It answers `true`
    /// for `pbe` on a machine with no PBE install, so it cannot gate a patchline
    /// picker; [`crate::models::product_registry::Patchline::is_installed`] is that
    /// question.
    pub fn is_eligible(&self, target: &LaunchTarget) -> Option<bool> {
        self.client
            .get_json(ELIGIBILITY.bind(&target_params(target)))
    }

    /// Whether the client already has a launch in flight.
    ///
    /// The guard against launching twice. Our POST can time out while the client
    /// is still working through its gates, and the timeout cancels nothing at
    /// the far end - so a retry that did not ask this first would queue a second
    /// launch for a request that was already accepted.
    pub fn is_launch_request_pending(&self) -> Option<bool> {
        self.client.get_json(IS_LAUNCH_REQUEST_PENDING)
    }

    /// Ask the client to launch the product.
    pub fn launch(&self, target: &LaunchTarget) -> Result<LaunchAttempt, LauncherError> {
        let response = match self
            .client
            .post(PATCHLINE.bind(&target_params(target)))
            .body("{}")
            .timeout(LAUNCH_TIMEOUT)
            .send()
        {
            Ok(response) => response,
            Err(RequestError::NoClient) => {
                return Ok(LaunchAttempt::NotReady {
                    reason: "the Riot Client is no longer running".to_string(),
                });
            }
            Err(e) => {
                return Ok(LaunchAttempt::NotReady {
                    reason: e.to_string(),
                });
            }
        };

        let status = response.status();

        // A client that has not loaded the launcher plugin answers 404 for the
        // whole namespace; a 5xx is one that loaded it but cannot serve it yet.
        if status == StatusCode::NOT_FOUND || status.is_server_error() {
            return Ok(LaunchAttempt::NotReady {
                reason: format!("HTTP {status}"),
            });
        }

        // Anything else is the client answering, not stalling: an unaccepted ToS, an
        // ineligible product, a locked patchline. Retrying cannot change those, and
        // the status alone cannot describe them - see [`refusal`].
        if !status.is_success() {
            return Err(refusal(&response));
        }

        // The body is a bare JSON string holding the session id. Losing it costs
        // us session tracking, not the launch, so a shape we don't recognise is
        // dropped rather than raised.
        let session_id = response.json::<String>().ok();
        tracing::info!(
            "Riot Client launched {}/{} (session {:?})",
            target.product_id,
            target.patchline_id,
            session_id
        );
        Ok(LaunchAttempt::Launched { session_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use assert_matches::assert_matches;

    /// Verbatim from a live refusal, non-standard status and all. The status is
    /// the part that carries no information: 464 is not in the standard set, so
    /// a user shown "HTTP 464" learns nothing about what to do.
    const EULA_REFUSAL: &str = r#"{"errorCode":"eula_not_accepted","httpStatus":464,"implementationDetails":{},"message":"eula_not_accepted: Cannot run product 'league_of_legends' patchline 'live' because the player didn't accept EULA Terms of Service"}"#;

    fn response(status: u16, body: &str) -> Response {
        Response::new(StatusCode::from_u16(status).unwrap(), body.to_string())
    }

    #[test]
    fn a_refusal_keeps_riots_code_and_drops_its_prefix_from_the_prose() {
        assert_matches!(
            refusal(&response(464, EULA_REFUSAL)),
            LauncherError::LaunchRefused { riot_error_code, message } => {
                assert_eq!(riot_error_code, "eula_not_accepted");
                assert!(
                    message.starts_with("Cannot run product"),
                    "the code is a field, so the prose must not repeat it: {message}"
                );
            }
        );
    }

    /// A refusal we cannot parse still has to reach the user - reporting it
    /// verbatim is worse than a tailored message and far better than nothing.
    #[test]
    fn an_unrecognised_body_falls_back_to_the_raw_text() {
        assert_matches!(
            refusal(&response(409, "patchline is locked")),
            LauncherError::RiotClientUnreachable { reason } => {
                assert!(reason.contains("409"));
                assert!(reason.contains("patchline is locked"));
            }
        );
    }
}
