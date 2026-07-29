//! Data types for `/rnet-product-registry`.
//!
//! The re-exports below are the public names; the definitions live in
//! [`super::flat`]. Behaviour lives above this crate - the `ritoclient` crate's
//! `ProductExt` / `PatchlineExt` extension traits.

pub use super::flat::{
    RnetProductRegistryPatchline as Patchline, RnetProductRegistryProduct as Product,
    RnetProductRegistrySecondaryPatchline as SecondaryPatchline,
};
