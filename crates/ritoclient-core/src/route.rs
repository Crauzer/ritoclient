//! Versioned routes on the Riot Client's remoting server.
//!
//! Routes are declared as structured data rather than assembled from string
//! literals, because the part most likely to break is the part a literal hides:
//! the version.
//!
//! # Version belongs to the route, not the namespace
//!
//! A namespace serves several versions at once. On a live client
//! `/rnet-product-registry/v1/install-states` and
//! `/rnet-product-registry/v4/products` are both current - different resources,
//! different versions, same namespace. `patch-proxy` (v1/v2) and
//! `product-session` (v1/v2) are split the same way. So there is no such thing
//! as "the version of `rnet-product-registry`", and any model that tried to
//! store one would be wrong the first time it met a real client.
//!
//! # Nothing is negotiated
//!
//! A [`Route`] names exactly one version and the client asks for exactly that.
//! There is no fallback list and no probing: if Riot retires a version, the
//! route 404s and the caller decides what that means, in keeping with how every
//! other status is handled here. Drift is caught by re-taking the schema
//! snapshot and reading the diff, not by guessing at runtime.

use std::fmt;

/// A versioned route: `/{namespace}/v{version}/{resource}`.
///
/// `resource` may carry `{name}` placeholders, filled by [`Route::bind`]:
///
/// ```
/// use ritoclient_core::route::Route;
///
/// const LAUNCH: Route = Route::new(
///     "product-launcher",
///     1,
///     "products/{productId}/patchlines/{patchlineId}",
/// );
///
/// assert_eq!(
///     LAUNCH.bind(&[("productId", "league_of_legends"), ("patchlineId", "live")]),
///     "/product-launcher/v1/products/league_of_legends/patchlines/live"
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    namespace: &'static str,
    version: u32,
    resource: &'static str,
}

impl Route {
    pub const fn new(namespace: &'static str, version: u32, resource: &'static str) -> Self {
        Self {
            namespace,
            version,
            resource,
        }
    }

    pub const fn namespace(&self) -> &'static str {
        self.namespace
    }

    pub const fn version(&self) -> u32 {
        self.version
    }

    /// The resource, still carrying any `{name}` placeholders.
    pub const fn resource(&self) -> &'static str {
        self.resource
    }

    /// The rendered path, placeholders left as they are.
    ///
    /// Use [`Route::bind`] for a route that takes parameters - this is for the
    /// ones that don't.
    pub fn path(&self) -> String {
        let resource = self.resource.trim_matches('/');
        if resource.is_empty() {
            format!("/{}/v{}", self.namespace, self.version)
        } else {
            format!("/{}/v{}/{resource}", self.namespace, self.version)
        }
    }

    /// The rendered path with `{name}` placeholders substituted.
    ///
    /// Values are percent-encoded for the characters that would otherwise
    /// change which route is addressed - a `/` in an id would silently point the
    /// request somewhere else rather than fail.
    ///
    /// Debug builds assert that nothing was left unsubstituted, which is what
    /// catches a misspelled parameter name; release builds send the path as-is
    /// and let the client answer 404.
    pub fn bind(&self, params: &[(&str, &str)]) -> String {
        let mut path = self.path();
        for (name, value) in params {
            path = path.replace(&format!("{{{name}}}"), &encode_segment(value));
        }

        debug_assert!(
            !path.contains('{'),
            "unsubstituted path parameter in {path} - check the names against {:?}",
            self.resource
        );
        path
    }
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.path())
    }
}

/// Declare a namespace's routes and its `ALL` table together.
///
/// Each entry expands to one `pub const`, and the table is assembled from the
/// same list - so a route cannot be declared and left out of it, which is the
/// only way a hand-written table ever goes wrong.
///
/// The namespace is written once; the version is written per route, because a
/// namespace serves several at a time (see the module docs).
///
/// ```
/// use ritoclient_core::{Route, routes};
///
/// mod lifecycle {
///     ritoclient_core::routes! {
///         namespace = "riot-client-lifecycle";
///
///         /// `POST` - "Hide the UX."
///         HIDE = 1, "hide";
///
///         /// `POST` - "Show the UX."
///         SHOW = 1, "show";
///     }
/// }
///
/// assert_eq!(lifecycle::HIDE.path(), "/riot-client-lifecycle/v1/hide");
/// assert_eq!(lifecycle::ALL, &[lifecycle::HIDE, lifecycle::SHOW]);
/// ```
#[macro_export]
macro_rules! routes {
    (
        namespace = $namespace:literal;
        $(
            $(#[$meta:meta])*
            $name:ident = $version:literal, $resource:literal;
        )*
    ) => {
        $(
            $(#[$meta])*
            pub const $name: $crate::route::Route =
                $crate::route::Route::new($namespace, $version, $resource);
        )*

        /// Every route this namespace declares, in declaration order.
        pub const ALL: &[$crate::route::Route] = &[$($name),*];
    };
}

impl From<Route> for String {
    fn from(route: Route) -> Self {
        route.path()
    }
}

/// Percent-encode the characters that would change which route is addressed.
///
/// Not a general URL encoder: the ids that reach here are things like
/// `league_of_legends.live`, and over-encoding them would break paths the client
/// accepts today.
fn encode_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '/' => out.push_str("%2F"),
            '?' => out.push_str("%3F"),
            '#' => out.push_str("%23"),
            '%' => out.push_str("%25"),
            ' ' => out.push_str("%20"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HIDE: Route = Route::new("riot-client-lifecycle", 1, "hide");
    const LAUNCH: Route = Route::new(
        "product-launcher",
        1,
        "products/{productId}/patchlines/{patchlineId}",
    );

    #[test]
    fn renders_namespace_version_and_resource() {
        assert_eq!(HIDE.path(), "/riot-client-lifecycle/v1/hide");
        assert_eq!(HIDE.namespace(), "riot-client-lifecycle");
        assert_eq!(HIDE.version(), 1);
    }

    #[test]
    fn binds_named_parameters() {
        assert_eq!(
            LAUNCH.bind(&[("productId", "league_of_legends"), ("patchlineId", "live")]),
            "/product-launcher/v1/products/league_of_legends/patchlines/live"
        );
    }

    /// The same namespace at two versions is the normal case on a live client,
    /// which is the whole reason version is stored per route.
    #[test]
    fn one_namespace_can_carry_several_versions() {
        const INSTALL_STATES: Route = Route::new("rnet-product-registry", 1, "install-states");
        const PRODUCTS: Route = Route::new("rnet-product-registry", 4, "products");

        assert_eq!(
            INSTALL_STATES.path(),
            "/rnet-product-registry/v1/install-states"
        );
        assert_eq!(PRODUCTS.path(), "/rnet-product-registry/v4/products");
        assert_eq!(INSTALL_STATES.namespace(), PRODUCTS.namespace());
        assert_ne!(INSTALL_STATES.version(), PRODUCTS.version());
    }

    /// A `/` in a value would otherwise address a different route entirely, and
    /// do it silently.
    #[test]
    fn a_separator_in_a_value_cannot_escape_its_segment() {
        assert_eq!(
            LAUNCH.bind(&[("productId", "a/../b"), ("patchlineId", "live")]),
            "/product-launcher/v1/products/a%2F..%2Fb/patchlines/live"
        );
    }

    #[test]
    fn ordinary_ids_are_left_alone() {
        assert_eq!(
            encode_segment("league_of_legends.live"),
            "league_of_legends.live"
        );
        assert_eq!(encode_segment("ED5FB7B738681EE8"), "ED5FB7B738681EE8");
    }

    #[test]
    fn a_resourceless_route_is_just_the_namespace() {
        assert_eq!(Route::new("patch", 1, "").path(), "/patch/v1");
    }

    #[test]
    fn converts_into_a_path_string() {
        assert_eq!(String::from(HIDE), "/riot-client-lifecycle/v1/hide");
        assert_eq!(HIDE.to_string(), "/riot-client-lifecycle/v1/hide");
    }
}
