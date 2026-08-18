//! The endpoints of `/riotclientapp`.

use ritoclient_core::endpoint::json_body;
use ritoclient_core::{Endpoint, EndpointMeta, Method, RequestError, Route};

use super::routes;

/// `POST /riotclientapp/v1/new-args` - hand a duplicate instance's argv to the
/// running client.
///
/// Its 204 acknowledges the arguments and reports nothing about what the client
/// does with them: on 136 a launch pair sent here launches, on 135 it did not.
/// Wake with an empty array and launch through
/// [`crate::namespaces::product_launcher`] - see the module docs for why the
/// difference is a cohort rather than a mistake.
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
