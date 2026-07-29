//! The endpoints of `/riot-client-lifecycle`.

use ritoclient_core::{Endpoint, EndpointMeta, Method, RequestError, Route};

use super::routes;

/// `POST /riot-client-lifecycle/v1/hide` - "Hide the UX."
///
/// Answers 204 with no body.
pub struct Hide;

impl Endpoint for Hide {
    type Output = ();
    const METHOD: Method = Method::Post;
    const ROUTE: Route = routes::HIDE;

    fn body(&self) -> Result<Option<String>, RequestError> {
        Ok(Some(String::new()))
    }
}

/// `POST /riot-client-lifecycle/v1/show` - "Show the UX."
///
/// Answers 204 with no body.
pub struct Show;

impl Endpoint for Show {
    type Output = ();
    const METHOD: Method = Method::Post;
    const ROUTE: Route = routes::SHOW;

    fn body(&self) -> Result<Option<String>, RequestError> {
        Ok(Some(String::new()))
    }
}

/// Every endpoint this namespace declares, in declaration order.
pub const ALL: &[EndpointMeta] = &[
    EndpointMeta {
        name: "Hide",
        method: Method::Post,
        route: routes::HIDE,
    },
    EndpointMeta {
        name: "Show",
        method: Method::Post,
        route: routes::SHOW,
    },
];
