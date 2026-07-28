//! Finding the `RiotClientServices.exe` that owns a given product install.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::LauncherError;

/// `%ProgramData%\Riot Games\RiotClientInstalls.json`.
///
/// Every field defaults and unknown keys are ignored on purpose: Riot adds keys
/// between patches, and a missing one means "try the next candidate", never an
/// error.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RiotClientInstalls {
    /// Maps a product install root to the client that owns it. Keys use forward
    /// slashes and a trailing slash.
    #[serde(default)]
    pub associated_client: HashMap<String, String>,
    #[serde(default)]
    pub rc_default: Option<String>,
    #[serde(default)]
    pub rc_live: Option<String>,
}

impl RiotClientInstalls {
    /// Read and parse the installs document. A missing or malformed file is not
    /// fatal here - the caller turns "no candidates" into
    /// [`LauncherError::RiotClientNotFound`] with the path it tried.
    pub fn load(path: &Path) -> Option<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) => {
                tracing::debug!("No Riot Client installs file at {}: {}", path.display(), e);
                return None;
            }
        };
        match serde_json::from_str(&text) {
            Ok(installs) => Some(installs),
            Err(e) => {
                tracing::warn!("Malformed {}: {}", path.display(), e);
                None
            }
        }
    }

    /// Candidate exe paths, most authoritative first: the client associated
    /// with this very install, then `rc_default`, then `rc_live`.
    ///
    /// Order is the whole point - a machine with both a live and a PBE install
    /// has several entries pointing at different clients, and only
    /// `associated_client` says which one owns *this* game directory.
    pub fn candidates(&self, product_root: Option<&Path>) -> Vec<String> {
        let mut out = Vec::new();

        if let Some(root) = product_root {
            let wanted = installs_key(root);
            for (key, exe) in &self.associated_client {
                if normalize_key(key) == wanted {
                    out.push(exe.clone());
                }
            }
        }

        out.extend(self.rc_default.clone());
        out.extend(self.rc_live.clone());
        out
    }
}

/// The default location of the installs document.
pub fn default_installs_path() -> PathBuf {
    let program_data =
        std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());
    Path::new(&program_data)
        .join("Riot Games")
        .join("RiotClientInstalls.json")
}

/// Normalise a product install path into an `associated_client` lookup key.
///
/// Users may have configured either the install root or the `Game`
/// subdirectory (`GameDir::resolve` accepts both), but Riot only ever keys on
/// the root.
fn installs_key(product_root: &Path) -> String {
    let key = normalize_key(&product_root.to_string_lossy());
    match key.strip_suffix("game/") {
        Some(root) if root.ends_with('/') => root.to_string(),
        _ => key,
    }
}

/// Forward slashes, one trailing slash, lowercased. Windows paths are
/// case-insensitive and Riot's casing does not have to match what the user
/// typed into settings.
///
/// Deliberately string-based rather than going through `Path`: these are always
/// Windows paths, and `Path` on a Unix test runner does not treat `\` as a
/// separator.
fn normalize_key(path: &str) -> String {
    let mut key = path.replace('\\', "/").to_lowercase();
    if !key.ends_with('/') {
        key.push('/');
    }
    key
}

/// Resolve the Riot Client exe for a product install.
///
/// Takes the first candidate that exists on disk. There is deliberately no
/// hardcoded `C:\Riot Games\…` fallback: guessing wrong points a launch at a
/// client that does not own the install.
pub fn resolve_riot_client(
    installs_path: &Path,
    product_root: Option<&Path>,
) -> Result<PathBuf, LauncherError> {
    let not_found = || LauncherError::RiotClientNotFound {
        installs_path: installs_path.display().to_string(),
    };

    let installs = RiotClientInstalls::load(installs_path).ok_or_else(not_found)?;

    installs
        .candidates(product_root)
        .into_iter()
        .map(PathBuf::from)
        .find(|exe| exe.is_file())
        .ok_or_else(not_found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{
        "associated_client": {
            "C:/Riot Games/League of Legends/": "C:/Riot Games/Riot Client/RiotClientServices.exe"
        },
        "patchlines": { "KeystoneFoundationLiveWin": "C:/other/RiotClientServices.exe" },
        "rc_default": "C:/default/RiotClientServices.exe",
        "rc_live": "C:/live/RiotClientServices.exe"
    }"#;

    fn parse(json: &str) -> RiotClientInstalls {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn deserializes_the_full_document_and_ignores_unknown_keys() {
        let installs = parse(FULL);
        assert_eq!(installs.associated_client.len(), 1);
        assert_eq!(
            installs.rc_default.as_deref(),
            Some("C:/default/RiotClientServices.exe")
        );
        assert_eq!(
            installs.rc_live.as_deref(),
            Some("C:/live/RiotClientServices.exe")
        );
    }

    #[test]
    fn deserializes_with_every_key_missing() {
        let installs = parse("{}");
        assert!(installs.associated_client.is_empty());
        assert!(installs.rc_default.is_none());
        assert!(installs.rc_live.is_none());
    }

    #[test]
    fn malformed_json_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("RiotClientInstalls.json");
        std::fs::write(&path, "not json at all").unwrap();
        assert!(RiotClientInstalls::load(&path).is_none());
    }

    #[test]
    fn missing_file_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(RiotClientInstalls::load(&dir.path().join("nope.json")).is_none());
    }

    #[test]
    fn associated_client_wins_over_rc_default_and_rc_live() {
        let installs = parse(FULL);
        let candidates = installs.candidates(Some(Path::new("C:/Riot Games/League of Legends")));
        assert_eq!(
            candidates,
            vec![
                "C:/Riot Games/Riot Client/RiotClientServices.exe",
                "C:/default/RiotClientServices.exe",
                "C:/live/RiotClientServices.exe",
            ]
        );
    }

    #[test]
    fn rc_default_wins_over_rc_live_when_no_association_matches() {
        let installs = parse(FULL);
        let candidates = installs.candidates(Some(Path::new("D:/Games/League of Legends")));
        assert_eq!(
            candidates,
            vec![
                "C:/default/RiotClientServices.exe",
                "C:/live/RiotClientServices.exe",
            ]
        );
    }

    /// Settings hold whatever the user typed or browsed to; the installs file
    /// holds Riot's canonical spelling. Neither slash style, trailing slash nor
    /// casing may decide whether we find the association.
    #[test]
    fn association_matches_across_slash_trailing_slash_and_case() {
        let installs = parse(FULL);
        for input in [
            "C:/Riot Games/League of Legends/",
            "C:/Riot Games/League of Legends",
            "C:\\Riot Games\\League of Legends",
            "c:\\riot games\\league of legends\\",
            "C:/RIOT GAMES/LEAGUE OF LEGENDS",
        ] {
            let candidates = installs.candidates(Some(Path::new(input)));
            assert_eq!(
                candidates.first().map(String::as_str),
                Some("C:/Riot Games/Riot Client/RiotClientServices.exe"),
                "failed for {input}"
            );
        }
    }

    /// `GameDir::resolve` accepts the `Game` subdirectory, so settings can hold
    /// it - but Riot keys the association on the install root.
    #[test]
    fn association_matches_when_settings_point_at_the_game_subdirectory() {
        let installs = parse(FULL);
        let candidates =
            installs.candidates(Some(Path::new("C:\\Riot Games\\League of Legends\\Game")));
        assert_eq!(
            candidates.first().map(String::as_str),
            Some("C:/Riot Games/Riot Client/RiotClientServices.exe")
        );
    }

    /// Only a trailing `Game` segment is stripped - an install that genuinely
    /// lives in a directory called "Game" must still match itself.
    #[test]
    fn only_a_trailing_game_segment_is_stripped() {
        assert_eq!(
            installs_key(Path::new("C:/Riot Games/League of Legends")),
            "c:/riot games/league of legends/"
        );
        assert_eq!(
            installs_key(Path::new("C:/Gameshelf/League of Legends")),
            "c:/gameshelf/league of legends/"
        );
    }

    #[test]
    fn no_league_root_still_offers_the_generic_candidates() {
        let installs = parse(FULL);
        assert_eq!(
            installs.candidates(None),
            vec![
                "C:/default/RiotClientServices.exe",
                "C:/live/RiotClientServices.exe",
            ]
        );
    }

    #[test]
    fn resolve_skips_candidates_that_do_not_exist_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let real_exe = dir.path().join("RiotClientServices.exe");
        std::fs::write(&real_exe, b"").unwrap();

        let installs_path = dir.path().join("RiotClientInstalls.json");
        std::fs::write(
            &installs_path,
            serde_json::json!({
                "rc_default": dir.path().join("missing.exe").to_string_lossy(),
                "rc_live": real_exe.to_string_lossy(),
            })
            .to_string(),
        )
        .unwrap();

        let resolved = resolve_riot_client(&installs_path, None).unwrap();
        assert_eq!(resolved, real_exe);
    }

    #[test]
    fn resolve_errors_with_the_path_it_tried_when_nothing_exists() {
        let dir = tempfile::tempdir().unwrap();
        let installs_path = dir.path().join("RiotClientInstalls.json");
        std::fs::write(
            &installs_path,
            serde_json::json!({ "rc_default": "Z:/definitely/not/here.exe" }).to_string(),
        )
        .unwrap();

        let error = resolve_riot_client(&installs_path, None).unwrap_err();
        assert!(matches!(
            error,
            LauncherError::RiotClientNotFound { ref installs_path }
                if installs_path.contains("RiotClientInstalls.json")
        ));
    }
}
