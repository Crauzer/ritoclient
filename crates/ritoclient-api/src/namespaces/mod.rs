//! The Riot Client's API namespaces, one module each.
//!
//! `/help` groups the client's 1261 functions into 126 namespaces, and that
//! grouping is the client's own - not a shape imposed here - so it is the one
//! this crate follows. Each module wraps one namespace and exposes a handle
//! obtained from the [`Client`](crate::Client):
//!
//! ```no_run
//! # use ritoclient_api::Client;
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
//! A module per namespace, holding that namespace's [`Route`]
//! declarations, its handler type, and the doc comments recording what was
//! measured about it against a live client. Nothing else: the transport is
//! [`crate::client`], repetition policy is [`crate::retry`], and deciding
//! *which* route to drive for a given job is orchestration - see
//! [`mod@crate::launch`] and [`crate::session`], which sit above these rather than
//! among them.
//!
//! Only the namespaces the manager actually uses are modelled. Anything else
//! the client serves is still reachable through [`Client`](crate::Client)
//! directly, so an unmodelled namespace is a missing convenience rather than a
//! wall.
//!
//! # Layout
//!
//! octocrab's handler-per-namespace scheme, made uniform:
//!
//! ```text
//! namespaces/<namespace>/
//!     mod.rs      the handler and its endpoint methods
//!     routes.rs   the Route declarations, and the helpers that bind them
//! ```
//!
//! Each `routes.rs` is one [`crate::routes!`] invocation, which declares
//! the constants and that namespace's `ALL` table from the same list.
//! [`ALL_ROUTES`] merges those tables, and [`routes`] flattens them.
//!
//! **Every namespace gets a folder, and routes always get their own file**, even
//! the ones with a single route. octocrab splits only when a module grows, which
//! suits a crate whose ~30 namespaces are all hand-written; this one is aimed at
//! the client's 126, with `routes.rs` the file a generator writes and `mod.rs`
//! the file it must never touch. A layout that changes shape at some size
//! threshold cannot be that seam, so there is no threshold.
//!
//! Routes are `pub`, being reference data in the same sense as [`crate::ids`]:
//! a caller reaching past a handler for a route we have not wrapped should not
//! have to respell it, and a namespace that publishes only its route table is
//! still more useful than none at all.
//!
//! **[`crate::models`] mirrors this tree**, so the types a namespace returns are
//! always at the matching path under `models::`.
//!
//! When an endpoint grows enough optional parameters to want a builder, it gets
//! `<namespace>/<endpoint>.rs`, the handler method returns the builder, and the
//! builder owns `send()`. None do yet.
//!
//! Handlers are named `<Namespace>Handler` rather than `<Namespace>`. The suffix
//! looks redundant at four namespaces and stops looking that way at 126: the
//! client's namespace names and its type names overlap heavily, and
//! `ProductSessionHandler` next to `models::product_session::ProductSession` is
//! the collision the suffix exists to prevent.

pub mod app_args;
pub mod lifecycle;
pub mod product_launcher;
pub mod product_registry;

use crate::route::Route;

/// Every namespace's route table, grouped as declared.
///
/// Grouped rather than flat because a `const` cannot concatenate other consts;
/// [`routes`] flattens it for the callers that do not care about the grouping.
pub const ALL_ROUTES: &[&[Route]] = &[
    app_args::routes::ALL,
    lifecycle::routes::ALL,
    product_launcher::routes::ALL,
    product_registry::routes::ALL,
];

/// Every route this crate declares.
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_namespace_lookup_spans_its_versions() {
        assert_eq!(
            routes_in("product-launcher").count(),
            product_launcher::routes::ALL.len()
        );
        assert_eq!(routes_in("no-such-namespace").count(), 0);
    }
}
