//! The routes of `/product-launcher`.

ritoclient_core::routes! {
    namespace = "product-launcher";

    /// `POST /product-launcher/v1/products/{productId}/patchlines/{patchlineId}`
    /// - launch this product/patchline. The route the client's own Play button
    /// calls.
    PATCHLINE = 1, "products/{productId}/patchlines/{patchlineId}";

    /// `GET .../{patchlineId}/eligibility` - whether the account is entitled to
    /// launch this product/patchline. Entitlement, not installed-ness.
    ELIGIBILITY = 1, "products/{productId}/patchlines/{patchlineId}/eligibility";

    /// `GET /product-launcher/v1/is-launch-request-pending` - whether a launch
    /// is already in flight.
    IS_LAUNCH_REQUEST_PENDING = 1, "is-launch-request-pending";
}
