//! Why a request to the Riot Client failed.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why a launch request could not be delivered.
///
/// These deliberately do not share one code: each variant has a different
/// remedy ("set your game path", "open the Riot Client"), so a host can map
/// each to its own error code and branch on that.
///
/// Read-only queries do not use this type - they answer `Option`, because the
/// caller always has a fallback and "the client didn't tell us" is not a failure
/// worth surfacing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[non_exhaustive]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(
    tag = "kind",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub enum LauncherError {
    /// No `RiotClientInstalls.json`, or nothing in it resolves to an exe that
    /// exists on disk.
    #[error("Could not find the Riot Client that owns this game installation")]
    RiotClientNotFound { installs_path: String },

    /// A client is alive and holding the lockfile, but it never answered with
    /// the 204 that means it took the launch args. Cold-starting over it would
    /// terminate the user's session, so this is terminal.
    #[error("The Riot Client did not accept the launch request: {reason}")]
    RiotClientUnreachable { reason: String },

    /// The client understood the request and refused it. On a launch: the
    /// player has not accepted the Terms of Service, the game is not up to
    /// date, the patchline is locked. Distinct from
    /// [`Self::RiotClientUnreachable`] because nothing about the manager is
    /// wrong and retrying changes nothing - the remedy is always something the
    /// player does in the Riot Client itself.
    ///
    /// Not launch-specific: closing and adopting are refused the same way, and
    /// `riot_error_code` is what says why.
    ///
    /// `riot_error_code` is Riot's own machine-readable tag (`eula_not_accepted`
    /// and friends), kept separate from the prose so a host can special-case the
    /// ones worth explaining without matching on English.
    #[error("The Riot Client refused the request: {message}")]
    Refused {
        riot_error_code: String,
        message: String,
    },

    /// A launcher was configured with something it cannot use.
    ///
    /// A programming error rather than a runtime condition - it names what is
    /// wrong rather than a remedy, because the remedy is a code change.
    #[error("The launcher is misconfigured: {reason}")]
    Misconfigured { reason: String },

    /// `RiotClientServices.exe` could not be spawned.
    #[error("Could not start the Riot Client: {reason}")]
    SpawnFailed { reason: String },

    #[error("Launching is only supported on Windows")]
    UnsupportedPlatform,
}

impl From<ritoclient_core::client::RequestError> for LauncherError {
    /// A request that never completed is a client we could not reach.
    ///
    /// The only mapping there is: [`RequestError`] means no round trip
    /// happened, so there is no status to interpret and nothing route-specific
    /// to say. Statuses are a different question entirely, and the layer that
    /// knows what one *means* handles them - see the `launch` module.
    ///
    /// [`RequestError`]: ritoclient_core::client::RequestError
    fn from(error: ritoclient_core::client::RequestError) -> Self {
        Self::RiotClientUnreachable {
            reason: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `kind` tag is what the shell matches on to pick an `ErrorCode`.
    #[test]
    fn variants_serialize_with_a_kind_tag() {
        let json = serde_json::to_value(LauncherError::UnsupportedPlatform).unwrap();
        assert_eq!(json["kind"], "UNSUPPORTED_PLATFORM");

        let json = serde_json::to_value(LauncherError::RiotClientNotFound {
            installs_path: "C:/ProgramData/Riot Games/RiotClientInstalls.json".to_string(),
        })
        .unwrap();
        assert_eq!(json["kind"], "RIOT_CLIENT_NOT_FOUND");
        assert_eq!(
            json["installsPath"],
            "C:/ProgramData/Riot Games/RiotClientInstalls.json"
        );
    }

    /// The conversion exists so callers can use `?` instead of repeating one
    /// `map_err` at every call site. It must keep the reason readable.
    #[test]
    fn a_request_that_never_happened_converts_to_unreachable() {
        let error: LauncherError = ritoclient_core::client::RequestError::NoClient.into();
        assert_eq!(
            error,
            LauncherError::RiotClientUnreachable {
                reason: "no Riot Client is running".to_string(),
            }
        );
    }
}
