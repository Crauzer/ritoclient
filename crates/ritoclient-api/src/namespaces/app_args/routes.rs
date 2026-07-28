//! The routes of `/riotclientapp`.

crate::routes! {
    namespace = "riotclientapp";

    /// `POST /riotclientapp/v1/new-args` - hand a duplicate instance's argv to
    /// the running client.
    ///
    /// Its 204 means "arguments queued", not "launched". See the module docs.
    NEW_ARGS = 1, "new-args";
}
