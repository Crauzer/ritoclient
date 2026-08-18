//! The endpoints of `/product-session`.

use std::collections::HashMap;

use ritoclient_core::{Endpoint, EndpointMeta, Method, Route};

use crate::models::product_session::Session;

use super::routes;

/// `GET /product-session/v1/external-sessions` - every session the client is
/// tracking outside itself. Games, not the UX product.
///
/// The body is an object keyed by session id, so it deserializes as a map
/// rather than a list. Probed 200 on 135.0.7.4760.
pub struct ExternalSessions;

impl Endpoint for ExternalSessions {
    type Output = HashMap<String, Session>;
    const METHOD: Method = Method::Get;
    const ROUTE: Route = routes::EXTERNAL_SESSIONS;
}

/// `GET /product-session/v1/external-sessions/{sessionId}` - one session by
/// the id a launch returned.
///
/// The id is the bare JSON string
/// [`product_launcher`](crate::namespaces::product_launcher) answers a launch
/// with.
pub struct ExternalSession<'a> {
    pub session_id: &'a str,
}

impl Endpoint for ExternalSession<'_> {
    type Output = Session;
    const METHOD: Method = Method::Get;
    const ROUTE: Route = routes::EXTERNAL_SESSION;

    fn path(&self) -> String {
        Self::ROUTE.bind(&[("sessionId", self.session_id)])
    }
}

/// Every endpoint this namespace declares, in declaration order.
pub const ALL: &[EndpointMeta] = &[
    EndpointMeta {
        name: "ExternalSessions",
        method: Method::Get,
        route: routes::EXTERNAL_SESSIONS,
    },
    EndpointMeta {
        name: "ExternalSession",
        method: Method::Get,
        route: routes::EXTERNAL_SESSION,
    },
];
