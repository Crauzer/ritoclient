//! Every API data type, in one flat namespace, under the client's own names.
//!
//! **Private on purpose.** This module is storage, not an API - the public
//! grouping lives in the sibling modules of [`super`], which re-export from here
//! under ergonomic names. Nothing outside `models` should name this module.
//!
//! Flat because the client's type universe is flat: `/help` reports 3966 types
//! whose names are already globally unique, and a type is routinely referenced
//! by several namespaces. Emitting each one exactly here, once, means generation
//! never has to decide which namespace *owns* a shared type - the grouping
//! modules just re-export it into every group that uses it.
//!
//! Names are Riot's, qualified. The grouping modules re-export them under short
//! ones - `RnetProductRegistryProduct` is
//! [`Product`](super::product_registry::Product) there.
//!
//! # Ownership
//!
//! This file becomes generator output. Definitions only - fields, serde
//! attributes, doc comments carried from the schema. Hand-written behaviour
//! lives in the `ritoclient` crate as extension traits, so regenerating this
//! crate can never clobber it.
//!
//! Everything deserializes tolerantly, because these namespaces churn between
//! patches and a caller always has a fallback. That is a generator policy, not a
//! per-type decision, and it takes three parts: `#[serde(default)]` for a key
//! that is absent, [`or_default`](super::tolerant::or_default) on every field
//! for one that is present and `null`, and unknown keys ignored. Only the first
//! and last come from `derive`. The middle one is the case that took the session
//! watcher out, and it costs an attribute per field.
//!
//! `Serialize` is derived too, so a host can forward one of these to its own
//! frontend without restating it. Round-tripping is not the point and is not
//! promised: a curated type is a subset, so serializing one emits the fields we
//! kept and nothing else.

use serde::{Deserialize, Serialize};

use super::tolerant::or_default;

/// One session the client is tracking: what is running, and how it ended.
///
/// **`launchConfiguration` is deliberately absent.** It carries the game's argv,
/// and inside that argv is an `rso_auth.authorization-key` - a credential the
/// client mints per session. The crate's rule is that a session payload is
/// stripped of it before it goes anywhere, and leaving the field out of the type
/// makes that structural rather than something to remember. Reading argv back is
/// a separate decision, and it comes with a redaction policy attached.
///
/// `phase` and `exit_reason` are enums on the wire (`Gameplay` / `Idle` / `None`
/// / `Pending`, and `Exit` / `Interrupt` / `StillRunning` / `Timeout` /
/// `Unknown`). They travel as `String` under the generator's tolerance policy, so
/// a variant Riot adds does not break deserialization.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ProductSessionSession {
    #[serde(rename = "productId", deserialize_with = "or_default")]
    pub product_id: String,
    #[serde(rename = "patchlineId", deserialize_with = "or_default")]
    pub patchline_id: String,
    #[serde(rename = "patchlineFullName", deserialize_with = "or_default")]
    pub patchline_full_name: String,
    /// The content release the session is running, e.g. `24C2E5A086AFFB82` - the
    /// same shape as [`release_id`](RnetProductRegistryPatchline::release_id),
    /// not the patch number a player would recognise. Worth saying, because
    /// "version" invites putting it on screen.
    #[serde(deserialize_with = "or_default")]
    pub version: String,
    /// What the session is doing. `Gameplay` while a match is running, `Pending`
    /// while the client is still getting one there, `Idle` for a session that
    /// exists but is not playing.
    ///
    /// **Not a test for "the game is up".** Recorded from client 137: with
    /// `LeagueClient.exe` running and the player sitting in the client, this
    /// reads `None` - the client saying it has nothing to report, which is a
    /// different fact from the process being absent. Ask the process table
    /// for that one.
    #[serde(deserialize_with = "or_default")]
    pub phase: String,
    /// Meaningful only once the session has ended - `exit_reason` is what says
    /// whether it has.
    #[serde(rename = "exitCode", deserialize_with = "or_default")]
    pub exit_code: i64,
    /// How the session ended: `Exit` for a normal one, `Interrupt` / `Timeout` /
    /// `Unknown` for the rest. This is the field that answers "is it over?" - a
    /// game that died on startup gives a reason here instead of just vanishing
    /// from the process table.
    ///
    /// **A live session reports `null`**, recorded from client 137, which is one
    /// of the reasons every field here tolerates one. `StillRunning` is a
    /// documented value for the same state, so assume neither spelling and read
    /// it through the `ritoclient` crate's `SessionExt::has_ended` instead of
    /// comparing.
    #[serde(rename = "exitReason", deserialize_with = "or_default")]
    pub exit_reason: String,
}

/// A product and every patchline it declares, installed or not.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RnetProductRegistryProduct {
    #[serde(deserialize_with = "or_default")]
    pub id: String,
    #[serde(deserialize_with = "or_default")]
    pub patchlines: Vec<RnetProductRegistryPatchline>,
}

/// One patchline of a product - `live`, `pbe`, and so on.
///
/// A record exists for every patchline the account is *entitled* to, which is
/// not the same as installed: the install test is `install_full_path` being
/// non-empty (the `ritoclient` crate's `PatchlineExt::is_installed`).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RnetProductRegistryPatchline {
    #[serde(deserialize_with = "or_default")]
    pub id: String,
    /// `league_of_legends.live` - joins onto `/patch/v1/installs` and the
    /// ProgramData metadata directory.
    #[serde(deserialize_with = "or_default")]
    pub install_id: String,
    /// Already localized and already disambiguated ("League of Legends PBE"),
    /// so there is no need to synthesize a display name.
    #[serde(deserialize_with = "or_default")]
    pub full_name: String,
    /// The install root. Empty when the patchline is not installed.
    #[serde(deserialize_with = "or_default")]
    pub install_full_path: String,
    #[serde(deserialize_with = "or_default")]
    pub install_dir: String,
    #[serde(deserialize_with = "or_default")]
    pub primary_executable: String,
    /// The content release on disk, e.g. `ED5FB7B738681EE8`. This is the key
    /// that changes when the game patches.
    #[serde(deserialize_with = "or_default")]
    pub release_id: String,
    /// Informative only - reports `unsupported_region` for a patchline the
    /// account's region has no configuration for, which says nothing about
    /// whether it is installed.
    #[serde(deserialize_with = "or_default")]
    pub configuration_status: String,
    #[serde(deserialize_with = "or_default")]
    pub vanguard_dependency: bool,
    #[serde(deserialize_with = "or_default")]
    pub launch_disabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub secondary_patchlines: Vec<RnetProductRegistrySecondaryPatchline>,
}

/// A nested install under a patchline - for League, the `Game` directory.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RnetProductRegistrySecondaryPatchline {
    #[serde(deserialize_with = "or_default")]
    pub id: String,
    /// Relative to the parent's `install_full_path`.
    ///
    /// Spelled camelCase in the response while its siblings are snake_case. The
    /// `alias` is insurance for the day that stops being true, and is the kind
    /// of judgement a schema cannot supply - it comes from the generator's
    /// override file, not from `/help`.
    #[serde(
        rename = "relativePath",
        alias = "relative_path",
        deserialize_with = "or_default"
    )]
    pub relative_path: String,
}
