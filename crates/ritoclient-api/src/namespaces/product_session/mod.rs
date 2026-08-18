//! `/product-session/v1` - the client's own bookkeeping for what it started.
//!
//! A launch answers with a session id, and this is what that id is a key into.
//! That makes it the honest answer to "did the game actually start?", where a
//! process-name match is a guess that happens to be right: this is the record
//! the Riot Client itself keeps, so it carries a phase, a version, and - when
//! the session is over - an exit code and a reason for it.
//!
//! *External* sessions are the ones outside the Riot Client: games, not the UX
//! product. That is the whole set a launcher cares about.
//!
//! Only the two read routes are wrapped. The write side of this namespace
//! (`/sessions`, the heartbeats, the host session) belongs to a running game
//! talking to its own client, not to us. And
//! [`models::product_session::Session`](crate::models::product_session::Session)
//! is a deliberate subset - see its docs for the field that is missing and why.

pub mod endpoints;
pub mod routes;

use std::collections::HashMap;

use ritoclient_core::client::Client;

use crate::models::product_session::Session;

/// The `/product-session/v1` namespace. Obtained from
/// [`ClientExt::product_session`](crate::ClientExt::product_session).
pub struct ProductSessionHandler<'a> {
    client: &'a Client,
}

impl<'a> ProductSessionHandler<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// One session by the id a launch returned.
    ///
    /// `None` covers both "no such session" and "could not ask", which is the
    /// right shape for a caller polling just after a launch: the session does not
    /// exist for the moment between the client accepting the request and opening
    /// the session, and neither case is an error.
    pub fn external_session(&self, session_id: &str) -> Option<Session> {
        self.client
            .endpoint(&endpoints::ExternalSession { session_id })
            .ok()
    }

    /// Every session the client is tracking outside itself, keyed by session id.
    ///
    /// `None` means the client could not answer - closed, tray-idle, or a shape we
    /// no longer recognise. An empty map is a different fact: the client answered,
    /// and nothing is running.
    pub fn external_sessions(&self) -> Option<HashMap<String, Session>> {
        self.client.endpoint(&endpoints::ExternalSessions).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ritoclient_core::Endpoint;

    #[test]
    fn the_session_route_binds_its_id() {
        let session = endpoints::ExternalSession {
            session_id: "irnZWC1kOMt",
        };
        assert_eq!(
            session.path(),
            "/product-session/v1/external-sessions/irnZWC1kOMt"
        );
    }
}
