//! The routes of `/product-session`.

ritoclient_core::routes! {
    namespace = "product-session";

    /// `GET /product-session/v1/external-sessions` - every session the client is
    /// tracking outside itself. Games, not the UX product.
    EXTERNAL_SESSIONS = 1, "external-sessions";

    /// `GET /product-session/v1/external-sessions/{sessionId}` - one session by
    /// the id a launch returned.
    EXTERNAL_SESSION = 1, "external-sessions/{sessionId}";
}
