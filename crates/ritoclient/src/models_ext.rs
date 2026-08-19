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

/// What a session is doing, as `ProductSessionProductPhase` on the wire.
///
/// The generated model carries this as a `String` - that is the generator's
/// tolerance policy, and it is what stops a variant Riot adds from breaking
/// deserialization. Typing it here rather than there gets exhaustiveness
/// checking back without giving that up: an unrecognised value is
/// [`Other`](Self::Other) rather than a parse failure.
///
/// The wire spelling is on [`Session::phase`] and is what a host should forward
/// to its own frontend; this type is for deciding things in Rust.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SessionPhase {
    /// The game is up.
    Gameplay,
    /// A session that exists but is not playing.
    Idle,
    /// Spelled `None` by the client, and meaning it has nothing to say. Not the
    /// absence of a phase - that is [`Other`](Self::Other) with an empty string.
    Nothing,
    /// The client is still getting a game into this session. The state every
    /// session passes through for the first seconds after a launch is accepted.
    Pending,
    /// A spelling this build of the crate does not know, carried verbatim.
    Other(String),
}

impl SessionPhase {
    /// The client's own spelling, which round-trips through [`From`].
    pub fn as_str(&self) -> &str {
        match self {
            Self::Gameplay => "Gameplay",
            Self::Idle => "Idle",
            Self::Nothing => "None",
            Self::Pending => "Pending",
            Self::Other(raw) => raw,
        }
    }
}

impl From<&str> for SessionPhase {
    fn from(raw: &str) -> Self {
        match raw {
            "Gameplay" => Self::Gameplay,
            "Idle" => Self::Idle,
            "None" => Self::Nothing,
            "Pending" => Self::Pending,
            other => Self::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for SessionPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a session ended, as `ProductSessionTerminationReason` on the wire.
///
/// Read [`is_terminal`](Self::is_terminal) rather than matching, unless the
/// specific ending is what you are after - see its docs for why the test is a
/// negative one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TerminationReason {
    /// The session is still going. The value a live session carries.
    StillRunning,
    /// It ended normally.
    Exit,
    /// It was interrupted.
    Interrupt,
    /// It timed out.
    Timeout,
    /// The client ended it and will not say how. A real value Riot sends, not a
    /// stand-in for one we failed to parse - that is [`Other`](Self::Other).
    Unknown,
    /// A spelling this build of the crate does not know, carried verbatim. An
    /// empty string lands here too, which is what an absent field deserializes
    /// to.
    Other(String),
}

impl TerminationReason {
    /// The client's own spelling, which round-trips through [`From`].
    pub fn as_str(&self) -> &str {
        match self {
            Self::StillRunning => "StillRunning",
            Self::Exit => "Exit",
            Self::Interrupt => "Interrupt",
            Self::Timeout => "Timeout",
            Self::Unknown => "Unknown",
            Self::Other(raw) => raw,
        }
    }

    /// Whether this describes a session that is over.
    ///
    /// **A negative test on purpose.** `StillRunning` is the one value meaning
    /// "not yet", and an empty one is no evidence either way; everything else is
    /// an ending, *including a value Riot adds later*. Listing the endings
    /// instead would quietly report a future `Crashed` as a live session.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::StillRunning) && !self.as_str().is_empty()
    }
}

impl From<&str> for TerminationReason {
    fn from(raw: &str) -> Self {
        match raw {
            "StillRunning" => Self::StillRunning,
            "Exit" => Self::Exit,
            "Interrupt" => Self::Interrupt,
            "Timeout" => Self::Timeout,
            "Unknown" => Self::Unknown,
            other => Self::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Behaviour for [`Session`].
///
/// Both of the client's enum fields arrive as strings, and both have a value
/// meaning "nothing has happened yet" that is easy to mistake for its opposite:
/// a session sitting at `Pending` is not playing, and one reporting
/// `StillRunning` has not ended. These read them.
pub trait SessionExt {
    /// What the session is doing.
    fn phase(&self) -> SessionPhase;

    /// How the session ended, or that it has not.
    fn exit_reason(&self) -> TerminationReason;

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
    fn phase(&self) -> SessionPhase {
        SessionPhase::from(self.phase.as_str())
    }

    fn exit_reason(&self) -> TerminationReason {
        TerminationReason::from(self.exit_reason.as_str())
    }

    fn is_playing(&self) -> bool {
        self.phase() == SessionPhase::Gameplay
    }

    fn has_ended(&self) -> bool {
        self.exit_reason().is_terminal()
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

        /// Recorded from a live session on client 137 (EUW, 2026-08-19), with
        /// `launchConfiguration` stripped as the crate's logging rule requires.
        ///
        /// The `null` is the whole reason this fixture exists. `exitReason` is
        /// documented as a string and typed as one, and a live session sends
        /// null rather than the `StillRunning` the docs promise - so every poll
        /// failed to deserialize, `external_session` answered `None`, and the
        /// watcher spent a whole game believing there was no session to watch.
        /// A parse failure that presents as a feature quietly doing nothing is
        /// the expensive kind.
        #[test]
        fn a_live_session_reports_a_null_exit_reason() {
            let recorded = r#"{
                "exitCode": 0,
                "exitReason": null,
                "isInternal": false,
                "patchlineFullName": "League of Legends",
                "patchlineId": "live",
                "phase": "None",
                "productId": "league_of_legends",
                "version": "24C2E5A086AFFB82"
            }"#;

            let session: Session = serde_json::from_str(recorded).unwrap();

            assert_eq!(session.exit_reason, "");
            assert!(!session.has_ended(), "a null reason is not an ending");
            assert_eq!(session.product_id, "league_of_legends");
            assert_eq!(session.version, "24C2E5A086AFFB82");
        }

        /// The same null in the shape the endpoint actually returns: a map of
        /// session id to record. One unparseable entry failed the whole
        /// response, so the `host_app` session the client always keeps could
        /// take the game's down with it.
        #[test]
        fn a_null_survives_the_map_the_endpoint_returns() {
            let recorded = r#"{
                "fz7WWY9jC7WS016HZ3dtbA": { "productId": "league_of_legends", "exitReason": null },
                "host_app": { "productId": "riot_client", "patchlineId": null, "exitReason": null }
            }"#;

            let sessions: std::collections::HashMap<String, Session> =
                serde_json::from_str(recorded).unwrap();

            assert_eq!(sessions.len(), 2);
            assert!(!sessions["fz7WWY9jC7WS016HZ3dtbA"].has_ended());
            assert_eq!(sessions["host_app"].patchline_id, "");
        }

        /// The point of typing these: a `match` gets checked, and the values
        /// still round-trip to the client's own spelling.
        #[test]
        fn the_wire_spellings_round_trip() {
            for raw in ["Gameplay", "Idle", "None", "Pending"] {
                assert_eq!(SessionPhase::from(raw).as_str(), raw);
            }
            for raw in ["StillRunning", "Exit", "Interrupt", "Timeout", "Unknown"] {
                assert_eq!(TerminationReason::from(raw).as_str(), raw);
            }
        }

        /// `None` is a phase Riot sends. Renaming it to `Nothing` in Rust keeps
        /// `SessionPhase::None` from reading as an absent value at a glance,
        /// and must not change what goes back on the wire.
        #[test]
        fn the_none_phase_keeps_its_spelling() {
            assert_eq!(SessionPhase::from("None"), SessionPhase::Nothing);
            assert_eq!(SessionPhase::Nothing.to_string(), "None");
        }

        /// A value neither we nor this build of the client knows is carried,
        /// not dropped - and for a termination reason it still counts as an
        /// ending, because `StillRunning` is the only thing that does not.
        #[test]
        fn an_unrecognised_value_survives_and_still_reads_as_an_ending() {
            let phase = SessionPhase::from("Rehearsal");
            assert_eq!(phase, SessionPhase::Other("Rehearsal".to_string()));
            assert_eq!(phase.to_string(), "Rehearsal");

            let reason = TerminationReason::from("Crashed");
            assert!(reason.is_terminal());
            assert!(session("None", "Crashed").has_ended());
        }

        /// Riot's own `Unknown` is a real ending. The catch-all is `Other`, and
        /// conflating the two would make a reported ending look unparsed.
        #[test]
        fn riots_unknown_is_a_value_not_a_parse_failure() {
            assert_eq!(
                TerminationReason::from("Unknown"),
                TerminationReason::Unknown
            );
            assert!(TerminationReason::Unknown.is_terminal());
        }

        /// An absent field is not an ending, and not a phase either.
        #[test]
        fn an_empty_value_is_no_evidence() {
            assert!(!TerminationReason::from("").is_terminal());
            assert_eq!(SessionPhase::from(""), SessionPhase::Other(String::new()));
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
