//! The Riot Client's API namespaces, one module each.
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

pub mod app_args;
pub mod lifecycle;
pub mod product_launcher;
pub mod product_registry;

use ritoclient_core::client::Client;
use ritoclient_core::endpoint::EndpointMeta;
use ritoclient_core::route::Route;

use app_args::AppArgsHandler;
use lifecycle::LifecycleHandler;
use product_launcher::ProductLauncherHandler;
use product_registry::ProductRegistryHandler;

/// Hangs a handler per namespace off a [`Client`].
///
/// An extension trait rather than inherent methods because an inherent `impl`
/// must live in the crate defining the type, and `Client` lives below this one -
/// which is exactly right: the transport does not know the generated layer
/// exists. One `use ritoclient_api::ClientExt;` (or the facade's prelude) puts
/// the accessors back on the client.
pub trait ClientExt {
    /// `/product-launcher/v1` - starting a product.
    fn product_launcher(&self) -> ProductLauncherHandler<'_>;

    /// `/rnet-product-registry/v4` - what is installed, and where.
    fn product_registry(&self) -> ProductRegistryHandler<'_>;

    /// `/riot-client-lifecycle/v1` - the client's own window.
    fn lifecycle(&self) -> LifecycleHandler<'_>;

    /// `/riotclientapp/v1` - the argv handoff.
    fn app_args(&self) -> AppArgsHandler<'_>;
}

impl ClientExt for Client {
    fn product_launcher(&self) -> ProductLauncherHandler<'_> {
        ProductLauncherHandler::new(self)
    }

    fn product_registry(&self) -> ProductRegistryHandler<'_> {
        ProductRegistryHandler::new(self)
    }

    fn lifecycle(&self) -> LifecycleHandler<'_> {
        LifecycleHandler::new(self)
    }

    fn app_args(&self) -> AppArgsHandler<'_> {
        AppArgsHandler::new(self)
    }
}

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

/// Every namespace's endpoint table, grouped as declared.
///
/// Grouped for the same reason as [`ALL_ROUTES`]; [`endpoints`] flattens it.
pub const ALL_ENDPOINTS: &[&[EndpointMeta]] = &[
    app_args::endpoints::ALL,
    lifecycle::endpoints::ALL,
    product_launcher::endpoints::ALL,
    product_registry::endpoints::ALL,
];

/// Every endpoint this crate declares.
///
/// The operation-level answer to "what do we cover?". Unlike [`routes`], each
/// row carries the verb, so a walker knows what it is probing - a 405 from a
/// GET on a declared-POST row confirms the gate instead of having to be
/// guessed at. Whether a row is live on a particular client is
/// [`Client::probe`]'s question, not the table's.
pub fn endpoints() -> impl Iterator<Item = EndpointMeta> + Clone {
    ALL_ENDPOINTS.iter().copied().flatten().copied()
}

#[cfg(test)]
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

    #[test]
    fn a_namespace_lookup_spans_its_versions() {
        assert_eq!(
            routes_in("product-launcher").count(),
            product_launcher::routes::ALL.len()
        );
        assert_eq!(routes_in("no-such-namespace").count(), 0);
    }

    /// The drift a two-table layout invites: an endpoint whose route was never
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

    /// The endpoint tables are hand-maintained until the generator writes
    /// them, so the consts they mirror are asserted against the real impls.
    #[test]
    fn the_metadata_rows_match_their_endpoint_impls() {
        fn meta_for(name: &str) -> EndpointMeta {
            endpoints()
                .find(|meta| meta.name == name)
                .unwrap_or_else(|| panic!("no EndpointMeta row named {name}"))
        }

        fn assert_row<E: Endpoint>(name: &str) {
            let meta = meta_for(name);
            assert_eq!(meta.method, E::METHOD, "{name}");
            assert_eq!(meta.route, E::ROUTE, "{name}");
        }

        assert_row::<app_args::endpoints::NewArgs>("NewArgs");
        assert_row::<lifecycle::endpoints::Hide>("Hide");
        assert_row::<lifecycle::endpoints::Show>("Show");
        assert_row::<product_launcher::endpoints::Patchline>("Patchline");
        assert_row::<product_launcher::endpoints::Eligibility>("Eligibility");
        assert_row::<product_launcher::endpoints::IsLaunchRequestPending>("IsLaunchRequestPending");
        assert_row::<product_registry::endpoints::Products>("Products");
    }
}
