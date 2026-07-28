//! The data the Riot Client's API carries.
//!
//! Split in two:
//!
//! - [`flat`] is **storage** - private, one flat namespace, every type under the
//!   client's own qualified name. It is generator output.
//! - The modules beside it are the **API** - public, grouped by the namespace
//!   that uses the type, re-exporting from `flat` under ergonomic names.
//!
//! So `models::product_registry::Product` is the public name of
//! `flat::RnetProductRegistryProduct`, and a type used by three namespaces is
//! defined once and re-exported three times rather than duplicated or arbitrarily
//! assigned an owner.
//!
//! Hand-written behaviour - the methods that turn a wire record into something
//! worth calling - lives in the grouping modules as `impl` blocks. Rust allows
//! implementing a type from anywhere in its own crate, so definitions and
//! behaviour can have different owners: regenerating [`flat`] cannot touch the
//! methods, and adding a method never risks a merge conflict with the generator.
//!
//! Types that belong to no namespace in particular live in [`crate::types`].

mod flat;

pub mod product_registry;
