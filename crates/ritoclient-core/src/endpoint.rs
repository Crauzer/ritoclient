//! Endpoints as values: the operation layer above [`Route`].
//!
//! A [`Route`] is path identity - namespace, version, resource - and
//! deliberately *not* an operation: one path can serve several verbs
//! (`data-store` reads and writes product settings through the same resource),
//! so a verb on `Route` would force either a duplicate route or a route that
//! lies. An [`Endpoint`] adds what the route leaves out: the verb, the
//! parameters that bind the path, the body, and what a successful answer
//! decodes into.
//!
//! Before this layer the verb lived in prose - ``/// `POST /.../hide` `` -
//! where nothing could read it. Here it is data, so a probe can ask a route
//! honestly instead of GETting a POST-only path to read the 405.
//!
//! # There is no `dyn Endpoint`
//!
//! The associated `Output` type and consts make the trait object-unsafe on
//! purpose. Enumeration - the drift check, the probe walker - goes through the
//! plain-data endpoint tables in the generated crate, not through trait
//! objects.

use std::marker::PhantomData;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::client::{Client, Method, RequestBuilder, RequestError, Response};
use crate::retry::RetryPolicy;
use crate::route::Route;

/// Serialize a value as a JSON request body, for [`Endpoint::body`] impls.
///
/// Kept here rather than duplicated per endpoint so the crate that declares
/// endpoints needs no JSON dependency of its own. The client's argument
/// convention applies, same as [`RequestBuilder::json`]: a function taking a
/// single non-path argument wants the body to **be** that value, not an object
/// wrapping it - `new-args` wants a bare `["--flag"]`.
pub fn json_body<T: Serialize + ?Sized>(value: &T) -> Result<Option<String>, RequestError> {
    serde_json::to_string(value)
        .map(Some)
        .map_err(|e| RequestError::Encode(e.to_string()))
}

/// One operation on the Riot Client's remoting server.
///
/// `METHOD` and `ROUTE` are associated consts rather than methods because the
/// verb and the path are properties of the *operation*, not of an instance - a
/// caller should be able to read them without building one.
///
/// Three shapes cover the whole surface:
///
/// ```
/// use ritoclient_core::{Endpoint, Method, RequestError, Route};
///
/// // No parameters. The default `path()` is the whole implementation.
/// struct IsLaunchRequestPending;
///
/// impl Endpoint for IsLaunchRequestPending {
///     type Output = bool;
///     const METHOD: Method = Method::Get;
///     const ROUTE: Route = Route::new("product-launcher", 1, "is-launch-request-pending");
/// }
///
/// // Path parameters. Fields are the parameters; `path()` binds them.
/// // Fields are borrowed: an endpoint that allocated to be constructed would
/// // make this layer cost something the handler methods above it do not.
/// struct Patchline<'a> {
///     product_id: &'a str,
///     patchline_id: &'a str,
/// }
///
/// impl Endpoint for Patchline<'_> {
///     type Output = String;
///     const METHOD: Method = Method::Post;
///     const ROUTE: Route =
///         Route::new("product-launcher", 1, "products/{productId}/patchlines/{patchlineId}");
///
///     fn path(&self) -> String {
///         Self::ROUTE.bind(&[
///             ("productId", self.product_id),
///             ("patchlineId", self.patchline_id),
///         ])
///     }
///
///     fn body(&self) -> Result<Option<String>, RequestError> {
///         Ok(Some("{}".to_string()))
///     }
/// }
///
/// // The third shape - a serialized body - is `body()` returning
/// // `serde_json::to_string(..)` mapped into `RequestError::Encode`.
/// ```
pub trait Endpoint {
    /// What a successful body deserializes into.
    ///
    /// `()` for the routes that answer 204 - finish those with
    /// [`EndpointBuilder::ignore`], because an empty body is not valid JSON
    /// even for `()`.
    type Output: DeserializeOwned;

    /// The verb.
    const METHOD: Method;

    /// The path identity this operation drives.
    const ROUTE: Route;

    /// The path, with any parameters bound.
    ///
    /// The default renders the route as-is; endpoints with path parameters
    /// override this with [`Route::bind`].
    fn path(&self) -> String {
        Self::ROUTE.path()
    }

    /// The request body, serialized.
    ///
    /// `Ok(None)` sends no body. An `Err` is deferred, surfacing from the
    /// finisher on [`EndpointBuilder`] - construction stays chainable.
    fn body(&self) -> Result<Option<String>, RequestError> {
        Ok(None)
    }
}

/// One row of an endpoint table: the plain-data face of an [`Endpoint`] impl.
///
/// There is no `dyn Endpoint`, so anything that wants "every endpoint" - the
/// drift check, the probe walker - reads these tables instead. Each generated
/// `endpoints.rs` declares its namespace's `ALL: &[EndpointMeta]` alongside
/// the endpoint types themselves.
///
/// A row says what the crate declares, not what a client serves: existence
/// varies per client build and boot state, so it is asked at runtime -
/// [`Client::probe`] - rather than recorded here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointMeta {
    /// The endpoint type's name, as declared in its namespace.
    pub name: &'static str,
    pub method: Method,
    pub route: Route,
}

impl Client {
    /// Drive an endpoint through the one request pipeline.
    ///
    /// The endpoint is read eagerly - path and body are extracted here - and
    /// the returned builder wraps [`RequestBuilder`], so there is still exactly
    /// one retry loop, one lockfile-per-attempt rule, and one place that knows
    /// the port moves when the client wakes.
    pub fn endpoint<E: Endpoint>(&self, endpoint: &E) -> EndpointBuilder<'_, E> {
        let path = endpoint.path();
        let request = match endpoint.body() {
            Ok(Some(body)) => Ok(self.request(E::METHOD, &path).body(body)),
            Ok(None) => Ok(self.request(E::METHOD, &path)),
            Err(e) => Err(e),
        };

        EndpointBuilder {
            path,
            request,
            _endpoint: PhantomData,
        }
    }
}

/// A request built from an [`Endpoint`], ready to finish.
///
/// The finishers differ only in what they make of the answer:
/// [`send`](Self::send) hands back the response, status and all;
/// [`json`](Self::json) decodes a successful body; [`ok`](Self::ok) is the
/// read-only convention with every failure collapsed to `None`;
/// [`ignore`](Self::ignore) is for the routes that answer 204 with no body.
pub struct EndpointBuilder<'a, E: Endpoint> {
    /// Kept for diagnostics: the finishers log against the bound path.
    path: String,
    /// `Err` when the endpoint's body would not serialize, so construction
    /// stays chainable; every finisher surfaces it.
    request: Result<RequestBuilder<'a>, RequestError>,
    _endpoint: PhantomData<E>,
}

impl<E: Endpoint> EndpointBuilder<'_, E> {
    /// Override the timeout this request inherits from the client.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.request = self.request.map(|r| r.timeout(timeout));
        self
    }

    /// Override the retry policy this request inherits from the client.
    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.request = self.request.map(|r| r.retry(policy));
        self
    }

    /// The response, status and all. A status is data, so this is the honest
    /// finisher - reach for it wherever the caller reads statuses itself.
    pub fn send(self) -> Result<Response, RequestError> {
        self.request?.send()
    }

    /// A successful body, decoded into [`Endpoint::Output`].
    ///
    /// A non-success status is [`RequestError::Decode`] rather than a distinct
    /// variant, exactly as in [`RequestBuilder::send_json`] - this finisher is
    /// for routes whose failure the caller has already decided not to inspect.
    ///
    /// Named collision, kept on purpose: [`RequestBuilder::json`] *sets* a
    /// request body, this *decodes* a response. Each follows its own layer's
    /// convention.
    pub fn json(self) -> Result<E::Output, RequestError> {
        self.request?.send_json()
    }

    /// The read-only convention: every failure collapses to `None`.
    ///
    /// Best-effort by design, like [`Client::get_json`]: no client, a refused
    /// connection, a 404 because the plugin isn't registered, a body whose
    /// shape moved - all `None`. Callers use this to *enrich* what they already
    /// know, so "the client couldn't tell us" must never become an error the
    /// user sees.
    pub fn ok(self) -> Option<E::Output> {
        let Self { path, request, .. } = self;

        let response = request
            .and_then(|r| r.send())
            .inspect_err(|e| tracing::debug!("{} {path} failed: {e}", E::METHOD))
            .ok()?;

        if !response.is_success() {
            tracing::debug!("{} {path} answered HTTP {}", E::METHOD, response.status());
            return None;
        }

        response
            .json()
            .inspect_err(|e| {
                tracing::warn!("{} {path} returned an unexpected shape: {e}", E::METHOD);
            })
            .ok()
    }

    /// For the routes that answer 204 with no body.
    ///
    /// What a `type Output = ()` endpoint calls: `()` is never decoded, because
    /// an empty body is not valid JSON for it. A non-success status is
    /// [`RequestError::Decode`], as in [`json`](Self::json).
    pub fn ignore(self) -> Result<(), RequestError> {
        let response = self.send()?;
        if !response.is_success() {
            return Err(RequestError::Decode(format!(
                "HTTP {}: {}",
                response.status(),
                response.body().trim()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::live_lockfile;

    const PENDING: Route = Route::new("product-launcher", 1, "is-launch-request-pending");
    const PATCHLINE: Route = Route::new(
        "product-launcher",
        1,
        "products/{productId}/patchlines/{patchlineId}",
    );

    /// The parameterless shape: the defaults are the whole implementation.
    struct IsPending;

    impl Endpoint for IsPending {
        type Output = bool;
        const METHOD: Method = Method::Get;
        const ROUTE: Route = PENDING;
    }

    /// Path parameters and a body, both overridden.
    struct Launch<'a> {
        product_id: &'a str,
        patchline_id: &'a str,
    }

    impl Endpoint for Launch<'_> {
        type Output = String;
        const METHOD: Method = Method::Post;
        const ROUTE: Route = PATCHLINE;

        fn path(&self) -> String {
            Self::ROUTE.bind(&[
                ("productId", self.product_id),
                ("patchlineId", self.patchline_id),
            ])
        }

        fn body(&self) -> Result<Option<String>, RequestError> {
            Ok(Some("{}".to_string()))
        }
    }

    /// A body that will not serialize, to prove the error defers to a finisher.
    struct BrokenBody;

    impl Endpoint for BrokenBody {
        type Output = ();
        const METHOD: Method = Method::Post;
        const ROUTE: Route = PENDING;

        fn body(&self) -> Result<Option<String>, RequestError> {
            Err(RequestError::Encode("refused to serialize".into()))
        }
    }

    #[test]
    fn the_default_path_and_body_come_from_the_route() {
        assert_eq!(
            IsPending.path(),
            "/product-launcher/v1/is-launch-request-pending"
        );
        assert!(IsPending.body().unwrap().is_none());
    }

    #[test]
    fn parameters_bind_and_the_body_encodes() {
        let launch = Launch {
            product_id: "league_of_legends",
            patchline_id: "live",
        };

        assert_eq!(
            launch.path(),
            "/product-launcher/v1/products/league_of_legends/patchlines/live"
        );
        assert_eq!(launch.body().unwrap().as_deref(), Some("{}"));
    }

    /// The point of associated consts: readable without building an instance.
    #[test]
    fn method_and_route_are_readable_without_an_instance() {
        assert_eq!(<IsPending as Endpoint>::METHOD, Method::Get);
        assert_eq!(<Launch<'_> as Endpoint>::METHOD, Method::Post);
        assert_eq!(<Launch<'_> as Endpoint>::ROUTE, PATCHLINE);
    }

    /// The deferred-error contract: a broken body never touches the wire and
    /// survives the chainable calls in between.
    #[test]
    fn a_body_error_surfaces_from_the_finisher() {
        let client = Client::new().unwrap();

        let outcome = client
            .endpoint(&BrokenBody)
            .timeout(Duration::from_secs(1))
            .retry(RetryPolicy::none())
            .send();

        assert!(matches!(outcome, Err(RequestError::Encode(_))));
    }

    /// Each finisher's shape for "nothing answered", mirroring the transport's
    /// own no-client test.
    #[test]
    fn without_a_live_client_every_finisher_answers_its_shape() {
        if live_lockfile().is_some() {
            return; // a real client is up on this machine; nothing to assert
        }
        let client = Client::new().unwrap();

        assert!(matches!(
            client.endpoint(&IsPending).send(),
            Err(RequestError::NoClient)
        ));
        assert!(matches!(
            client.endpoint(&IsPending).json(),
            Err(RequestError::NoClient)
        ));
        assert!(client.endpoint(&IsPending).ok().is_none());
        assert!(matches!(
            client.endpoint(&BrokenBody).ignore(),
            Err(RequestError::Encode(_))
        ));
    }
}
