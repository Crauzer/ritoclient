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
//! Behaviour does not live in this crate at all. The methods that turn a wire
//! record into something worth calling - `is_installed`, `secondary_dir` - are
//! extension traits in the `ritoclient` crate, re-exported from its prelude: an
//! inherent `impl` must live in the crate defining the type, and hand-written
//! code does not belong in this one.
//!
//! Types that belong to no namespace in particular live in
//! [`ritoclient_core::types`].

mod flat;

pub mod product_registry;
pub mod product_session;
