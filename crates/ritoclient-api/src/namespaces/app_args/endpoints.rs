//! The endpoints of `/riotclientapp`.

use ritoclient_core::endpoint::json_body;
use ritoclient_core::{Endpoint, EndpointMeta, Method, RequestError, Route};

use super::routes;

/// `POST /riotclientapp/v1/new-args` - hand a duplicate instance's argv to the
/// running client.
///
/// Its 204 means "arguments queued", not "launched". See the module docs.
///
/// The client's argument convention applies: the body is the bare array
/// (`["--flag"]`), not an object wrapping it.
pub struct NewArgs<'a> {
    pub args: &'a [String],
}

impl Endpoint for NewArgs<'_> {
    type Output = ();
    const METHOD: Method = Method::Post;
    const ROUTE: Route = routes::NEW_ARGS;

    fn body(&self) -> Result<Option<String>, RequestError> {
        json_body(self.args)
    }
}

/// Every endpoint this namespace declares, in declaration order.
pub const ALL: &[EndpointMeta] = &[EndpointMeta {
    name: "NewArgs",
    method: Method::Post,
    route: routes::NEW_ARGS,
}];
