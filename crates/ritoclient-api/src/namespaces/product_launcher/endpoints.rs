//! The endpoints of `/product-launcher`.

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
        name: "IsLaunchRequestPending",
        method: Method::Get,
        route: routes::IS_LAUNCH_REQUEST_PENDING,
    },
];
