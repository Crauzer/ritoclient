//! Transport for the Riot Client's local loopback API.
//!
//! The Riot Client runs a loopback HTTPS server whose port and password live in
//! a lockfile only the current user can read. This crate is the mechanism for
//! talking to it - the connection, the retry policy, the route and endpoint
//! vocabulary, and the on-disk lockfile - and nothing else. It knows no
//! namespace, models no payload, and launches nothing.
//!
//! The typed API surface lives in `ritoclient-api`, which is generated; the
//! launch and session orchestration lives in `ritoclient`, which is the crate
//! most consumers want. Both are built on this one.
//!
//! # A status is an answer, not an error
//!
//! [`RequestError`] means no round trip happened: nothing was listening, the
//! connection failed, a body would not encode. Every status the client answers
//! with arrives as a [`Response`], 404 and 5xx included, because the same status
//! means different things on different routes and the caller knows which route
//! it asked.
//!
//! # The wire vocabulary is this crate's own
//!
//! [`Method`] and [`StatusCode`] are defined here rather than re-exported from
//! an HTTP library, so the HTTP client is an implementation detail of
//! [`client`] - swapping it touches one file, not the API surface built on top.
//!
//! [`endpoint`] is the operation layer: an [`Endpoint`] is a value carrying its
//! verb, route, parameters and output type, driven through
//! [`Client::endpoint`]. The generated crate above declares ~150 of them; the
//! trait and its combinators are written once, here.

pub mod client;
pub mod endpoint;
pub mod lockfile;
pub mod processes;
pub mod retry;
pub mod route;
pub mod types;

pub use client::{Client, ClientBuilder, Method, Presence, RequestError, Response, StatusCode};
pub use endpoint::{Endpoint, EndpointBuilder, EndpointMeta};
pub use lockfile::{Lockfile, default_lockfile_path, live_lockfile};
pub use retry::{Backoff, RetryPolicy};
pub use route::Route;
pub use types::RiotError;
