//! Typed namespaces and models for the Riot Client's local API.
//!
//! This crate is the generated layer between `ritoclient-core` (the transport)
//! and `ritoclient` (the launch orchestration and facade). It wraps the API
//! namespaces the workspace models - per namespace a handler, a route table,
//! and an endpoint table, plus the data types they carry - and nothing else:
//! no loops, no sleeps, no OS calls, and no opinion about what a status
//! *means*, because all of those are judgements the layers above make.
//!
//! Every operation is a value implementing
//! [`Endpoint`](ritoclient_core::Endpoint); the handlers are thin sugar that
//! construct the endpoint and pick a finisher. [`namespaces::endpoints`]
//! enumerates them all as plain data, verb included.
//!
//! Handlers are reached through [`ClientExt`], which hangs them off a
//! [`Client`](ritoclient_core::Client):
//!
//! ```no_run
//! use ritoclient_api::ClientExt;
//! use ritoclient_core::Client;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Client::new()?;
//! let products = client.product_registry().products();
//! # Ok(())
//! # }
//! ```
//!
//! # Destined to be generator output
//!
//! Every file in this crate other than `Cargo.toml` is slated to be written by
//! `cargo xtask ritoclient-codegen` from a checked-in schema snapshot, and the
//! hand-written namespaces here are the shape that generator must reproduce.
//! The dependency list is the boundary's enforcement: nothing here can reach a
//! launcher type, a sleep, or Win32, because no dependency provides one.

// Several modules explain their own privacy - `models::flat` is private on
// purpose, and the prose says so - and naming the private item is the clearest
// way to write that. Rustdoc renders these as plain code rather than links,
// which is the intended result.
#![allow(rustdoc::private_intra_doc_links)]

pub mod models;
pub mod namespaces;

pub use namespaces::ClientExt;
