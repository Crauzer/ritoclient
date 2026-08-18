//! Behaviour for the generated model types.
//!
//! `ritoclient-api` is (destined to be) generator output, so hand-written
//! methods cannot live there - and an inherent `impl` must live in the crate
//! defining the type, so they cannot be written here as `impl Product` either.
//! Extension traits are the standard answer, re-exported from
//! [`crate::prelude`] so callers pay one `use`.

use std::path::PathBuf;

use ritoclient_api::models::product_registry::{Patchline, Product};
use ritoclient_api::models::product_session::Session;

/// Behaviour for [`Product`].
pub trait ProductExt {
    fn patchline(&self, id: &str) -> Option<&Patchline>;

    /// Only the patchlines that are actually on disk.
    fn installed_patchlines(&self) -> impl Iterator<Item = &Patchline>;
}

impl ProductExt for Product {
    fn patchline(&self, id: &str) -> Option<&Patchline> {
        self.patchlines.iter().find(|p| p.id == id)
    }

    fn installed_patchlines(&self) -> impl Iterator<Item = &Patchline> {
        self.patchlines.iter().filter(|p| p.is_installed())
    }
}

/// Behaviour for [`Patchline`].
pub trait PatchlineExt {
    /// Whether this patchline is on disk.
    ///
    /// **This is the install test.** `/product-launcher/…/eligibility` is not:
    /// it answers `true` for `pbe` on a machine with no PBE install, because it
    /// asks whether the account *may* launch it.
    fn is_installed(&self) -> bool;

    /// The install root, when installed.
    fn install_root(&self) -> Option<PathBuf>;

    /// The directory a named secondary patchline resolves to.
    ///
    /// For League, `secondary_dir(ids::secondary_patchlines::GAME_PATCH)` is the
    /// `Game` subdirectory - a **declared relative path**, so it is derived
    /// rather than assumed. A patchline that declares no such secondary gets
    /// `None`: guessing here would defeat the point of asking.
    fn secondary_dir(&self, id: &str) -> Option<PathBuf>;
}

impl PatchlineExt for Patchline {
    fn is_installed(&self) -> bool {
        !self.install_full_path.is_empty()
    }

    fn install_root(&self) -> Option<PathBuf> {
        self.is_installed()
            .then(|| PathBuf::from(&self.install_full_path))
    }

    fn secondary_dir(&self, id: &str) -> Option<PathBuf> {
        let root = self.install_root()?;
        let relative = self.secondary_patchlines.iter().find(|s| s.id == id)?;
        if relative.relative_path.is_empty() {
            return None;
        }
        Some(root.join(&relative.relative_path))
    }
}

/// The wire spellings of `ProductSessionProductPhase`, which the generated
/// model carries as a `String` so a variant Riot adds cannot break
/// deserialization. Private because reading them is what [`SessionExt`] is for.
mod phases {
    pub const GAMEPLAY: &str = "Gameplay";
}

/// The wire spellings of `ProductSessionTerminationReason`. `STILL_RUNNING` is
/// the one that matters: it is the value a live session carries, so "has it
/// ended?" is a test against it rather than a list of the ways it can end.
mod termination {
    pub const STILL_RUNNING: &str = "StillRunning";
}

/// Behaviour for [`Session`].
///
/// These read the two enum fields, which arrive as strings. The point of
/// wrapping them is that both have a value meaning "nothing has happened yet"
/// that is easy to mistake for its opposite: a session sitting at `Pending` is
/// not playing, and one reporting `StillRunning` has not ended.
pub trait SessionExt {
    /// Whether the game is actually up.
    ///
    /// False for a session the client has opened but not got a game into yet -
    /// which is the state a session is in for the first few seconds after a
    /// launch is accepted.
    fn is_playing(&self) -> bool;

    /// Whether the session is over.
    ///
    /// This is the field to poll rather than the process table when the
    /// question is "did it exit, and why" - a game that dies during startup
    /// leaves a reason here, where the process table only shows an absence.
    /// `exit_code` is meaningful once this is true and not before.
    ///
    /// A session the client has never heard of deserializes to an empty
    /// `exit_reason`, which reads as not ended: an absent session is not the
    /// same fact as a finished one, and the caller that asked for an id it
    /// does not have should get `None` from the lookup instead.
    fn has_ended(&self) -> bool;
}

impl SessionExt for Session {
    fn is_playing(&self) -> bool {
        self.phase == phases::GAMEPLAY
    }

    fn has_ended(&self) -> bool {
        !self.exit_reason.is_empty() && self.exit_reason != termination::STILL_RUNNING
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ritoclient_api::models::product_registry::SecondaryPatchline;

    use crate::ids::{products, secondary_patchlines};

    /// Recorded from a live client (EUW, 2026-07-27), trimmed to the fields we
    /// read. `pbe` is entitled but not installed on that machine, which is what
    /// makes it the useful half of the fixture.
    const RECORDED: &str = r#"[
      {
        "id": "league_of_legends",
        "patchlines": [
          {
            "id": "live",
            "install_id": "league_of_legends.live",
            "full_name": "League of Legends",
            "install_full_path": "C:/Riot Games/League of Legends",
            "install_dir": "League of Legends",
            "primary_executable": "LeagueClient.exe",
            "release_id": "ED5FB7B738681EE8",
            "configuration_status": "has_configuration",
            "vanguard_dependency": true,
            "launch_disabled": false,
            "secondary_patchlines": [{ "id": "game_patch", "relativePath": "Game" }]
          },
          {
            "id": "pbe",
            "install_id": "league_of_legends.pbe",
            "full_name": "League of Legends PBE",
            "install_full_path": "",
            "install_dir": "League of Legends (PBE)",
            "primary_executable": "",
            "release_id": "",
            "configuration_status": "unsupported_region",
            "vanguard_dependency": false,
            "launch_disabled": false,
            "secondary_patchlines": []
          }
        ]
      },
      { "id": "bacon", "patchlines": [] }
    ]"#;

    fn league_fixture() -> Product {
        serde_json::from_str::<Vec<Product>>(RECORDED)
            .unwrap()
            .into_iter()
            .find(|p| p.id == products::LEAGUE_OF_LEGENDS)
            .unwrap()
    }

    #[test]
    fn parses_the_recorded_response() {
        let league = league_fixture();
        assert_eq!(league.patchlines.len(), 2);

        let live = league.patchline("live").unwrap();
        assert_eq!(live.full_name, "League of Legends");
        assert_eq!(live.release_id, "ED5FB7B738681EE8");
        assert_eq!(live.primary_executable, "LeagueClient.exe");
        assert!(live.vanguard_dependency);
    }

    /// The whole reason to prefer this over `…/eligibility`, which answers
    /// `true` for a patchline that is not on the machine at all.
    #[test]
    fn an_empty_install_path_means_not_installed() {
        let league = league_fixture();
        assert!(league.patchline("live").unwrap().is_installed());
        assert!(!league.patchline("pbe").unwrap().is_installed());

        let installed: Vec<&str> = league
            .installed_patchlines()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(installed, vec!["live"]);
    }

    /// `Game` is a declared relative path. Deriving it is the point.
    #[test]
    fn secondary_dir_comes_from_the_declared_secondary_patchline() {
        let league = league_fixture();
        let live = league.patchline("live").unwrap();

        assert_eq!(
            live.secondary_dir(secondary_patchlines::GAME_PATCH)
                .unwrap(),
            PathBuf::from("C:/Riot Games/League of Legends").join("Game")
        );
        assert_eq!(
            live.install_root().unwrap(),
            PathBuf::from("C:/Riot Games/League of Legends")
        );
    }

    /// An installed patchline that declares no `game_patch` must not silently
    /// fall back to a hardcoded `Game` - that guess is exactly what this
    /// endpoint exists to remove.
    #[test]
    fn a_missing_secondary_yields_no_dir() {
        let patchline: Patchline = serde_json::from_str(
            r#"{ "id": "live", "install_full_path": "C:/Games/LoL", "secondary_patchlines": [] }"#,
        )
        .unwrap();

        assert!(patchline.is_installed());
        assert!(
            patchline
                .secondary_dir(secondary_patchlines::GAME_PATCH)
                .is_none()
        );
    }

    #[test]
    fn an_uninstalled_patchline_has_neither_path() {
        let league = league_fixture();
        let pbe = league.patchline("pbe").unwrap();
        assert!(pbe.install_root().is_none());
        assert!(
            pbe.secondary_dir(secondary_patchlines::GAME_PATCH)
                .is_none()
        );
    }

    /// The namespace is absent from swagger and already split across `v1`/`v4`,
    /// so it *will* grow and rename fields. Neither may break parsing - which is
    /// why tolerance is a generator policy rather than a per-type choice.
    #[test]
    fn tolerates_unknown_keys_and_missing_ones() {
        let products: Vec<Product> = serde_json::from_str(
            r#"[{ "id": "league_of_legends", "brand_new_key": { "nested": 1 },
                  "patchlines": [{ "id": "live", "another_new_one": true }] }]"#,
        )
        .unwrap();

        let live = products[0].patchline("live").unwrap();
        assert!(!live.is_installed());
        assert_eq!(live.release_id, "");
        assert!(live.secondary_patchlines.is_empty());
    }

    /// Insurance for the day the odd camelCase spelling is normalised.
    #[test]
    fn accepts_either_spelling_of_the_relative_path() {
        for body in [
            r#"{ "id": "game_patch", "relativePath": "Game" }"#,
            r#"{ "id": "game_patch", "relative_path": "Game" }"#,
        ] {
            let secondary: SecondaryPatchline = serde_json::from_str(body).unwrap();
            assert_eq!(secondary.relative_path, "Game");
        }
    }

    #[test]
    fn no_league_product_yields_nothing() {
        let products: Vec<Product> = serde_json::from_str(r#"[{ "id": "valorant" }]"#).unwrap();
        assert!(!products.iter().any(|p| p.id == products::LEAGUE_OF_LEGENDS));
    }

    /// The two enum fields are the whole reason `SessionExt` exists, and both
    /// have a value that reads like its opposite at a glance.
    mod sessions {
        use super::*;

        fn session(phase: &str, reason: &str) -> Session {
            serde_json::from_str(&format!(
                r#"{{ "productId": "league_of_legends", "patchlineId": "live",
                      "phase": "{phase}", "exitReason": "{reason}", "exitCode": 0 }}"#
            ))
            .unwrap()
        }

        #[test]
        fn a_live_session_is_playing_and_has_not_ended() {
            let live = session("Gameplay", "StillRunning");
            assert!(live.is_playing());
            assert!(!live.has_ended());
        }

        /// The gap between "the client accepted the launch" and "the game is
        /// up". Treating this as playing is the mistake the trait prevents.
        #[test]
        fn a_pending_session_is_neither_playing_nor_ended() {
            let pending = session("Pending", "StillRunning");
            assert!(!pending.is_playing());
            assert!(!pending.has_ended());
        }

        #[test]
        fn every_terminal_reason_ends_the_session() {
            for reason in ["Exit", "Interrupt", "Timeout", "Unknown"] {
                let over = session("None", reason);
                assert!(over.has_ended(), "{reason}");
                assert!(!over.is_playing(), "{reason}");
            }
        }

        /// An absent field must not read as a finished session - the caller
        /// asking about an id the client does not have gets `None` from the
        /// lookup, and this is the fallback if one ever slips past that.
        #[test]
        fn an_empty_reason_is_not_an_ending() {
            let empty: Session = serde_json::from_str("{}").unwrap();
            assert!(!empty.has_ended());
            assert!(!empty.is_playing());
        }

        /// The whole type is a curated subset, so the field that must never
        /// appear is worth a test rather than a comment.
        #[test]
        fn the_launch_configuration_never_lands_in_the_type() {
            let recorded = r#"{
                "productId": "league_of_legends",
                "patchlineId": "live",
                "phase": "Gameplay",
                "launchConfiguration": {
                    "arguments": ["--rso_auth.authorization-key=SECRET"]
                }
            }"#;
            let session: Session = serde_json::from_str(recorded).unwrap();
            assert_eq!(session.product_id, "league_of_legends");
            assert!(session.is_playing());
            assert!(!format!("{session:?}").contains("SECRET"));
        }
    }
}
