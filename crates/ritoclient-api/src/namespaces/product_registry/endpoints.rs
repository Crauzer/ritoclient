//! The endpoints of `/rnet-product-registry`.

use ritoclient_core::{Endpoint, EndpointMeta, Method, Route};

use crate::models::product_registry::Product;

use super::routes;

/// `GET /rnet-product-registry/v4/products` - every product the client knows
/// about, with its patchlines.
pub struct Products;

impl Endpoint for Products {
    type Output = Vec<Product>;
    const METHOD: Method = Method::Get;
    const ROUTE: Route = routes::PRODUCTS;
}

/// Every endpoint this namespace declares, in declaration order.
pub const ALL: &[EndpointMeta] = &[EndpointMeta {
    name: "Products",
    method: Method::Get,
    route: routes::PRODUCTS,
}];
