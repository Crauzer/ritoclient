//! The fixed text of the generated crate - everything that is the same
//! regardless of what the schema says.
//!
//! Prose that names a namespace (the worked examples in the crate and module
//! docs) lives here too: it is part of the generator's editorial voice, and if
//! a regeneration drops the namespace an example uses, the doctest fails
//! loudly rather than silently shipping a broken example.

/// `crates/ritoclient-api/src/lib.rs`, whole.
pub const LIB_RS: &str = r#"//! Typed namespaces and models for the Riot Client's local API.
//!
//! This crate is the generated layer between `ritoclient-core` (the transport)
//! and `ritoclient` (the launch orchestration and facade). It wraps the API
//! namespaces the workspace models - per namespace a handler, a route table,
//! and an endpoint table, plus the data types they carry - and nothing else:
//! no loops, no sleeps, no OS calls, and no opinion about what a status
//! *means*, because all of those are judgements the layers above make.
//!
//! Every operation is a value implementing
//! [`Endpoint`](ritoclient_core::Endpoint); the handlers are thin sugar that
//! construct the endpoint and pick a finisher. [`namespaces::endpoints`]
//! enumerates them all as plain data, verb included.
//!
//! Handlers are reached through [`ClientExt`], which hangs them off a
//! [`Client`](ritoclient_core::Client):
//!
//! ```no_run
//! use ritoclient_api::ClientExt;
//! use ritoclient_core::Client;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Client::new()?;
//! let products = client.product_registry().products();
//! # Ok(())
//! # }
//! ```
//!
//! # Destined to be generator output
//!
//! Every file in this crate other than `Cargo.toml` is slated to be written by
//! `cargo xtask ritoclient-codegen` from a checked-in schema snapshot, and the
//! hand-written namespaces here are the shape that generator must reproduce.
//! The dependency list is the boundary's enforcement: nothing here can reach a
//! launcher type, a sleep, or Win32, because no dependency provides one.

// Several modules explain their own privacy - `models::flat` is private on
// purpose, and the prose says so - and naming the private item is the clearest
// way to write that. Rustdoc renders these as plain code rather than links,
// which is the intended result.
#![allow(rustdoc::private_intra_doc_links)]

pub mod models;
pub mod namespaces;

pub use namespaces::ClientExt;
"#;

/// The `//!` header of `namespaces/mod.rs`.
pub const NAMESPACES_HEADER: &str = r#"//! The Riot Client's API namespaces, one module each.
//!
//! `/help` groups the client's 1261 functions into 126 namespaces, and that
//! grouping is the client's own - not a shape imposed here - so it is the one
//! this crate follows. Each module wraps one namespace and exposes a handle
//! obtained from a [`Client`] through [`ClientExt`]:
//!
//! ```no_run
//! use ritoclient_api::ClientExt;
//! use ritoclient_core::Client;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Client::new()?;
//!
//! let installed = client.product_registry().products();
//! client.lifecycle().hide()?;
//! # Ok(())
//! # }
//! ```
//!
//! # What belongs here
//!
//! A module per namespace, holding that namespace's [`Route`] declarations, its
//! handler type, and the doc comments recording what was measured about it
//! against a live client. Nothing else: the transport is
//! [`ritoclient_core::client`], repetition policy is [`ritoclient_core::retry`],
//! and deciding *which* route to drive for a given job is orchestration - it
//! lives in the `ritoclient` crate, above this one, which is also where anything
//! that loops, sleeps, calls the OS, or judges what a status means belongs.
//! This crate's dependency list cannot express any of those, on purpose.
//!
//! Only the namespaces the workspace actually uses are modelled. Anything else
//! the client serves is still reachable through [`Client`] directly, so an
//! unmodelled namespace is a missing convenience rather than a wall.
//!
//! # Layout
//!
//! ```text
//! namespaces/<namespace>/
//!     mod.rs         the handler and its endpoint methods
//!     routes.rs      the Route declarations
//!     endpoints.rs   the endpoint types and the namespace's EndpointMeta table
//! ```
//!
//! Each `routes.rs` is one [`ritoclient_core::routes!`] invocation, which
//! declares the constants and that namespace's `ALL` table from the same list.
//! [`ALL_ROUTES`] merges those tables, and [`routes`] flattens them.
//!
//! Each `endpoints.rs` declares the namespace's
//! [`Endpoint`](ritoclient_core::Endpoint) types - the operations, where the
//! verb is data rather than prose - and its own
//! `ALL: &[EndpointMeta]` table beside them. [`ALL_ENDPOINTS`] merges those,
//! and [`endpoints`] flattens them. Endpoints live apart from `routes.rs`
//! because several endpoints can share one route, and apart from `mod.rs`
//! because the handler reads better without the struct definitions above it -
//! legibility, not protection.
//!
//! **Every namespace gets a folder, and routes always get their own file**, even
//! the ones with a single route. This crate is aimed at the client's 126
//! namespaces and is slated to be written whole by a generator; a layout that
//! changes shape at some size threshold cannot be a generator target, so there
//! is no threshold.
//!
//! Routes are `pub`, being reference data: a caller reaching past a handler for
//! a route we have not wrapped should not have to respell it, and a namespace
//! that publishes only its route table is still more useful than none at all.
//!
//! **[`crate::models`] mirrors this tree**, so the types a namespace returns are
//! always at the matching path under `models::`.
//!
//! Handlers are named `<Namespace>Handler` rather than `<Namespace>`. The suffix
//! looks redundant at four namespaces and stops looking that way at 126: the
//! client's namespace names and its type names overlap heavily, and
//! `ProductSessionHandler` next to `models::product_session::ProductSession` is
//! the collision the suffix exists to prevent.
"#;

/// The doc block on the `ClientExt` trait.
pub const CLIENT_EXT_DOC: &str = r#"/// Hangs a handler per namespace off a [`Client`].
///
/// An extension trait rather than inherent methods because an inherent `impl`
/// must live in the crate defining the type, and `Client` lives below this one -
/// which is exactly right: the transport does not know the generated layer
/// exists. One `use ritoclient_api::ClientExt;` (or the facade's prelude) puts
/// the accessors back on the client.
"#;

/// The doc block on `ALL_ROUTES`.
pub const ALL_ROUTES_DOC: &str = r#"/// Every namespace's route table, grouped as declared.
///
/// Grouped rather than flat because a `const` cannot concatenate other consts;
/// [`routes`] flattens it for the callers that do not care about the grouping.
"#;

/// `routes()` and `routes_in()`, docs and bodies.
pub const ROUTES_FNS: &str = r#"/// Every route this crate declares.
///
/// The crate's own answer to "what do we cover?" - what a drift check compares
/// against a schema snapshot, and what `examples/probe.rs` walks.
///
/// ```
/// // Which namespaces are modelled, and how much of each.
/// let mut namespaces: Vec<_> = ritoclient_api::namespaces::routes()
///     .map(|route| route.namespace())
///     .collect();
/// namespaces.sort_unstable();
/// namespaces.dedup();
///
/// assert!(namespaces.contains(&"product-launcher"));
/// ```
pub fn routes() -> impl Iterator<Item = Route> + Clone {
    ALL_ROUTES.iter().copied().flatten().copied()
}

/// Every route declared under one namespace, across versions.
///
/// A namespace serves several versions at a time, so this is the honest way to
/// ask "what does `rnet-product-registry` expose?" - the answer spans `v1` and
/// `v4`.
pub fn routes_in(namespace: &str) -> impl Iterator<Item = Route> + '_ {
    routes().filter(move |route| route.namespace() == namespace)
}
"#;

/// The doc block on `ALL_ENDPOINTS`.
pub const ALL_ENDPOINTS_DOC: &str = r#"/// Every namespace's endpoint table, grouped as declared.
///
/// Grouped for the same reason as [`ALL_ROUTES`]; [`endpoints`] flattens it.
"#;

/// `endpoints()`, doc and body.
pub const ENDPOINTS_FN: &str = r#"/// Every endpoint this crate declares.
///
/// The operation-level answer to "what do we cover?". Unlike [`routes`], each
/// row carries the verb, so a walker knows what it is probing - a 405 from a
/// GET on a declared-POST row confirms the gate instead of having to be
/// guessed at. Whether a row is live on a particular client is
/// [`Client::probe`]'s question, not the table's.
pub fn endpoints() -> impl Iterator<Item = EndpointMeta> + Clone {
    ALL_ENDPOINTS.iter().copied().flatten().copied()
}
"#;

/// The `//!` header of `models/mod.rs`.
pub const MODELS_MOD_HEADER: &str = r#"//! The data the Riot Client's API carries.
//!
//! Split in two:
//!
//! - [`flat`] is **storage** - private, one flat namespace, every type under the
//!   client's own qualified name. It is generator output.
//! - The modules beside it are the **API** - public, grouped by the namespace
//!   that uses the type, re-exporting from `flat` under ergonomic names.
//!
//! So `models::product_registry::Product` is the public name of
//! `flat::RnetProductRegistryProduct`, and a type used by three namespaces is
//! defined once and re-exported three times rather than duplicated or arbitrarily
//! assigned an owner.
//!
//! Behaviour does not live in this crate at all. The methods that turn a wire
//! record into something worth calling - `is_installed`, `secondary_dir` - are
//! extension traits in the `ritoclient` crate, re-exported from its prelude: an
//! inherent `impl` must live in the crate defining the type, and hand-written
//! code does not belong in this one.
//!
//! Types that belong to no namespace in particular live in
//! [`ritoclient_core::types`].
"#;

/// The `//!` header of `models/flat.rs`, plus its one import.
pub const FLAT_HEADER: &str = r#"//! Every API data type, in one flat namespace, under the client's own names.
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
//!
//! `Serialize` is derived too, so a host can forward one of these to its own
//! frontend without restating it. Round-tripping is not the point and is not
//! promised: a curated type is a subset, so serializing one emits the fields we
//! kept and nothing else.

use serde::{Deserialize, Serialize};
"#;

/// The fixed part of the tests module in `namespaces/mod.rs`, up to the two
/// derived tests. `{}`-free so it can be spliced verbatim.
pub const NAMESPACES_TESTS_HEAD: &str = r#"#[cfg(test)]
mod tests {
    use super::*;

    use ritoclient_core::Endpoint;

    /// Two routes rendering the same path means one of them is a copy-paste
    /// that was never corrected - the mistake a generated table makes most.
    #[test]
    fn no_two_routes_render_the_same_path() {
        let mut paths: Vec<String> = routes().map(|route| route.path()).collect();
        let declared = paths.len();

        paths.sort_unstable();
        paths.dedup();

        assert_eq!(paths.len(), declared, "duplicate route path");
    }

    /// The client spells its namespaces in lowercase kebab-case, and a route
    /// whose namespace has a stray slash or capital would 404 in a way that
    /// looks like the resource is wrong.
    #[test]
    fn every_namespace_is_spelled_the_way_the_client_spells_them() {
        for route in routes() {
            let namespace = route.namespace();
            assert!(!namespace.is_empty());
            assert!(
                namespace
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{namespace}"
            );
        }
    }
"#;

/// The fixed middle of the tests module, between the two derived tests.
pub const NAMESPACES_TESTS_MIDDLE: &str = r#"    /// The drift a two-table layout invites: an endpoint whose route was never
    /// declared in `routes.rs`, or was removed from it.
    #[test]
    fn every_endpoint_route_is_declared_in_the_route_table() {
        for meta in endpoints() {
            assert!(
                routes().any(|route| route == meta.route),
                "endpoint {} drives {}, which is missing from ALL_ROUTES",
                meta.name,
                meta.route
            );
        }
    }

    /// Two rows with the same verb and path is the copy-paste mistake a
    /// metadata table makes most; sharing a *route* across verbs is fine.
    #[test]
    fn no_two_endpoints_share_a_verb_and_path() {
        let mut operations: Vec<String> = endpoints()
            .map(|meta| format!("{} {}", meta.method, meta.route))
            .collect();
        let declared = operations.len();

        operations.sort_unstable();
        operations.dedup();

        assert_eq!(operations.len(), declared, "duplicate endpoint operation");
    }
"#;
