//! `/rnet-product-registry/v4` - what is installed, and where.
//!
//! One call answers what would otherwise be guesswork: where a product lives,
//! which subdirectory holds its game data, which patchlines are installed, and
//! which content release is on disk.
//!
//! It is worth knowing *why* these answers are authoritative rather than merely
//! convenient. Foundation reads this same registry to build a game's command
//! line - `executable = PathJoin(install_full_path, primary_executable)`,
//! `workingDir = install_full_path`. So `install_full_path` is not a path Riot
//! happens to know about; it is by construction the directory the client will
//! launch the game from.
//!
//! # Caveats
//!
//! - **The namespace is absent from swagger and split across `v1`/`v4`.** It
//!   will churn. Everything here deserializes tolerantly: unknown keys are
//!   ignored, missing keys default, and any failure is `None` rather than an
//!   error. Callers use this to enrich what they already know.
//! - **It is plugin-gated.** A tray-idle client's entire API surface collapses
//!   to the argv handoff (see [`crate::namespaces::app_args`]), so this 404s in that state
//!   and does not exist at all when the client is closed.

pub mod routes;

use crate::client::Client;
use crate::models::product_registry::Product;

use routes::PRODUCTS;

/// The `/rnet-product-registry/v4` namespace. Obtained from
/// [`Client::product_registry`].
pub struct ProductRegistryHandler<'a> {
    client: &'a Client,
}

impl<'a> ProductRegistryHandler<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// `GET /rnet-product-registry/v4/products`.
    ///
    /// `None` means the client could not answer - closed, tray-idle, or a shape
    /// we no longer recognise. Never an error: every caller has a fallback.
    pub fn products(&self) -> Option<Vec<Product>> {
        let products: Vec<Product> = self.client.get_json(PRODUCTS)?;
        tracing::debug!("Product registry returned {} product(s)", products.len());
        Some(products)
    }

    /// One product's record, by id.
    pub fn product(&self, id: &str) -> Option<Product> {
        self.products()?.into_iter().find(|p| p.id == id)
    }
}
