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
//! Everything deserializes tolerantly - `#[serde(default)]`, unknown keys
//! ignored - because these namespaces churn between patches and a caller always
//! has a fallback. That is a generator policy, not a per-type decision.

use serde::Deserialize;

/// A product and every patchline it declares, installed or not.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RnetProductRegistryProduct {
    pub id: String,
    pub patchlines: Vec<RnetProductRegistryPatchline>,
}

/// One patchline of a product - `live`, `pbe`, and so on.
///
/// A record exists for every patchline the account is *entitled* to, which is
/// not the same as installed: the install test is `install_full_path` being
/// non-empty (the `ritoclient` crate's `PatchlineExt::is_installed`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RnetProductRegistryPatchline {
    pub id: String,
    /// `league_of_legends.live` - joins onto `/patch/v1/installs` and the
    /// ProgramData metadata directory.
    pub install_id: String,
    /// Already localized and already disambiguated ("League of Legends PBE"),
    /// so there is no need to synthesize a display name.
    pub full_name: String,
    /// The install root. Empty when the patchline is not installed.
    pub install_full_path: String,
    pub install_dir: String,
    pub primary_executable: String,
    /// The content release on disk, e.g. `ED5FB7B738681EE8`. This is the key
    /// that changes when the game patches.
    pub release_id: String,
    /// Informative only - reports `unsupported_region` for a patchline the
    /// account's region has no configuration for, which says nothing about
    /// whether it is installed.
    pub configuration_status: String,
    pub vanguard_dependency: bool,
    pub launch_disabled: bool,
    pub secondary_patchlines: Vec<RnetProductRegistrySecondaryPatchline>,
}

/// A nested install under a patchline - for League, the `Game` directory.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RnetProductRegistrySecondaryPatchline {
    pub id: String,
    /// Relative to the parent's `install_full_path`.
    ///
    /// Spelled camelCase in the response while its siblings are snake_case. The
    /// `alias` is insurance for the day that stops being true, and is the kind
    /// of judgement a schema cannot supply - it comes from the generator's
    /// override file, not from `/help`.
    #[serde(rename = "relativePath", alias = "relative_path")]
    pub relative_path: String,
}
