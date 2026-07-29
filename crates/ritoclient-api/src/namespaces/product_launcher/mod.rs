//! `/product-launcher/v1` - the route that actually starts a product.
//!
//! This is what the Riot Client's own Play button calls. The distinction from
//! [`crate::namespaces::app_args`] cost a debugging session and is worth stating plainly:
//! that one *queues arguments* and answers 204 without launching anything.
//!
//! A tray-idle client registers only the argv handoff; this plugin appears once
//! it finishes booting, which is why the launch orchestration in the
//! `ritoclient` crate needs a two-step.

pub mod endpoints;
pub mod routes;

use std::time::Duration;

use ritoclient_core::client::{Client, RequestError, Response};

/// A launch has to survive a client that is mid-startup, so it waits.
///
/// Generous because the client runs its own gates before it spawns anything -
/// a patch-state refresh, a player-affinity token, an up-to-date check - and
/// each is a round trip to Riot's servers. Five seconds covered none of that on
/// a slow link, and a timeout here does **not** cancel the work: the client went
/// on to launch minutes later while we had already reported a failure.
pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);

/// The `/product-launcher/v1` namespace. Obtained from
/// [`ClientExt::product_launcher`](crate::ClientExt::product_launcher).
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
    /// picker; whether the patchline is on disk is
    /// [`models::product_registry::Patchline`](crate::models::product_registry::Patchline)'s
    /// `install_full_path` being non-empty.
    pub fn is_eligible(&self, product_id: &str, patchline_id: &str) -> Option<bool> {
        self.client
            .endpoint(&endpoints::Eligibility {
                product_id,
                patchline_id,
            })
            .ok()
    }

    /// Whether the client already has a launch in flight.
    ///
    /// The guard against launching twice. A launch POST can time out while the
    /// client is still working through its gates, and the timeout cancels
    /// nothing at the far end - so a retry that did not ask this first would
    /// queue a second launch for a request that was already accepted.
    pub fn is_launch_request_pending(&self) -> Option<bool> {
        self.client
            .endpoint(&endpoints::IsLaunchRequestPending)
            .ok()
    }

    /// Ask the client to launch the product.
    ///
    /// On success the body is a bare JSON string holding the session id the
    /// client minted - the key into `/product-session/v1/external-sessions`.
    /// What any status means here is the caller's to decide: a 404 from this
    /// namespace on a tray-idle client means "wait", and a refusal (an
    /// unaccepted ToS, a locked patchline) arrives as a status with a payload,
    /// not as an error.
    pub fn launch(&self, product_id: &str, patchline_id: &str) -> Result<Response, RequestError> {
        self.client
            .endpoint(&endpoints::Patchline {
                product_id,
                patchline_id,
            })
            .timeout(LAUNCH_TIMEOUT)
            .send()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ritoclient_core::Endpoint;

    #[test]
    fn the_endpoints_bind_both_ids() {
        let launch = endpoints::Patchline {
            product_id: "league_of_legends",
            patchline_id: "pbe",
        };
        assert_eq!(
            launch.path(),
            "/product-launcher/v1/products/league_of_legends/patchlines/pbe"
        );

        let eligibility = endpoints::Eligibility {
            product_id: "league_of_legends",
            patchline_id: "pbe",
        };
        assert_eq!(
            eligibility.path(),
            "/product-launcher/v1/products/league_of_legends/patchlines/pbe/eligibility"
        );
    }
}
