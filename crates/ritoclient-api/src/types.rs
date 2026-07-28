//! Types that belong to no namespace in particular.
//!
//! [`crate::models`] holds the data a *specific* namespace carries. This holds
//! the rest: shapes the client uses everywhere, which no namespace owns and
//! which therefore have nowhere sensible to sit among them.

use serde::Deserialize;

/// The client's own error payload.
///
/// ```json
/// {"errorCode":"eula_not_accepted","httpStatus":464,"implementationDetails":{},
///  "message":"eula_not_accepted: Cannot run product 'league_of_legends' ..."}
/// ```
///
/// Worth parsing even when the status looks self-explanatory: Riot answers
/// refusals with codes outside the standard set (464 for an unaccepted ToS), so
/// the status classifies nothing on its own.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiotError {
    pub error_code: String,
    #[serde(default)]
    pub http_status: Option<u16>,
    #[serde(default)]
    pub message: String,
}

impl RiotError {
    /// The route is registered but its handler is unavailable - the plugin is
    /// present and not ready, or does not apply to this account. Worth retrying.
    pub fn is_rpc_error(&self) -> bool {
        self.error_code == "RPC_ERROR"
    }

    /// No such route: the spelling is wrong, or the plugin is not registered at
    /// all. Whether that is terminal depends on the caller - a client still
    /// booting registers its namespaces late.
    pub fn is_resource_not_found(&self) -> bool {
        self.error_code == "RESOURCE_NOT_FOUND"
    }

    /// The message with its own error code stripped from the front.
    ///
    /// The client prefixes the prose with the code it already gave you as a
    /// field, so anything rendering both would say it twice.
    pub fn prose(&self) -> &str {
        self.message
            .strip_prefix(&format!("{}: ", self.error_code))
            .unwrap_or(&self.message)
    }
}
