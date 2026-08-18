//! The endpoints of `/product-launcher`.

use ritoclient_core::endpoint::json_body;
use ritoclient_core::{Endpoint, EndpointMeta, Method, RequestError, Route};

use super::routes;

/// `POST /product-launcher/v1/products/{productId}/patchlines/{patchlineId}` -
/// launch this product/patchline. The route the client's own Play button calls.
///
/// On success the body is a bare JSON string holding the session id the client
/// minted - the key into `/product-session/v1/external-sessions`.
pub struct Patchline<'a> {
    pub product_id: &'a str,
    pub patchline_id: &'a str,
}

impl Endpoint for Patchline<'_> {
    type Output = String;
    const METHOD: Method = Method::Post;
    const ROUTE: Route = routes::PATCHLINE;

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

/// `GET .../{patchlineId}/eligibility` - whether the account is entitled to
/// launch this product/patchline. Entitlement, not installed-ness.
pub struct Eligibility<'a> {
    pub product_id: &'a str,
    pub patchline_id: &'a str,
}

impl Endpoint for Eligibility<'_> {
    type Output = bool;
    const METHOD: Method = Method::Get;
    const ROUTE: Route = routes::ELIGIBILITY;

    fn path(&self) -> String {
        Self::ROUTE.bind(&[
            ("productId", self.product_id),
            ("patchlineId", self.patchline_id),
        ])
    }
}

/// `DELETE /product-launcher/v1/products/{productId}/patchlines/{patchlineId}` -
/// close the product the client launched. Measured: 204, and League gone in
/// under six seconds.
///
/// **No body.** The route takes an optional `shouldTerminateProcess` that is
/// not modelled: it is the one argument on this namespace we have not seen on
/// the wire, and whether it belongs in a body or a query string is unconfirmed.
/// Sending nothing leaves the client on whatever it defaults to, which is the
/// behaviour that was measured.
pub struct Close<'a> {
    pub product_id: &'a str,
    pub patchline_id: &'a str,
}

impl Endpoint for Close<'_> {
    type Output = ();
    const METHOD: Method = Method::Delete;
    const ROUTE: Route = routes::PATCHLINE;

    fn path(&self) -> String {
        Self::ROUTE.bind(&[
            ("productId", self.product_id),
            ("patchlineId", self.patchline_id),
        ])
    }
}

/// `PUT /product-launcher/v1/products/{productId}/patchlines/{patchlineId}` -
/// take ownership of a game that is already running.
///
/// The client's own words: *"Recover a session for a product that is already
/// running, but Riot Client Services doesn't know about since it just started
/// up."* Answers the same bare JSON string a launch does - a session id.
///
/// The pid rides in the body as a bare JSON number, which is this client's
/// argument convention for a single non-path argument. **Unconfirmed against a
/// live client**: the other reading is a query parameter, and nothing in the
/// workspace models those yet.
pub struct Adopt<'a> {
    pub product_id: &'a str,
    pub patchline_id: &'a str,
    pub pid: i32,
}

impl Endpoint for Adopt<'_> {
    type Output = String;
    const METHOD: Method = Method::Put;
    const ROUTE: Route = routes::PATCHLINE;

    fn path(&self) -> String {
        Self::ROUTE.bind(&[
            ("productId", self.product_id),
            ("patchlineId", self.patchline_id),
        ])
    }

    fn body(&self) -> Result<Option<String>, RequestError> {
        json_body(&self.pid)
    }
}

/// `GET /product-launcher/v1/is-launch-request-pending` - whether a launch is
/// already in flight.
pub struct IsLaunchRequestPending;

impl Endpoint for IsLaunchRequestPending {
    type Output = bool;
    const METHOD: Method = Method::Get;
    const ROUTE: Route = routes::IS_LAUNCH_REQUEST_PENDING;
}

/// Every endpoint this namespace declares, in declaration order.
pub const ALL: &[EndpointMeta] = &[
    EndpointMeta {
        name: "Patchline",
        method: Method::Post,
        route: routes::PATCHLINE,
    },
    EndpointMeta {
        name: "Eligibility",
        method: Method::Get,
        route: routes::ELIGIBILITY,
    },
    EndpointMeta {
        name: "Close",
        method: Method::Delete,
        route: routes::PATCHLINE,
    },
    EndpointMeta {
        name: "Adopt",
        method: Method::Put,
        route: routes::PATCHLINE,
    },
    EndpointMeta {
        name: "IsLaunchRequestPending",
        method: Method::Get,
        route: routes::IS_LAUNCH_REQUEST_PENDING,
    },
];
