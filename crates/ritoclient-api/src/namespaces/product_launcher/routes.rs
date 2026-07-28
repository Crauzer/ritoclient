//! The routes of `/product-launcher`, and the parameters they bind.

use crate::launch::LaunchTarget;

crate::routes! {
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

/// The path parameters every product-scoped route in this namespace takes.
pub(crate) fn target_params(target: &LaunchTarget) -> [(&'static str, &str); 2] {
    [
        ("productId", target.product_id.as_str()),
        ("patchlineId", target.patchline_id.as_str()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_routes_carry_both_ids() {
        let target = LaunchTarget {
            product_id: "league_of_legends".to_string(),
            patchline_id: "pbe".to_string(),
        };

        assert_eq!(
            PATCHLINE.bind(&target_params(&target)),
            "/product-launcher/v1/products/league_of_legends/patchlines/pbe"
        );
        assert_eq!(
            ELIGIBILITY.bind(&target_params(&target)),
            "/product-launcher/v1/products/league_of_legends/patchlines/pbe/eligibility"
        );
    }
}
