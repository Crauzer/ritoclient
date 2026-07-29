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
#[derive(Debug, Clone, Serialize, Deserialize, Error)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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

    /// The client understood the request and refused it: the player has not
    /// accepted the Terms of Service, the game is not up to date, the patchline
    /// is locked. Distinct from [`Self::RiotClientUnreachable`] because nothing
    /// about the manager is wrong and retrying changes nothing - the remedy is
    /// always something the player does in the Riot Client itself.
    ///
    /// `riot_error_code` is Riot's own machine-readable tag (`eula_not_accepted`
    /// and friends), kept separate from the prose so a host can special-case the
    /// ones worth explaining without matching on English.
    #[error("The Riot Client refused to launch the game: {message}")]
    LaunchRefused {
        riot_error_code: String,
        message: String,
    },

    /// `RiotClientServices.exe` could not be spawned.
    #[error("Could not start the Riot Client: {reason}")]
    SpawnFailed { reason: String },

    #[error("Launching is only supported on Windows")]
    UnsupportedPlatform,
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
}
