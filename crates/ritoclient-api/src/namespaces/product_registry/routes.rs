//! The routes of `/rnet-product-registry`.

ritoclient_core::routes! {
    namespace = "rnet-product-registry";

    /// `GET /rnet-product-registry/v4/products` - every product the client
    /// knows about, with its patchlines.
    PRODUCTS = 4, "products";
}
